//! Subtype 22 — long string missing values (collection wrapper).

use encoding_rs::Encoding;

use crate::spss::sav::byte_cursor::ByteCursor;
use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    LONG_STRING_MISSING_VALUE_MAX_COUNT, LONG_STRING_MISSING_VALUES_ELEMENT_SIZE,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::unexpected_value_error;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::long_missing_value_record::LongMissingValueRecord;
use crate::spss::sav::sav_error::{Field, Result, Section};

/// Reads a subtype-22 record from `envelope`, yielding the
/// [`LongMissingValues`]. Forwards the envelope's fields and `encoding` to [`parse`].
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
    let values = LongMissingValues::builder().records(records).build();
    let record = ExtensionRecord::LongMissingValues(values);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// The long string missing values from one extension subtype-22
/// record.
///
/// A newtype over the parsed [`LongMissingValueRecord`]s (one per
/// variable), in on-disk order, so the extension record's payload
/// shape can gain fields without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongMissingValues {
    records: Vec<LongMissingValueRecord>,
}

impl LongMissingValues {
    /// Returns a fresh [`LongMissingValuesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongMissingValuesBuilder {
        LongMissingValuesBuilder::default()
    }

    /// The per-variable long missing value records, in on-disk order.
    #[must_use]
    #[inline]
    pub fn records(&self) -> &[LongMissingValueRecord] {
        &self.records
    }
}

/// Builder for [`LongMissingValues`].
#[derive(Debug, Default, Clone)]
pub struct LongMissingValuesBuilder {
    records: Vec<LongMissingValueRecord>,
}

impl LongMissingValuesBuilder {
    /// Appends one variable's long missing value record.
    #[must_use]
    #[inline]
    pub fn record(mut self, value: LongMissingValueRecord) -> Self {
        self.records.push(value);
        self
    }

    /// Replaces the collection with `records`.
    #[must_use]
    #[inline]
    pub fn records(mut self, records: Vec<LongMissingValueRecord>) -> Self {
        self.records = records;
        self
    }

    /// Finalizes this builder into a [`LongMissingValues`].
    ///
    /// Unset records default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongMissingValues {
        LongMissingValues {
            records: self.records,
        }
    }
}

/// Parses an extension subtype-22 payload (long string missing values)
/// into one [`LongMissingValueRecord`] per variable.
///
/// The payload repeats until exhausted: a `u32`-length-prefixed
/// variable name, a single count byte (`1..=`
/// [`LONG_STRING_MISSING_VALUE_MAX_COUNT`]), a `u32` width shared by
/// every value, then that many raw values each `width` bytes long. The
/// `u32` fields honor `byte_order`. Variable names are decoded through
/// `encoding`; the value bytes are kept verbatim.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::LongMissingValueCount`] when the count byte is not
///   `1..=3` (matching `ReadStat`).
/// * [`Field::LongMissingValue`] when the payload is truncated.
fn parse(
    actual_size: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<LongMissingValueRecord>> {
    if actual_size != LONG_STRING_MISSING_VALUES_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let mut cursor = ByteCursor::new(payload, Section::Dictionary, position);
    let mut records: Vec<LongMissingValueRecord> = Vec::new();
    while !cursor.is_empty() {
        let record = parse_long_missing_value_record(&mut cursor, byte_order, encoding)?;
        records.push(record);
    }
    Ok(records)
}

/// Parses one per-variable [`LongMissingValueRecord`] from `cursor`,
/// advancing past it: a `u32`-length-prefixed variable name, a single
/// count byte (`1..=`[`LONG_STRING_MISSING_VALUE_MAX_COUNT`]), a `u32`
/// width shared by every value, then that many raw values each `width`
/// bytes long. The value bytes are kept verbatim; the variable name is
/// decoded through `encoding`.
fn parse_long_missing_value_record(
    cursor: &mut ByteCursor<'_>,
    byte_order: ByteOrder,
    encoding: &'static Encoding,
) -> Result<LongMissingValueRecord> {
    let field = Field::LongMissingValue;
    let name_bytes = cursor.take_length_prefixed(byte_order, field)?;
    let count = cursor.take_u8(field)?;
    if count == 0 || count > LONG_STRING_MISSING_VALUE_MAX_COUNT {
        return Err(cursor.unexpected_value(Field::LongMissingValueCount));
    }
    let width = cursor.take_u32_as_usize(byte_order, field)?;
    let mut values: Vec<Vec<u8>> = Vec::new();
    for _ in 0..count {
        let value_bytes = cursor.take_bytes(width, field)?;
        let value = value_bytes.to_vec();
        values.push(value);
    }
    let (variable_name, _, _) = encoding.decode(name_bytes);
    let record = LongMissingValueRecord::builder()
        .variable_name(variable_name.into_owned())
        .values(values)
        .build();
    Ok(record)
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
    fn parse_single_variable() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"longvar", byte_order);
        payload.push(2); // n_missing
        push_u32(&mut payload, 3, byte_order); // width
        payload.extend_from_slice(b"XXX");
        payload.extend_from_slice(b"YYY");

        let result = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "longvar");
        assert_eq!(result[0].values(), &[b"XXX".to_vec(), b"YYY".to_vec()]);
    }

    #[test]
    fn parse_big_endian_multiple_variables() {
        let byte_order = ByteOrder::BigEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v1", byte_order);
        payload.push(1);
        push_u32(&mut payload, 2, byte_order);
        payload.extend_from_slice(b"ab");
        push_prefixed(&mut payload, b"v2", byte_order);
        payload.push(3);
        push_u32(&mut payload, 1, byte_order);
        payload.extend_from_slice(b"xyz");

        let result = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].values(), &[b"ab".to_vec()]);
        assert_eq!(
            result[1].values(),
            &[b"x".to_vec(), b"y".to_vec(), b"z".to_vec()]
        );
    }

    #[test]
    fn parse_rejects_zero_count() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        payload.push(0);
        push_u32(&mut payload, 1, byte_order);
        let err = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::LongMissingValueCount);
    }

    #[test]
    fn parse_rejects_count_above_three() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        payload.push(4);
        push_u32(&mut payload, 1, byte_order);
        payload.extend_from_slice(b"abcd");
        let err = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::LongMissingValueCount);
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
    fn parse_rejects_truncated_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        push_prefixed(&mut payload, b"v", byte_order);
        payload.push(2);
        push_u32(&mut payload, 3, byte_order); // width 3, but only 3 bytes for one value
        payload.extend_from_slice(b"XXX");
        let err = parse(1, &payload, byte_order, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::LongMissingValue);
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
    fn reader_long_string_missing_values() {
        let byte_order = ByteOrder::LittleEndian;
        let mut payload = Vec::new();
        payload.extend_from_slice(&3u32.to_le_bytes()); // var name length
        payload.extend_from_slice(b"abc");
        payload.push(2); // n_missing
        payload.extend_from_slice(&4u32.to_le_bytes()); // value width
        payload.extend_from_slice(b"MISS");
        payload.extend_from_slice(b"GONE");

        let mut bytes = build_header(byte_order);
        write_extension_record(
            &mut bytes,
            byte_order,
            22,
            1,
            u32::try_from(payload.len()).unwrap(),
            &payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::LongMissingValues(records)) = record
        else {
            panic!("expected LongMissingValues, got {record:?}");
        };
        let records = records.records();
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].variable_name(), "abc");
        assert_eq!(records[0].values(), &[b"MISS".to_vec(), b"GONE".to_vec()]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 22, 4, 2, &[0; 8]);
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
