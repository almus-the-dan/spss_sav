//! Subtypes 7 / 19 — multiple response sets (collection wrapper).

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::{
    MULTIPLE_RESPONSE_SET_TYPE_CATEGORY, MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_COUNTED_VALUES,
    MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_VARIABLE_LABELS, MULTIPLE_RESPONSE_SETS_ELEMENT_SIZE,
    MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR, MULTIPLE_RESPONSE_SETS_LINE_SEPARATOR,
    MULTIPLE_RESPONSE_SETS_NAME_TERMINATOR,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::category_label_source::CategoryLabelSource;
use crate::spss::sav::extensions::extension_parse::unexpected_value_error;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::multiple_response_set::MultipleResponseSet;
use crate::spss::sav::extensions::multiple_response_set_kind::MultipleResponseSetKind;
use crate::spss::sav::reader_state::u32_as_usize;
use crate::spss::sav::sav_error::{Field, Result, Section};

/// Reads a subtype-7 or subtype-19 record from `envelope`, yielding
/// the [`MultipleResponseSets`]. Forwards the envelope's fields to
/// [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let sets = parse(
        envelope.element_size,
        &envelope.payload,
        encoding,
        envelope.element_size_position,
    )?;
    let sets = MultipleResponseSets::builder().sets(sets).build();
    let record = ExtensionRecord::MultipleResponseSets(sets);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// The multiple response sets from one extension subtype-7 or -19
/// record.
///
/// A newtype over the parsed [`MultipleResponseSet`]s, in on-disk
/// order, so the extension record's payload shape can gain fields
/// without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipleResponseSets {
    sets: Vec<MultipleResponseSet>,
}

impl MultipleResponseSets {
    /// Returns a fresh [`MultipleResponseSetsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> MultipleResponseSetsBuilder {
        MultipleResponseSetsBuilder::default()
    }

    /// The multiple response sets, in on-disk order.
    #[must_use]
    #[inline]
    pub fn sets(&self) -> &[MultipleResponseSet] {
        &self.sets
    }
}

/// Builder for [`MultipleResponseSets`].
#[derive(Debug, Default, Clone)]
pub struct MultipleResponseSetsBuilder {
    sets: Vec<MultipleResponseSet>,
}

impl MultipleResponseSetsBuilder {
    /// Appends one multiple response set.
    #[must_use]
    #[inline]
    pub fn set(mut self, value: MultipleResponseSet) -> Self {
        self.sets.push(value);
        self
    }

    /// Replaces the collection with `sets`.
    #[must_use]
    #[inline]
    pub fn sets(mut self, sets: Vec<MultipleResponseSet>) -> Self {
        self.sets = sets;
        self
    }

    /// Finalizes this builder into a [`MultipleResponseSets`].
    ///
    /// Unset sets default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> MultipleResponseSets {
        MultipleResponseSets { sets: self.sets }
    }
}

/// Parses an extension subtype-7 or subtype-19 payload (multiple
/// response sets) into its [`MultipleResponseSet`]s.
///
/// The payload is one set per line, each terminated by a line feed. A
/// line is `$name=<type>...`, where `<type>` is `C` (multiple
/// category), `D` (multiple dichotomy, category labels from variable
/// labels), or `E` (multiple dichotomy, category labels from counted
/// values — subtype 19 only). Counted values and the set label are
/// "counted strings" (`<decimal-length> <space> <that-many-bytes>`);
/// member variable names follow, space-separated. Names (including the
/// leading `$`), labels, counted values, and members are decoded
/// through `encoding`.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::MultipleResponseSet`] on a malformed set line (missing
///   `=`, empty name, unknown type letter, malformed counted string,
///   or a non-numeric `E` label source).
fn parse(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<MultipleResponseSet>> {
    if actual_size != MULTIPLE_RESPONSE_SETS_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let mut sets: Vec<MultipleResponseSet> = Vec::new();
    let lines = payload.split(|&b| b == MULTIPLE_RESPONSE_SETS_LINE_SEPARATOR);
    for line in lines {
        // The trailing line feed leaves a final empty segment.
        if line.is_empty() {
            continue;
        }
        let set = parse_multiple_response_set(line, encoding, position)?;
        sets.push(set);
    }
    Ok(sets)
}

/// Parses one `$name=<type>...` set line into a
/// [`MultipleResponseSet`].
fn parse_multiple_response_set(
    line: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<MultipleResponseSet> {
    let field = Field::MultipleResponseSet;
    let mut cursor = line;

    let name_bytes = take_before(&mut cursor, MULTIPLE_RESPONSE_SETS_NAME_TERMINATOR)
        .ok_or_else(|| unexpected_value_error(position, field))?;
    if name_bytes.is_empty() {
        return Err(unexpected_value_error(position, field));
    }

    let (&type_byte, after_type) = cursor
        .split_first()
        .ok_or_else(|| unexpected_value_error(position, field))?;
    cursor = after_type;

    let kind = match type_byte {
        MULTIPLE_RESPONSE_SET_TYPE_CATEGORY => read_category_kind(&mut cursor, position, field)?,
        MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_VARIABLE_LABELS => {
            read_dichotomy_variable_labels_kind(&mut cursor, encoding, position, field)?
        }
        MULTIPLE_RESPONSE_SET_TYPE_DICHOTOMY_COUNTED_VALUES => {
            read_dichotomy_counted_values_kind(&mut cursor, encoding, position, field)?
        }
        _ => return Err(unexpected_value_error(position, field)),
    };

    let label_bytes = read_counted_string(&mut cursor, position, field)?;
    let (label, _, _) = encoding.decode(label_bytes);
    let (name, _, _) = encoding.decode(name_bytes);

    let mut builder = MultipleResponseSet::builder()
        .name(name.into_owned())
        .label(label.into_owned())
        .kind(kind);
    // The remaining bytes are the member names, space-separated (the
    // leading separator and any empty tokens are dropped).
    let members = cursor.split(|&b| b == MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR);
    for member in members {
        if member.is_empty() {
            continue;
        }
        let (member, _, _) = encoding.decode(member);
        builder = builder.variable(member.into_owned());
    }
    let set = builder.build();
    Ok(set)
}

/// Reads the wire type `C` body — a multiple-category set. It has no
/// counted value; a single field separator precedes the label, which
/// the caller reads next.
fn read_category_kind(
    cursor: &mut &[u8],
    position: u64,
    field: Field,
) -> Result<MultipleResponseSetKind> {
    expect_field_separator(cursor, position, field)?;
    Ok(MultipleResponseSetKind::MultipleCategory)
}

/// Reads the wire type `D` body — a multiple-dichotomy set whose
/// category labels come from variable labels. The counted value (a
/// counted string) immediately follows the type letter, then a field
/// separator before the label.
fn read_dichotomy_variable_labels_kind(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<MultipleResponseSetKind> {
    let counted = read_counted_string(cursor, position, field)?;
    expect_field_separator(cursor, position, field)?;
    let (counted_value, _, _) = encoding.decode(counted);
    let kind = MultipleResponseSetKind::MultipleDichotomy {
        counted_value: counted_value.into_owned(),
        category_labels: CategoryLabelSource::VariableLabels,
    };
    Ok(kind)
}

/// Reads the wire type `E` body — a multiple-dichotomy set whose
/// category labels come from counted values (subtype 19 only). A
/// separator and label-source number precede the counted value, which
/// is followed by a separator before the label.
fn read_dichotomy_counted_values_kind(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
    field: Field,
) -> Result<MultipleResponseSetKind> {
    expect_field_separator(cursor, position, field)?;
    let source_bytes = take_before(cursor, MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR)
        .ok_or_else(|| unexpected_value_error(position, field))?;
    let label_source =
        parse_ascii_u32(source_bytes).ok_or_else(|| unexpected_value_error(position, field))?;
    let counted = read_counted_string(cursor, position, field)?;
    expect_field_separator(cursor, position, field)?;
    let (counted_value, _, _) = encoding.decode(counted);
    let kind = MultipleResponseSetKind::MultipleDichotomy {
        counted_value: counted_value.into_owned(),
        category_labels: CategoryLabelSource::CountedValues { label_source },
    };
    Ok(kind)
}

/// Reads a "counted string" (`<decimal-length> <space> <that-many
/// -bytes>`) from the front of `cursor`, advancing past it. Errors,
/// tagged `field`, when the length is missing/non-numeric or the bytes
/// run past the end.
fn read_counted_string<'a>(cursor: &mut &'a [u8], position: u64, field: Field) -> Result<&'a [u8]> {
    let length_bytes = take_before(cursor, MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR)
        .ok_or_else(|| unexpected_value_error(position, field))?;
    let length =
        parse_ascii_u32(length_bytes).ok_or_else(|| unexpected_value_error(position, field))?;
    let length = u32_as_usize(length, position, Section::Dictionary, field)?;
    if cursor.len() < length {
        return Err(unexpected_value_error(position, field));
    }
    let (bytes, rest) = cursor.split_at(length);
    *cursor = rest;
    Ok(bytes)
}

/// Splits the bytes before the first `delim` off the front of
/// `cursor`, advancing past both the prefix and the delimiter. Returns
/// `None` when `delim` is absent.
fn take_before<'a>(cursor: &mut &'a [u8], delim: u8) -> Option<&'a [u8]> {
    let position = cursor.iter().position(|&b| b == delim)?;
    let (head, rest) = cursor.split_at(position);
    *cursor = &rest[1..];
    Some(head)
}

/// Consumes a single [`MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR`] from
/// the front of `cursor`. Errors, tagged `field`, when the next byte
/// isn't that separator.
fn expect_field_separator(cursor: &mut &[u8], position: u64, field: Field) -> Result<()> {
    match cursor.split_first() {
        Some((&b, rest)) if b == MULTIPLE_RESPONSE_SETS_FIELD_SEPARATOR => {
            *cursor = rest;
            Ok(())
        }
        _ => Err(unexpected_value_error(position, field)),
    }
}

/// Parses `bytes` as an unsigned decimal integer. Returns `None` when
/// empty, non-digit, or overflowing `u32`.
fn parse_ascii_u32(bytes: &[u8]) -> Option<u32> {
    if bytes.is_empty() {
        return None;
    }
    let mut value: u32 = 0;
    for &byte in bytes {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add(u32::from(byte - b'0'))?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::test_support::assert_unexpected_value_error;

    #[test]
    fn parse_dichotomy_variable_labels() {
        let payload = b"$dich=D1 1 13 Dichotomy set q1 q2 q3\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        let set = &result[0];
        assert_eq!(set.name(), "$dich"); // leading '$' preserved
        assert_eq!(set.label(), "Dichotomy set");
        assert_eq!(
            set.variables(),
            ["q1".to_string(), "q2".to_string(), "q3".to_string()]
        );
        let MultipleResponseSetKind::MultipleDichotomy {
            counted_value,
            category_labels,
        } = set.kind()
        else {
            panic!("expected dichotomy, got {:?}", set.kind());
        };
        assert_eq!(counted_value.as_str(), "1");
        assert_eq!(*category_labels, CategoryLabelSource::VariableLabels);
    }

    #[test]
    fn parse_category() {
        let payload = b"$cat=C 12 Category set q1 q2\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let set = &result[0];
        assert_eq!(set.name(), "$cat");
        assert_eq!(set.label(), "Category set");
        assert_eq!(set.variables(), ["q1".to_string(), "q2".to_string()]);
        assert_eq!(*set.kind(), MultipleResponseSetKind::MultipleCategory);
    }

    #[test]
    fn parse_dichotomy_counted_values() {
        let payload = b"$counted=E 1 1 1 0  q2 q3\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let set = &result[0];
        assert_eq!(set.name(), "$counted");
        assert_eq!(set.label(), ""); // empty label (counted string length 0)
        assert_eq!(set.variables(), ["q2".to_string(), "q3".to_string()]);
        let MultipleResponseSetKind::MultipleDichotomy {
            counted_value,
            category_labels,
        } = set.kind()
        else {
            panic!("expected dichotomy, got {:?}", set.kind());
        };
        assert_eq!(counted_value.as_str(), "1");
        assert_eq!(
            *category_labels,
            CategoryLabelSource::CountedValues { label_source: 1 }
        );
    }

    #[test]
    fn parse_label_source_eleven() {
        // LABELSOURCE=VARLABEL is encoded as 11.
        let payload = b"$e=E 11 1 1 3 lbl a b\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let MultipleResponseSetKind::MultipleDichotomy {
            category_labels: CategoryLabelSource::CountedValues { label_source },
            ..
        } = result[0].kind()
        else {
            panic!(
                "expected counted-values dichotomy, got {:?}",
                result[0].kind()
            );
        };
        assert_eq!(*label_source, 11);
        assert_eq!(result[0].label(), "lbl");
        assert_eq!(result[0].variables(), ["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn parse_multiple_sets_and_string_counted_value() {
        // Two sets on two lines; the first has a multi-byte string
        // counted value ("yes", length 3) whose internal space-free
        // bytes are read by length.
        let payload = b"$a=D3 yes 5 Label q1\n$b=C 3 Two q2 q3\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        let MultipleResponseSetKind::MultipleDichotomy { counted_value, .. } = result[0].kind()
        else {
            panic!("expected dichotomy, got {:?}", result[0].kind());
        };
        assert_eq!(counted_value.as_str(), "yes");
        assert_eq!(result[0].variables(), ["q1".to_string()]);
        assert_eq!(*result[1].kind(), MultipleResponseSetKind::MultipleCategory);
        assert_eq!(result[1].label(), "Two");
    }

    #[test]
    fn parse_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252; the label byte length counts bytes.
        let payload = b"$s=C 5 caf\xE9x q1\n";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].label(), "caféx");
    }

    #[test]
    fn parse_empty_payload_yields_no_sets() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"$a=C 1 x q1\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_rejects_missing_equals() {
        let err = parse(1, b"$noeq\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::MultipleResponseSet);
    }

    #[test]
    fn parse_rejects_unknown_type_letter() {
        let err = parse(1, b"$a=X 1 x q1\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::MultipleResponseSet);
    }

    #[test]
    fn parse_rejects_counted_string_running_past_end() {
        // Label length 9 but only "Label" (5 bytes) remain.
        let err = parse(1, b"$a=C 9 Label\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::MultipleResponseSet);
    }
}
