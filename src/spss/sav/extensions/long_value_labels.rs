//! Subtype 21 — long string value labels (collection wrapper).

use encoding_rs::Encoding;

use crate::spss::sav::byte_cursor::ByteCursor;
use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::LONG_STRING_VALUE_LABELS_ELEMENT_SIZE;
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::unexpected_value_error;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::long_value_label::LongValueLabel;
use crate::spss::sav::extensions::long_value_label_record::LongValueLabelRecord;
use crate::spss::sav::sav_error::{Field, Result, Section};

/// Reads a subtype-21 record from `envelope`, yielding the
/// [`LongValueLabels`]. Forwards the envelope's fields and `encoding` to [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let records = parse(
        envelope.element_size,
        &envelope.payload,
        envelope.byte_order,
        encoding,
        envelope.element_size_position,
    )?;
    let labels = LongValueLabels::builder().add_records(records).build();
    let record = ExtensionRecord::LongValueLabels(labels);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// The long string value labels from one extension subtype-21 record.
///
/// A newtype over the parsed [`LongValueLabelRecord`]s (one per
/// variable), in on-disk order, so the extension record's payload
/// shape can gain fields without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongValueLabels {
    records: Vec<LongValueLabelRecord>,
}

impl LongValueLabels {
    /// Returns a fresh [`LongValueLabelsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongValueLabelsBuilder {
        LongValueLabelsBuilder::default()
    }

    /// The per-variable long value label records, in on-disk order.
    #[must_use]
    #[inline]
    pub fn records(&self) -> &[LongValueLabelRecord] {
        &self.records
    }
}

/// Builder for [`LongValueLabels`].
#[derive(Debug, Default, Clone)]
pub struct LongValueLabelsBuilder {
    records: Vec<LongValueLabelRecord>,
}

impl LongValueLabelsBuilder {
    /// Appends one variable's long value label record.
    #[must_use]
    #[inline]
    pub fn add_record(mut self, value: LongValueLabelRecord) -> Self {
        self.records.push(value);
        self
    }

    /// Appends `records`.
    #[must_use]
    #[inline]
    pub fn add_records(mut self, records: impl IntoIterator<Item = LongValueLabelRecord>) -> Self {
        self.records.extend(records);
        self
    }

    /// Finalizes this builder into a [`LongValueLabels`].
    ///
    /// Unset records default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongValueLabels {
        LongValueLabels {
            records: self.records,
        }
    }
}

/// Parses an extension subtype-21 payload (long string value labels)
/// into one [`LongValueLabelRecord`] per variable.
///
/// The payload repeats until exhausted: a `u32`-length-prefixed
/// variable name, a `u32` declared width, a `u32` label count, then
/// that many `(value, label)` pairs where each of value and label is a
/// `u32`-length-prefixed byte string. All `u32` fields honor
/// `byte_order`. Variable names and labels are decoded through
/// `encoding`; the value bytes are kept verbatim.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::LongValueLabel`] when the payload is truncated (a
///   length prefix or its bytes run past the end).
fn parse(
    actual_size: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<LongValueLabelRecord>> {
    if actual_size != LONG_STRING_VALUE_LABELS_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let mut cursor = ByteCursor::new(payload, Section::Dictionary, position);
    let mut records: Vec<LongValueLabelRecord> = Vec::new();
    while !cursor.is_empty() {
        let record = parse_long_value_label_record(&mut cursor, byte_order, encoding)?;
        records.push(record);
    }
    Ok(records)
}

/// Parses one per-variable [`LongValueLabelRecord`] from `cursor`,
/// advancing past it: a `u32`-length-prefixed variable name, a `u32`
/// declared width, a `u32` label count, then that many `(value,
/// label)` pairs.
fn parse_long_value_label_record(
    cursor: &mut ByteCursor<'_>,
    byte_order: ByteOrder,
    encoding: &'static Encoding,
) -> Result<LongValueLabelRecord> {
    let field = Field::LongValueLabel;
    let name_bytes = cursor.take_length_prefixed(byte_order, field)?;
    let width = cursor.take_u32(byte_order, field)?;
    let label_count = cursor.take_u32(byte_order, field)?;
    let mut labels: Vec<LongValueLabel> = Vec::new();
    for _ in 0..label_count {
        let label = parse_long_value_label(cursor, byte_order, encoding)?;
        labels.push(label);
    }
    let (variable_name, _, _) = encoding.decode(name_bytes);
    let record = LongValueLabelRecord::builder()
        .variable_name(variable_name.into_owned())
        .width(width)
        .add_labels(labels)
        .build();
    Ok(record)
}

/// Parses one `(value, label)` pair from `cursor`, advancing past it.
/// Both value and label are `u32`-length-prefixed byte strings; the
/// value bytes are kept verbatim and the label is decoded through
/// `encoding`.
fn parse_long_value_label(
    cursor: &mut ByteCursor<'_>,
    byte_order: ByteOrder,
    encoding: &'static Encoding,
) -> Result<LongValueLabel> {
    let field = Field::LongValueLabel;
    let value = cursor.take_length_prefixed(byte_order, field)?;
    let value = value.to_vec();
    let label_bytes = cursor.take_length_prefixed(byte_order, field)?;
    let (label, _, _) = encoding.decode(label_bytes);
    let label = LongValueLabel::builder()
        .value(value)
        .label(label.into_owned())
        .build();
    Ok(label)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::sav_error::{FormatErrorKind, SavError};
    use crate::spss::sav::test_support::{
        assert_unexpected_value_error, build_header, open, push_prefixed, push_u32,
        write_extension_record, write_terminator,
    };

    #[test]
    fn parse_single_variable_single_label() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"longvar", byte_order);
        push_u32(&mut payload, 20, byte_order); // width
        push_u32(&mut payload, 1, byte_order); // label count
        push_prefixed(&mut payload, b"code01", byte_order); // value
        push_prefixed(&mut payload, b"First", byte_order); // label

        let result = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "longvar");
        assert_eq!(result[0].width(), 20);
        assert_eq!(result[0].labels().len(), 1);
        assert_eq!(result[0].labels()[0].value(), b"code01");
        assert_eq!(result[0].labels()[0].label(), "First");
    }

    #[test]
    fn parse_multiple_labels_and_variables() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v1", byte_order);
        push_u32(&mut payload, 10, byte_order);
        push_u32(&mut payload, 2, byte_order);
        push_prefixed(&mut payload, b"a", byte_order);
        push_prefixed(&mut payload, b"Apple", byte_order);
        push_prefixed(&mut payload, b"b", byte_order);
        push_prefixed(&mut payload, b"Banana", byte_order);
        push_prefixed(&mut payload, b"v2", byte_order);
        push_u32(&mut payload, 12, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_prefixed(&mut payload, b"z", byte_order);
        push_prefixed(&mut payload, b"Zed", byte_order);

        let result = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].labels().len(), 2);
        assert_eq!(result[0].labels()[1].label(), "Banana");
        assert_eq!(result[1].variable_name(), "v2");
        assert_eq!(result[1].labels()[0].value(), b"z");
    }

    #[test]
    fn parse_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        push_u32(&mut payload, 9, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_prefixed(&mut payload, b"x", byte_order);
        push_prefixed(&mut payload, b"Label", byte_order);

        let result = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].width(), 9);
        assert_eq!(result[0].labels()[0].label(), "Label");
    }

    #[test]
    fn parse_keeps_value_bytes_verbatim() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        push_u32(&mut payload, 8, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_prefixed(&mut payload, b"ab   ", byte_order); // trailing spaces preserved
        push_prefixed(&mut payload, b"L", byte_order);

        let result = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].labels()[0].value(), b"ab   ");
    }

    #[test]
    fn parse_empty_payload_yields_empty_vec() {
        let result = parse(
            1,
            &[],
            ByteOrder::LittleEndian,
            encoding_rs::WINDOWS_1252,
            0,
        )
        .unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(
            4,
            &[0; 4],
            ByteOrder::LittleEndian,
            encoding_rs::WINDOWS_1252,
            0,
        )
        .unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_rejects_truncated_payload() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        push_u32(&mut payload, 8, byte_order);
        push_u32(&mut payload, 1, byte_order);
        push_u32(&mut payload, 5, byte_order); // value length 5, but no bytes follow
        let err = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::LongValueLabel);
    }

    #[test]
    fn reader_long_string_value_labels() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        payload.extend_from_slice(&3u32.to_le_bytes()); // var name length
        payload.extend_from_slice(b"abc");
        payload.extend_from_slice(&20u32.to_le_bytes()); // width
        payload.extend_from_slice(&1u32.to_le_bytes()); // label count
        payload.extend_from_slice(&2u32.to_le_bytes()); // value length
        payload.extend_from_slice(b"hi");
        payload.extend_from_slice(&5u32.to_le_bytes()); // label length
        payload.extend_from_slice(b"Hello");

        let mut bytes = build_header(byte_order);
        write_extension_record(
            &mut bytes,
            byte_order,
            21,
            1,
            u32::try_from(payload.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::LongValueLabels(records)) = record else {
            panic!("expected LongValueLabels, got {record:?}");
        };
        let records = records.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].variable_name(), "abc");
        assert_eq!(records[0].width(), 20);
        assert_eq!(records[0].labels()[0].value(), b"hi");
        assert_eq!(records[0].labels()[0].label(), "Hello");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 21, 4, 2, &[0; 8]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }
}
