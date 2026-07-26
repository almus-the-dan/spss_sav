//! Subtype 18 — per-variable custom attributes (collection wrapper).

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::{
    VARIABLE_ATTRIBUTES_ELEMENT_SIZE, VARIABLE_ATTRIBUTES_NAME_TERMINATOR,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::{parse_attribute_set, unexpected_value_error};
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::variable_attribute_entry::VariableAttributeEntry;
use crate::spss::sav::extensions::variable_attribute_record::VariableAttributeRecord;
use crate::spss::sav::sav_error::{Field, Result};

/// Reads a subtype-18 record from `envelope`, yielding the
/// [`VariableAttributes`]. Forwards the envelope's fields to
/// [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let records = parse(
        envelope.element_size,
        &envelope.payload,
        encoding,
        envelope.element_size_position,
    )?;
    let attributes = VariableAttributes::builder().records(records).build();
    let record = ExtensionRecord::VariableAttributes(attributes);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// The per-variable custom attributes from one extension subtype-18
/// record.
///
/// A newtype over the parsed [`VariableAttributeRecord`]s (one per
/// variable), in on-disk order, so the extension record's payload
/// shape can gain fields without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableAttributes {
    records: Vec<VariableAttributeRecord>,
}

impl VariableAttributes {
    /// Returns a fresh [`VariableAttributesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableAttributesBuilder {
        VariableAttributesBuilder::default()
    }

    /// The per-variable attribute records, in on-disk order.
    #[must_use]
    #[inline]
    pub fn records(&self) -> &[VariableAttributeRecord] {
        &self.records
    }
}

/// Builder for [`VariableAttributes`].
#[derive(Debug, Default, Clone)]
pub struct VariableAttributesBuilder {
    records: Vec<VariableAttributeRecord>,
}

impl VariableAttributesBuilder {
    /// Appends one variable's attribute record.
    #[must_use]
    #[inline]
    pub fn record(mut self, value: VariableAttributeRecord) -> Self {
        self.records.push(value);
        self
    }

    /// Replaces the collection with `records`.
    #[must_use]
    #[inline]
    pub fn records(mut self, records: Vec<VariableAttributeRecord>) -> Self {
        self.records = records;
        self
    }

    /// Finalizes this builder into a [`VariableAttributes`].
    ///
    /// Unset records default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableAttributes {
        VariableAttributes {
            records: self.records,
        }
    }
}

/// Parses an extension subtype-18 (variable attributes) payload into
/// its list of [`VariableAttributeRecord`]s.
///
/// The payload is a sequence of `variable_name:attribute-set` groups,
/// each after the first delimited from the previous by `/`. The
/// attribute set within each group uses the same grammar as subtype
/// 17. Variable names and attribute contents are decoded through
/// `encoding`.
///
/// # Errors
/// - [`Field::ExtensionElementSize`] when `actual_size` isn't `1`.
/// - [`Field::VariableAttribute`] on a structurally malformed group
///   (missing `:`, missing `(`, unterminated value group, or a value
///   not properly quoted) — exact policy pinned in the
///   implementation.
fn parse(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<VariableAttributeRecord>> {
    if actual_size != VARIABLE_ATTRIBUTES_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let mut cursor = payload;
    let mut records: Vec<VariableAttributeRecord> = Vec::new();
    while !cursor.is_empty() {
        let record = parse_variable_attribute(&mut cursor, encoding, position)?;
        records.push(record);
    }
    Ok(records)
}

/// Parses one `variable_name:attribute-set` group from `cursor` into a
/// [`VariableAttributeRecord`], advancing the cursor past the group
/// (including its trailing `/` set separator, if present). The name
/// runs up to the `:` that precedes its attributes.
fn parse_variable_attribute(
    cursor: &mut &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<VariableAttributeRecord> {
    let name_end = cursor
        .iter()
        .position(|&b| b == VARIABLE_ATTRIBUTES_NAME_TERMINATOR);
    let Some(name_end) = name_end else {
        return Err(unexpected_value_error(position, Field::VariableAttribute));
    };
    let name_bytes = &cursor[..name_end];
    if name_bytes.is_empty() {
        return Err(unexpected_value_error(position, Field::VariableAttribute));
    }
    *cursor = &cursor[name_end + 1..];
    let set = parse_attribute_set(cursor, encoding, position, Field::VariableAttribute)?;
    let (variable_name, _, _) = encoding.decode(name_bytes);
    let mut builder = VariableAttributeRecord::builder().variable_name(variable_name.into_owned());
    for (name, values) in set {
        let entry = VariableAttributeEntry::builder()
            .name(name)
            .values(values)
            .build();
        builder = builder.attribute(entry);
    }
    let record = builder.build();
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::sav_error::{FormatErrorKind, SavError};
    use crate::spss::sav::test_support::{
        assert_unexpected_value_error, build_header, open, write_extension_record, write_terminator,
    };

    #[test]
    fn parse_single_variable_single_attribute() {
        let payload = b"var:a('1'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "var");
        assert_eq!(result[0].attributes().len(), 1);
        assert_eq!(result[0].attributes()[0].name(), "a");
        assert_eq!(result[0].attributes()[0].values(), &["1".to_string()]);
    }

    #[test]
    fn parse_multiple_variables_slash_delimited() {
        let payload = b"v1:a('1'\n)/v2:b('2'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].variable_name(), "v1");
        assert_eq!(result[0].attributes()[0].name(), "a");
        assert_eq!(result[1].variable_name(), "v2");
        assert_eq!(result[1].attributes()[0].name(), "b");
    }

    #[test]
    fn parse_multiple_attributes_per_variable() {
        let payload = b"v:a('1'\n)b('2'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].attributes().len(), 2);
        assert_eq!(result[0].attributes()[0].name(), "a");
        assert_eq!(result[0].attributes()[1].name(), "b");
    }

    #[test]
    fn parse_trailing_slash_accepted() {
        let payload = b"v:a('1'\n)/";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].variable_name(), "v");
    }

    #[test]
    fn parse_slash_inside_value_is_not_a_separator() {
        // A `/` before a value's line feed is content, not the set
        // delimiter, so it stays in the single record's value.
        let payload = b"v:a('a/b'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].attributes()[0].values(), &["a/b".to_string()]);
    }

    #[test]
    fn parse_empty_payload_yields_empty_vec() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"v:a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_rejects_missing_colon() {
        let err = parse(1, b"a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableAttribute);
    }

    #[test]
    fn parse_rejects_empty_variable_name() {
        let err = parse(1, b":a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableAttribute);
    }

    #[test]
    fn parse_rejects_malformed_attribute() {
        let err = parse(1, b"v:a('1')", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::VariableAttribute);
    }

    #[test]
    fn reader_variable_attributes() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"weight:$@Role('0'\n)/height:units('cm'\n)";
        write_extension_record(
            &mut bytes,
            byte_order,
            18,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::VariableAttributes(records)) = record
        else {
            panic!("expected VariableAttributes, got {record:?}");
        };
        let records = records.records();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].variable_name(), "weight");
        assert_eq!(records[0].attributes()[0].name(), "$@Role");
        assert_eq!(records[0].attributes()[0].values(), &["0".to_string()]);
        assert_eq!(records[1].variable_name(), "height");
        assert_eq!(records[1].attributes()[0].values(), &["cm".to_string()]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 18, 4, 2, &[0; 8]);

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
