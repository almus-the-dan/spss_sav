//! Schema of variables in a SAV file, and the accumulator that builds
//! one.
//!
//! Reconciliation is a two-stage affair, and the order is forced by the
//! format rather than chosen. Extension records sit at the *end* of the
//! dictionary, after the variable records they patch, so nothing can be
//! applied as it arrives — the crate-internal `SavSchemaBuilder` collects
//! the records and folds them together when the dictionary phase
//! finalizes.
//!
//! Within that fold the order still matters. Subtypes 13 and 14 key off
//! the **short** name; subtypes 18, 21 and 22 key off the **long** one.
//! So long names have to be established first, or the later three have
//! nothing to match against.
//!
//! Every mismatch warns and drops the offending patch. A dictionary that
//! disagrees with its own extension records is common enough in files
//! from non-SPSS writers that failing the read would reject data PSPP
//! opens without complaint.

use std::collections::HashMap;
use std::rc::Rc;

use crate::spss::sav::alignment::Alignment;
use crate::spss::sav::data_layout::DataLayout;
use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
use crate::spss::sav::extensions::float_sentinels::FloatSentinels;
use crate::spss::sav::extensions::long_missing_values::LongMissingValues;
use crate::spss::sav::extensions::long_value_labels::LongValueLabels;
use crate::spss::sav::extensions::long_variable_names::LongVariableNames;
use crate::spss::sav::extensions::raw_display_parameters::RawDisplayParameters;
use crate::spss::sav::extensions::variable_attributes::VariableAttributes;
use crate::spss::sav::extensions::variable_display::VariableDisplay;
use crate::spss::sav::float_encoding::FloatEncoding;
use crate::spss::sav::measurement_level::MeasurementLevel;
use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::raw_value_label_set::RawValueLabelSet;
use crate::spss::sav::sav_variable::{SavVariable, SavVariableBuilder};
use crate::spss::sav::sav_variable_header::SavVariableHeader;
use crate::spss::sav::sav_warning::SavWarning;
use crate::spss::sav::value_label_entry::ValueLabelEntry;
use crate::spss::sav::value_label_set::ValueLabelSet;
use crate::spss::sav::value_label_value::ValueLabelValue;
use crate::spss::sav::variable_attribute::VariableAttribute;
use crate::spss::sav::variable_type::VariableType;
use crate::spss::sav::{missing_value_reconcile, variable_attribute_reconcile};

/// The set of variables in a SAV file, in declaration order.
///
/// `SavSchema` is the presentation half of what the dictionary
/// describes — names, labels, value labels, display parameters,
/// attributes. The record reader does not consult it; it reads rows
/// through a separate, always-complete layout, which is why a caller
/// can turn schema building off entirely with
/// [`SavReader::build_schema`](crate::spss::sav::sav_reader::SavReader::build_schema)
/// without affecting how the data reads.
///
/// Very-long-string segments are collapsed: a variable declared `A300`
/// appears once, at width 300, not as the 255-byte and 48-byte segments
/// the wire level yields.
///
/// It has no public constructor; users only ever get one from
/// [`RecordReader::schema`](crate::spss::sav::record_reader::RecordReader::schema)
/// after the dictionary phase has finalized.
///
/// Value-label sets are shared between the variables a single
/// type-3 / type-4 pair covered, so `SavSchema` is `Clone` but neither
/// `Send` nor `Sync`.
#[derive(Debug, Clone)]
pub struct SavSchema {
    variables: Vec<SavVariable>,
    /// Index into `variables` of the weight variable, if the file
    /// declared one that resolves.
    weight: Option<usize>,
}

impl SavSchema {
    pub(crate) fn new(variables: Vec<SavVariable>, weight: Option<usize>) -> Self {
        Self { variables, weight }
    }

    /// All variables in declaration order.
    #[must_use]
    #[inline]
    pub fn variables(&self) -> &[SavVariable] {
        &self.variables
    }

    /// Number of variables.
    #[must_use]
    #[inline]
    pub fn variable_count(&self) -> usize {
        self.variables.len()
    }

    /// The variable at 0-based `index`, matching
    /// [`SavVariable::index`].
    #[must_use]
    #[inline]
    pub fn variable(&self, index: usize) -> Option<&SavVariable> {
        self.variables.get(index)
    }

    /// The 0-based index of the variable named `name`.
    ///
    /// Matched case-insensitively against the long name where one was
    /// declared and the short name otherwise — the same name
    /// [`SavVariable::full_name`] reports, and the same
    /// case-insensitive rule SPSS itself applies.
    ///
    /// The first declaration wins if a malformed file names two
    /// variables the same, matching how reconciliation resolves the
    /// names that extension records key off.
    #[must_use]
    pub fn variable_index(&self, name: &str) -> Option<usize> {
        self.variables
            .iter()
            .position(|variable| variable.full_name().eq_ignore_ascii_case(name))
    }

    /// The variable named `name`, or `None` when the schema holds no
    /// such variable. See [`variable_index`](Self::variable_index) for
    /// how names are matched.
    #[must_use]
    #[inline]
    pub fn variable_by_name(&self, name: &str) -> Option<&SavVariable> {
        self.variable(self.variable_index(name)?)
    }

    /// The variable SPSS weights cases by, if the file declared one.
    ///
    /// The file stores this in its 176-byte header, but as an offset
    /// into the data row rather than as a name. Which variable it
    /// picks out is only knowable once the dictionary has been walked
    /// and its variables reconciled. It is reported here rather than on
    /// [`SavHeader`](crate::spss::sav::sav_header::SavHeader) because
    /// the thing it names is a property of the variable set. SPSS
    /// spells it `WEIGHT BY`, and `APPLY DICTIONARY` carries it with the
    /// variable labels and missing values.
    ///
    /// `None` when the file declared no weight, and also when the offset
    /// it declared picks out no variable — a continuation record, or a
    /// position past the end.
    #[must_use]
    #[inline]
    pub fn weight_variable(&self) -> Option<&SavVariable> {
        self.variable(self.weight?)
    }
}

/// A subtype-21 record reduced to what reconciliation needs: the
/// variable's long name and its `(full-width key, label)` pairs.
type LongValueLabelEntries = (String, Vec<(Box<[u8]>, String)>);

/// Collects dictionary records and folds them into a [`SavSchema`].
///
/// Fed the same records the caller sees, in the same order, before they
/// are handed out. Skipped and un-pulled records reach it too, so the
/// schema does not depend on how the caller drives the reader.
#[derive(Debug, Default)]
pub(crate) struct SavSchemaBuilder {
    /// One entry per segment (type-2 primary), in declaration order.
    segments: Vec<SavVariableHeader>,
    value_label_sets: Vec<RawValueLabelSet>,
    long_names: Vec<(String, String)>,
    display: Option<RawDisplayParameters>,
    attributes: Vec<(String, Vec<VariableAttribute>)>,
    long_value_labels: Vec<LongValueLabelEntries>,
    long_missing_values: Vec<(String, Vec<Box<[u8]>>)>,
}

impl SavSchemaBuilder {
    /// Records one type-2 primary record.
    pub fn push_variable(&mut self, header: &SavVariableHeader) {
        self.segments.push(header.clone());
    }

    /// Records one type-3 / type-4 value-label pair.
    pub fn push_value_labels(&mut self, set: &RawValueLabelSet) {
        self.value_label_sets.push(set.clone());
    }

    /// Records the subtype-13 short-to-long name mappings.
    pub fn set_long_names(&mut self, names: &LongVariableNames) {
        self.long_names = names
            .mappings()
            .iter()
            .map(|mapping| {
                (
                    mapping.short_name().to_owned(),
                    mapping.long_name().to_owned(),
                )
            })
            .collect();
    }

    /// Records the subtype-11 display parameters, unsliced.
    pub fn set_display_parameters(&mut self, parameters: &RawDisplayParameters) {
        self.display = Some(parameters.clone());
    }

    /// Records the subtype-18 per-variable attributes, collapsing any
    /// `name[n]` runs into single array-valued attributes.
    pub fn set_variable_attributes(&mut self, attributes: &VariableAttributes) {
        self.attributes = attributes
            .records()
            .iter()
            .map(|record| {
                let collapsed = variable_attribute_reconcile::collapse(record.attributes());
                (record.variable_name().to_owned(), collapsed)
            })
            .collect();
    }

    /// Records the subtype-21 long-string value labels.
    pub fn set_long_value_labels(&mut self, labels: &LongValueLabels) {
        self.long_value_labels = labels
            .records()
            .iter()
            .map(|record| {
                let entries = record
                    .labels()
                    .iter()
                    .map(|label| {
                        (
                            label.value().to_vec().into_boxed_slice(),
                            label.label().to_owned(),
                        )
                    })
                    .collect();
                (record.variable_name().to_owned(), entries)
            })
            .collect();
    }

    /// Records the subtype-22 long-string missing values.
    pub fn set_long_missing_values(&mut self, values: &LongMissingValues) {
        self.long_missing_values = values
            .records()
            .iter()
            .map(|record| {
                let entries = record
                    .values()
                    .iter()
                    .map(|value| value.clone().into_boxed_slice())
                    .collect();
                (record.variable_name().to_owned(), entries)
            })
            .collect();
    }

    /// Folds everything collected into a schema, using `layout` for the
    /// segment-to-variable grouping so the two agree by construction.
    pub fn build(
        self,
        layout: &DataLayout,
        float_encoding: FloatEncoding,
        sentinels: &FloatSentinels,
        weight: Option<usize>,
        warnings: &mut Vec<SavWarning>,
    ) -> SavSchema {
        // Which segment each logical variable starts at. The layout
        // already did the very-long-string grouping; reusing it is what
        // keeps the schema and the row reader from ever disagreeing
        // about how many variables there are.
        let starts = segment_starts(layout);

        let mut builders: Vec<SavVariableBuilder> = starts
            .iter()
            .enumerate()
            .map(|(index, &start)| {
                let header = &self.segments[start];
                let variable_type = layout.variables()[index].variable_type();
                base_builder(header, variable_type, index, float_encoding, sentinels)
            })
            .collect();

        // Long names first: subtypes 18, 21 and 22 match against them.
        self.apply_long_names(&mut builders, &starts, warnings);
        let names = NameIndex::build(&builders);

        self.apply_display(&mut builders, &starts, warnings);
        self.apply_value_labels(&mut builders, &starts, layout, warnings);
        self.apply_attributes(&mut builders, &names, warnings);
        self.apply_long_value_labels(&mut builders, &names, warnings);
        self.apply_long_missing_values(&mut builders, &names, warnings);

        let variables: Vec<SavVariable> = builders
            .into_iter()
            .map(SavVariableBuilder::build)
            .collect();
        let weight = weight.filter(|index| *index < variables.len());
        SavSchema::new(variables, weight)
    }

    /// Patches long names on, keyed by short name.
    fn apply_long_names(
        &self,
        builders: &mut [SavVariableBuilder],
        starts: &[usize],
        warnings: &mut Vec<SavWarning>,
    ) {
        for (short_name, long_name) in &self.long_names {
            let found = starts.iter().position(|&start| {
                self.segments[start]
                    .short_name()
                    .eq_ignore_ascii_case(short_name)
            });
            let Some(index) = found else {
                let warning = unknown_variable(ExtensionSubtype::LongVariableNames, short_name);
                warnings.push(warning);
                continue;
            };
            replace(builders, index, |builder| builder.long_name(long_name));
        }
    }

    /// Slices the subtype-11 payload per segment and attaches the tuple
    /// belonging to each variable's first segment.
    ///
    /// The payload is `2` or `3` values per **segment**, not per
    /// variable — SPSS writes a tuple for every very-long-string
    /// segment — so the stride is derived from the segment count and
    /// the leading segment's tuple is the one that describes the
    /// variable.
    fn apply_display(
        &self,
        builders: &mut [SavVariableBuilder],
        starts: &[usize],
        warnings: &mut Vec<SavWarning>,
    ) {
        let Some(parameters) = &self.display else {
            return;
        };
        let values = parameters.values();
        let segment_count = self.segments.len();
        let Some(stride) = tuple_stride(values.len(), segment_count) else {
            let warning = SavWarning::DisplayParameterCountMismatch {
                element_count: u32::try_from(values.len()).unwrap_or(u32::MAX),
                segment_count: u32::try_from(segment_count).unwrap_or(u32::MAX),
            };
            warnings.push(warning);
            return;
        };

        for (index, &start) in starts.iter().enumerate() {
            let offset = start * stride;
            let tuple = &values[offset..offset + stride];
            let display = display_from_tuple(tuple, warnings, index);
            replace(builders, index, |builder| builder.display(display));
        }
    }

    /// Attaches value-label sets, sharing one allocation across every
    /// variable a type-3 / type-4 pair named.
    fn apply_value_labels(
        &self,
        builders: &mut [SavVariableBuilder],
        starts: &[usize],
        layout: &DataLayout,
        warnings: &mut Vec<SavWarning>,
    ) {
        for set in &self.value_label_sets {
            let targets: Vec<usize> = set
                .segment_indices()
                .iter()
                .filter_map(|&segment| {
                    let segment = usize::try_from(segment).ok()?;
                    // A segment that does not *start* a variable is an
                    // interior very-long-string segment. SPSS puts
                    // those labels in subtype 21, and an eight-byte key
                    // could not match a long value anyway.
                    let Ok(index) = starts.binary_search(&segment) else {
                        let warning = SavWarning::ValueLabelOnVeryLongString {
                            segment_index: u32::try_from(segment).unwrap_or(u32::MAX),
                        };
                        warnings.push(warning);
                        return None;
                    };
                    Some(index)
                })
                .collect();
            if targets.is_empty() {
                continue;
            }

            let shared = Rc::new(Self::typed_set(set, &targets, layout));
            for index in targets {
                let shared = Rc::clone(&shared);
                replace(builders, index, |builder| {
                    builder.value_labels(Rc::clone(&shared))
                });
            }
        }
    }

    /// Turns the wire-level eight-byte keys into typed ones, reading
    /// them as numeric or string according to the variables they apply
    /// to.
    fn typed_set(set: &RawValueLabelSet, targets: &[usize], layout: &DataLayout) -> ValueLabelSet {
        let numeric = targets.iter().all(|&index| {
            matches!(
                layout.variables()[index].variable_type(),
                VariableType::Numeric
            )
        });
        let entries = set
            .entries()
            .iter()
            .map(|entry| {
                let bytes = entry.value();
                let value = if numeric {
                    ValueLabelValue::Numeric(layout.float_encoding().decode(bytes))
                } else {
                    ValueLabelValue::String(bytes)
                };
                ValueLabelEntry::new(value, entry.label().to_owned())
            })
            .collect();
        ValueLabelSet::new(entries)
    }

    fn apply_attributes(
        &self,
        builders: &mut [SavVariableBuilder],
        names: &NameIndex,
        warnings: &mut Vec<SavWarning>,
    ) {
        for (name, attributes) in &self.attributes {
            let Some(index) = names.lookup(name) else {
                let warning = unknown_variable(ExtensionSubtype::VariableAttributes, name);
                warnings.push(warning);
                continue;
            };
            replace(builders, index, |builder| {
                builder.attributes(attributes.clone())
            });
        }
    }

    /// Attaches subtype-21 labels, whose keys carry the variable's full
    /// declared width rather than the eight bytes a type-3 record uses.
    fn apply_long_value_labels(
        &self,
        builders: &mut [SavVariableBuilder],
        names: &NameIndex,
        warnings: &mut Vec<SavWarning>,
    ) {
        for (name, labels) in &self.long_value_labels {
            let Some(index) = names.lookup(name) else {
                let warning = unknown_variable(ExtensionSubtype::LongValueLabels, name);
                warnings.push(warning);
                continue;
            };
            let entries = labels
                .iter()
                .map(|(value, label)| {
                    let long_string = ValueLabelValue::LongString(value.clone());
                    ValueLabelEntry::new(long_string, label.clone())
                })
                .collect();
            let shared = Rc::new(ValueLabelSet::new(entries));
            replace(builders, index, |builder| {
                builder.value_labels(Rc::clone(&shared))
            });
        }
    }

    fn apply_long_missing_values(
        &self,
        builders: &mut [SavVariableBuilder],
        names: &NameIndex,
        warnings: &mut Vec<SavWarning>,
    ) {
        for (name, values) in &self.long_missing_values {
            let Some(index) = names.lookup(name) else {
                let warning = unknown_variable(ExtensionSubtype::LongMissingValues, name);
                warnings.push(warning);
                continue;
            };
            let spec = MissingValueSpecification::String(values.clone());
            replace(builders, index, |builder| {
                builder.missing_value_spec(spec.clone())
            });
        }
    }
}

/// Case-insensitive lookup from a variable's full name to its index,
/// built after long names are patched on.
struct NameIndex {
    by_name: HashMap<String, usize>,
}

impl NameIndex {
    fn build(builders: &[SavVariableBuilder]) -> Self {
        let mut by_name = HashMap::with_capacity(builders.len());
        for (index, builder) in builders.iter().enumerate() {
            // First declaration wins, so a duplicated name resolves the
            // way a sequential scan would.
            by_name
                .entry(builder.full_name().to_ascii_lowercase())
                .or_insert(index);
        }
        Self { by_name }
    }

    fn lookup(&self, name: &str) -> Option<usize> {
        self.by_name.get(&name.to_ascii_lowercase()).copied()
    }
}

/// The segment each logical variable starts at, in variable order.
///
/// Derived from the layout's own grouping rather than recomputed, so
/// the schema cannot disagree with the row reader about where one
/// variable ends and the next begins.
fn segment_starts(layout: &DataLayout) -> Vec<usize> {
    let mut starts = Vec::with_capacity(layout.variables().len());
    let mut segment = 0;
    for variable in layout.variables() {
        starts.push(segment);
        segment += variable.segments().len();
    }
    starts
}

/// The per-segment tuple width a subtype-11 payload implies, or `None`
/// when the count is neither two nor three values per segment.
fn tuple_stride(value_count: usize, segment_count: usize) -> Option<usize> {
    if segment_count == 0 {
        return None;
    }
    [3, 2]
        .into_iter()
        .find(|&stride| value_count == segment_count * stride)
}

/// Builds a [`VariableDisplay`] from a 2- or 3-value tuple. The
/// three-value form carries a display width between the level and the
/// alignment; the two-value form omits it.
fn display_from_tuple(
    tuple: &[u32],
    warnings: &mut Vec<SavWarning>,
    variable_index: usize,
) -> VariableDisplay {
    let level_value = tuple[0];
    let level = measurement_level(level_value, warnings, variable_index);
    let mut builder = VariableDisplay::builder().measurement_level(level);
    if tuple.len() == 3 {
        builder = builder.display_width(tuple[1]);
    }
    let alignment_value = tuple[tuple.len() - 1];
    let alignment = Alignment::from_byte(narrow_to_byte(alignment_value));
    builder.alignment(alignment).build()
}

fn measurement_level(
    value: u32,
    warnings: &mut Vec<SavWarning>,
    variable_index: usize,
) -> MeasurementLevel {
    let byte = narrow_to_byte(value);
    let level = MeasurementLevel::from_byte(byte);
    if matches!(level, MeasurementLevel::Unknown(_)) {
        let warning = SavWarning::UnknownMeasurementLevel {
            variable_index: u32::try_from(variable_index).unwrap_or(u32::MAX),
            byte,
        };
        warnings.push(warning);
    }
    level
}

/// Narrows a subtype-11 value to the byte the enums classify.
///
/// The payload is `u32` on disk but every defined code fits a byte;
/// saturating keeps an out-of-range value out of range rather than
/// wrapping it onto a valid code.
fn narrow_to_byte(value: u32) -> u8 {
    u8::try_from(value).unwrap_or(u8::MAX)
}

fn unknown_variable(subtype: ExtensionSubtype, name: &str) -> SavWarning {
    SavWarning::UnknownVariableInExtension {
        subtype,
        name: name.to_owned(),
    }
}

/// Applies `patch` to the builder at `index`.
///
/// Builders take `self` and return `Self`, so patching one in place
/// means moving it out and back. [`std::mem::take`] gives a cheap
/// stand-in for the gap.
fn replace(
    builders: &mut [SavVariableBuilder],
    index: usize,
    patch: impl FnOnce(SavVariableBuilder) -> SavVariableBuilder,
) {
    let slot = &mut builders[index];
    let current = std::mem::take(slot);
    *slot = patch(current);
}

/// Builds the pre-reconciliation form of one variable from the type-2
/// record that leads it.
fn base_builder(
    header: &SavVariableHeader,
    variable_type: VariableType,
    index: usize,
    float_encoding: FloatEncoding,
    sentinels: &FloatSentinels,
) -> SavVariableBuilder {
    let missing = missing_value_reconcile::decode(
        header.missing_values(),
        variable_type,
        float_encoding,
        sentinels,
    );
    let mut builder = SavVariable::builder()
        .short_name(header.short_name())
        .variable_type(variable_type)
        .print_format(header.print_format())
        .write_format(header.write_format())
        .missing_value_spec(missing)
        .index(index);
    if let Some(label) = header.label() {
        builder = builder.label(label);
    }
    builder
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a schema whose variables carry `(short name, long name)`,
    /// with `weight` naming one of them by index.
    fn schema(names: &[(&str, Option<&str>)], weight: Option<usize>) -> SavSchema {
        let variables = names
            .iter()
            .enumerate()
            .map(|(index, (short_name, long_name))| {
                let mut builder = SavVariable::builder().short_name(*short_name).index(index);
                if let Some(long_name) = long_name {
                    builder = builder.long_name(*long_name);
                }
                builder.build()
            })
            .collect();
        SavSchema::new(variables, weight)
    }

    fn simple() -> SavSchema {
        schema(
            &[
                ("ID", Some("id")),
                ("Q1", None),
                ("LONGSTR", Some("longstr")),
            ],
            None,
        )
    }

    #[test]
    fn variable_index_finds_a_long_name() {
        assert_eq!(simple().variable_index("longstr"), Some(2));
    }

    /// A variable with no subtype-13 entry is reached by its short name,
    /// which is what `full_name` falls back to.
    #[test]
    fn variable_index_finds_a_short_name_when_no_long_name_was_declared() {
        assert_eq!(simple().variable_index("Q1"), Some(1));
    }

    #[test]
    fn variable_index_matches_case_insensitively() {
        let schema = simple();
        for spelling in ["longstr", "LONGSTR", "LongStr"] {
            assert_eq!(schema.variable_index(spelling), Some(2), "{spelling}");
        }
    }

    /// The long name displaces the short one, so a variable renamed by
    /// subtype 13 is no longer reachable under its on-disk short name.
    #[test]
    fn variable_index_does_not_match_a_displaced_short_name() {
        // ID declares the long name "id", so both spellings resolve --
        // but a short name that differs from the long one does not.
        let schema = schema(&[("V1", Some("household_income"))], None);
        assert_eq!(schema.variable_index("household_income"), Some(0));
        assert_eq!(schema.variable_index("V1"), None);
    }

    #[test]
    fn variable_index_misses_return_none() {
        assert_eq!(simple().variable_index("nope"), None);
    }

    /// A malformed file can name two variables the same. Resolving to
    /// the first matches how reconciliation picks a target, so a lookup
    /// and a patch land on the same variable.
    #[test]
    fn variable_index_resolves_a_duplicate_name_to_the_first() {
        let schema = schema(&[("A", Some("dup")), ("B", Some("dup"))], None);
        assert_eq!(schema.variable_index("dup"), Some(0));
    }

    #[test]
    fn variable_by_name_agrees_with_variable_index() {
        let schema = simple();
        for name in ["id", "Q1", "longstr", "nope"] {
            let by_index = schema.variable_index(name).and_then(|i| schema.variable(i));
            let by_name = schema.variable_by_name(name);
            assert_eq!(
                by_name.map(SavVariable::index),
                by_index.map(SavVariable::index),
                "{name}",
            );
        }
    }

    #[test]
    fn variable_reports_the_index_it_was_found_at() {
        let schema = simple();
        let variable = schema.variable_by_name("longstr").expect("present");
        assert_eq!(variable.index(), 2);
        assert_eq!(
            schema.variable(2).map(SavVariable::full_name),
            Some("longstr"),
        );
        assert!(schema.variable(3).is_none());
    }

    #[test]
    fn variable_count_matches_the_variable_list() {
        assert_eq!(simple().variable_count(), 3);
        assert_eq!(simple().variables().len(), 3);
    }

    #[test]
    fn weight_variable_resolves_through_its_index() {
        let schema = schema(&[("ID", Some("id")), ("WGT", Some("wgt"))], Some(1));
        let weight = schema.weight_variable().expect("weight present");
        assert_eq!(weight.full_name(), "wgt");
    }

    #[test]
    fn no_weight_index_means_no_weight_variable() {
        assert!(simple().weight_variable().is_none());
    }
}
