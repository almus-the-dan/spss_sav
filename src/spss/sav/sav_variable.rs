//! A reconciled SAV variable.

use std::rc::Rc;

use crate::spss::sav::extensions::variable_display::VariableDisplay;
use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::sav_format::SavFormat;
use crate::spss::sav::value_label_set::ValueLabelSet;
use crate::spss::sav::value_label_value::ValueLabelValue;
use crate::spss::sav::variable_attribute::VariableAttribute;
use crate::spss::sav::variable_type::VariableType;

/// A fully reconciled SAV variable.
///
/// `SavVariable` is the user-facing type returned from a finalized
/// [`SavSchema`](crate::spss::sav::sav_schema::SavSchema). It bundles
/// the wire-level fields read from a single type-2 record with the
/// information patched in from extension records — long names,
/// display parameters, custom attributes, and very long string
/// widths.
///
/// Distinct from
/// [`SavVariableHeader`](crate::spss::sav::sav_variable_header::SavVariableHeader),
/// which is the streaming-yielded wire-level type used during the
/// dictionary phase. A very long string appears here as **one**
/// variable at its full declared width, not as the several
/// fixed-width segments the wire level yields.
#[derive(Debug, Clone)]
pub struct SavVariable {
    short_name: String,
    long_name: Option<String>,
    variable_type: VariableType,
    print_format: SavFormat,
    write_format: SavFormat,
    label: Option<String>,
    missing_value_spec: MissingValueSpecification,
    value_labels: Option<Rc<ValueLabelSet>>,
    display: Option<VariableDisplay>,
    attributes: Vec<VariableAttribute>,
    index: usize,
}

impl SavVariable {
    /// Returns a fresh [`SavVariableBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> SavVariableBuilder {
        SavVariableBuilder::default()
    }

    /// The 1–8 byte SAV short name.
    #[must_use]
    #[inline]
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// The long variable name, if one was declared via subtype 13.
    ///
    /// Empty until the dictionary phase has consumed subtype 13 and
    /// patched it onto this variable.
    #[must_use]
    #[inline]
    pub fn long_name(&self) -> Option<&str> {
        self.long_name.as_deref()
    }

    /// Convenience accessor returning [`long_name`](Self::long_name)
    /// when populated, otherwise [`short_name`](Self::short_name).
    #[must_use]
    pub fn full_name(&self) -> &str {
        self.long_name.as_deref().unwrap_or(&self.short_name)
    }

    /// Storage type (numeric or fixed-width string).
    ///
    /// For a very long string this is the logical width the file
    /// declared in extension subtype 14, not the 255-byte width of its
    /// first segment.
    #[must_use]
    #[inline]
    pub fn variable_type(&self) -> VariableType {
        self.variable_type
    }

    /// The print format used for default rendering.
    #[must_use]
    #[inline]
    pub fn print_format(&self) -> SavFormat {
        self.print_format
    }

    /// The write format used when serializing the value back to text.
    #[must_use]
    #[inline]
    pub fn write_format(&self) -> SavFormat {
        self.write_format
    }

    /// The user-facing variable label, if one was declared.
    #[must_use]
    #[inline]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// The missing-value specification.
    #[must_use]
    #[inline]
    pub fn missing_value_spec(&self) -> &MissingValueSpecification {
        &self.missing_value_spec
    }

    /// The value labels attached to this variable, if it has any.
    ///
    /// The SAV format has no named label sets: a type-3 record lists
    /// the pairs and the type-4 record that follows names the variables
    /// they cover. Variables covered by the same pair share one set
    /// rather than each holding a copy.
    #[must_use]
    #[inline]
    pub fn value_labels(&self) -> Option<&ValueLabelSet> {
        self.value_labels.as_deref()
    }

    /// Convenience lookup: the label this variable gives `value`, or
    /// `None` when it has no labels or none of them match.
    #[must_use]
    #[inline]
    pub fn label_for(&self, value: &ValueLabelValue) -> Option<&str> {
        self.value_labels.as_ref()?.label_for(value)
    }

    /// Display parameters from extension subtype 11, if present.
    #[must_use]
    #[inline]
    pub fn display(&self) -> Option<&VariableDisplay> {
        self.display.as_ref()
    }

    /// Custom attributes from extension subtype 18.
    #[must_use]
    #[inline]
    pub fn attributes(&self) -> &[VariableAttribute] {
        &self.attributes
    }

    /// The attribute named `name`, if this variable carries one.
    ///
    /// Matched case-insensitively: an attribute name is an SPSS
    /// identifier, and identifiers are not case-sensitive, so two
    /// attributes cannot differ by case alone.
    ///
    /// ASCII-only, with the same shortfall and the same miss-rather-
    /// than-mismatch behavior described on
    /// [`SavSchema::variable_index`](crate::spss::sav::sav_schema::SavSchema::variable_index).
    #[must_use]
    pub fn attribute(&self, name: &str) -> Option<&VariableAttribute> {
        self.attributes
            .iter()
            .find(|attribute| attribute.name().eq_ignore_ascii_case(name))
    }

    /// 0-based index of this variable in the schema.
    ///
    /// Counts logical variables, so it matches the position in
    /// [`SavSchema::variables`](crate::spss::sav::sav_schema::SavSchema::variables).
    /// It is **not** the segment index a
    /// [`RawValueLabelSet`](crate::spss::sav::raw_value_label_set::RawValueLabelSet)
    /// carries, which counts type-2 primary records and so runs ahead
    /// once a very long string appears.
    #[must_use]
    #[inline]
    pub fn index(&self) -> usize {
        self.index
    }
}

/// Builder for [`SavVariable`].
#[derive(Debug, Default, Clone)]
pub struct SavVariableBuilder {
    short_name: Option<String>,
    long_name: Option<String>,
    variable_type: Option<VariableType>,
    print_format: Option<SavFormat>,
    write_format: Option<SavFormat>,
    label: Option<String>,
    missing_value_spec: Option<MissingValueSpecification>,
    value_labels: Option<Rc<ValueLabelSet>>,
    display: Option<VariableDisplay>,
    attributes: Vec<VariableAttribute>,
    index: usize,
}

impl SavVariableBuilder {
    /// Sets the SAV short name (1–8 bytes).
    #[must_use]
    #[inline]
    pub fn short_name(mut self, name: impl Into<String>) -> Self {
        self.short_name = Some(name.into());
        self
    }

    /// Sets the long variable name (typically populated by the
    /// dictionary reader after subtype 13 is processed).
    #[must_use]
    #[inline]
    pub fn long_name(mut self, name: impl Into<String>) -> Self {
        self.long_name = Some(name.into());
        self
    }

    /// Clears the long variable name.
    #[must_use]
    #[inline]
    pub fn clear_long_name(mut self) -> Self {
        self.long_name = None;
        self
    }

    /// Sets the storage type.
    #[must_use]
    #[inline]
    pub fn variable_type(mut self, variable_type: VariableType) -> Self {
        self.variable_type = Some(variable_type);
        self
    }

    /// Sets the print format.
    #[must_use]
    #[inline]
    pub fn print_format(mut self, format: SavFormat) -> Self {
        self.print_format = Some(format);
        self
    }

    /// Sets the write format.
    #[must_use]
    #[inline]
    pub fn write_format(mut self, format: SavFormat) -> Self {
        self.write_format = Some(format);
        self
    }

    /// Sets the user-facing variable label.
    #[must_use]
    #[inline]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Clears the user-facing variable label.
    #[must_use]
    #[inline]
    pub fn clear_label(mut self) -> Self {
        self.label = None;
        self
    }

    /// Sets the missing-value specification.
    #[must_use]
    #[inline]
    pub fn missing_value_spec(mut self, spec: MissingValueSpecification) -> Self {
        self.missing_value_spec = Some(spec);
        self
    }

    /// Attaches a value-label set, shared with any other variable the
    /// same type-3 / type-4 pair covered.
    #[must_use]
    #[inline]
    pub fn value_labels(mut self, labels: Rc<ValueLabelSet>) -> Self {
        self.value_labels = Some(labels);
        self
    }

    /// Detaches the value-label set.
    #[must_use]
    #[inline]
    pub fn clear_value_labels(mut self) -> Self {
        self.value_labels = None;
        self
    }

    /// Sets the display parameters (typically attached by the
    /// dictionary reader after subtype 11 is processed).
    #[must_use]
    #[inline]
    pub fn display(mut self, display: VariableDisplay) -> Self {
        self.display = Some(display);
        self
    }

    /// Clears the display parameters.
    #[must_use]
    #[inline]
    pub fn clear_display(mut self) -> Self {
        self.display = None;
        self
    }

    /// Appends a custom attribute.
    #[must_use]
    #[inline]
    pub fn attribute(mut self, attribute: VariableAttribute) -> Self {
        self.attributes.push(attribute);
        self
    }

    /// Replaces the attribute list wholesale.
    #[must_use]
    #[inline]
    pub fn attributes(mut self, attributes: Vec<VariableAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// The name the built variable will report from
    /// [`SavVariable::full_name`] — the long name if one has been set,
    /// the short name otherwise, and `""` if neither has.
    ///
    /// Answerable before [`build`](Self::build) so a caller assembling
    /// several variables can tell them apart without materializing them
    /// first. The dictionary reader uses it that way, indexing builders
    /// by name while it reconciles extension records against them.
    #[must_use]
    #[inline]
    pub(crate) fn full_name(&self) -> &str {
        self.long_name
            .as_deref()
            .or(self.short_name.as_deref())
            .unwrap_or_default()
    }

    /// Sets the 0-based variable index.
    ///
    /// Crate-internal — set by the dictionary reader / writer when
    /// the variable's position in the schema becomes known.
    #[inline]
    pub(crate) fn index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    /// Finalizes this builder into a [`SavVariable`].
    ///
    /// Unset fields take the same neutral defaults
    /// [`SavVariableHeader`](crate::spss::sav::sav_variable_header::SavVariableHeader)
    /// uses: an empty short name, [`VariableType::Numeric`], freshly
    /// built formats, no label, and no declared missing values.
    /// Required-vs-optional checks live at write time.
    #[must_use]
    pub fn build(self) -> SavVariable {
        let short_name = self.short_name.unwrap_or_default();
        let variable_type = self.variable_type.unwrap_or(VariableType::Numeric);
        let print_format = self
            .print_format
            .unwrap_or_else(|| SavFormat::builder().build());
        let write_format = self
            .write_format
            .unwrap_or_else(|| SavFormat::builder().build());
        let missing_value_spec = self
            .missing_value_spec
            .unwrap_or(MissingValueSpecification::None);
        SavVariable {
            short_name,
            long_name: self.long_name,
            variable_type,
            print_format,
            write_format,
            label: self.label,
            missing_value_spec,
            value_labels: self.value_labels,
            display: self.display,
            attributes: self.attributes,
            index: self.index,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn full_name_prefers_the_long_name() {
        let builder = SavVariable::builder().short_name("V1").long_name("income");
        assert_eq!(builder.full_name(), "income");
        assert_eq!(builder.build().full_name(), "income");
    }

    #[test]
    fn full_name_falls_back_to_the_short_name() {
        let builder = SavVariable::builder().short_name("V1");
        assert_eq!(builder.full_name(), "V1");
        assert_eq!(builder.build().full_name(), "V1");
    }

    /// `build` defaults an unset short name to empty, so the builder has
    /// to report the same rather than having nothing to say.
    #[test]
    fn full_name_of_an_empty_builder_is_empty() {
        let builder = SavVariable::builder();
        assert_eq!(builder.full_name(), "");
        assert_eq!(builder.build().full_name(), "");
    }

    /// Clearing the long name puts the short one back in play, which is
    /// what a caller undoing a subtype-13 patch would expect.
    #[test]
    fn clearing_the_long_name_restores_the_short_one() {
        let builder = SavVariable::builder()
            .short_name("V1")
            .long_name("income")
            .clear_long_name();
        assert_eq!(builder.full_name(), "V1");
        assert_eq!(builder.build().full_name(), "V1");
    }

    /// The builder's answer has to survive `build` unchanged, or callers
    /// identifying a builder by name would be identifying something
    /// else.
    #[test]
    fn the_builder_and_the_built_variable_agree() {
        let cases = [
            (Some("V1"), Some("income")),
            (Some("V1"), None),
            (None, Some("income")),
            (None, None),
        ];
        for (short_name, long_name) in cases {
            let mut builder = SavVariable::builder();
            if let Some(short_name) = short_name {
                builder = builder.short_name(short_name);
            }
            if let Some(long_name) = long_name {
                builder = builder.long_name(long_name);
            }
            let expected = builder.full_name().to_owned();
            assert_eq!(
                builder.build().full_name(),
                expected,
                "{short_name:?} / {long_name:?}",
            );
        }
    }

    /// An attribute name is an SPSS identifier, so a caller need not
    /// match the file's capitalization to find one.
    #[test]
    fn attribute_lookup_is_case_insensitive() {
        let variable = SavVariable::builder()
            .short_name("V1")
            .attribute(
                VariableAttribute::builder()
                    .name("MyAttr")
                    .value("hello")
                    .build(),
            )
            .build();
        for spelling in ["MyAttr", "myattr", "MYATTR"] {
            assert_eq!(
                variable
                    .attribute(spelling)
                    .and_then(VariableAttribute::value),
                Some("hello"),
                "{spelling}",
            );
        }
        assert!(variable.attribute("other").is_none());
    }

    #[test]
    fn build_defaults_an_unset_variable_to_numeric_with_no_missing_values() {
        let variable = SavVariable::builder().build();
        assert_eq!(variable.variable_type(), VariableType::Numeric);
        assert_eq!(
            variable.missing_value_spec(),
            &MissingValueSpecification::None,
        );
        assert!(variable.label().is_none());
        assert!(variable.long_name().is_none());
        assert!(variable.value_labels().is_none());
        assert!(variable.display().is_none());
        assert!(variable.attributes().is_empty());
        assert_eq!(variable.index(), 0);
    }
}
