//! Subtype 17 — file-level custom attributes (collection wrapper).

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::DATA_FILE_ATTRIBUTES_ELEMENT_SIZE;
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::{parse_attribute_set, unexpected_value_error};
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::file_attribute::FileAttribute;
use crate::spss::sav::sav_error::{Field, Result};

/// Reads a subtype-17 record from `envelope`, yielding the
/// [`FileAttributes`]. Forwards the envelope's fields and `encoding` to [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let attributes = parse(
        envelope.element_size,
        &envelope.payload,
        encoding,
        envelope.element_size_position,
    )?;
    let attributes = FileAttributes::builder().add_attributes(attributes).build();
    let record = ExtensionRecord::FileAttributes(attributes);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// The file-level custom attributes from one extension subtype-17
/// record.
///
/// A newtype over the parsed [`FileAttribute`]s, in on-disk order, so
/// the extension record's payload shape can gain fields without
/// changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAttributes {
    attributes: Vec<FileAttribute>,
}

impl FileAttributes {
    /// Returns a fresh [`FileAttributesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> FileAttributesBuilder {
        FileAttributesBuilder::default()
    }

    /// The file attributes, in on-disk order.
    #[must_use]
    #[inline]
    pub fn attributes(&self) -> &[FileAttribute] {
        &self.attributes
    }
}

/// Builder for [`FileAttributes`].
#[derive(Debug, Default, Clone)]
pub struct FileAttributesBuilder {
    attributes: Vec<FileAttribute>,
}

impl FileAttributesBuilder {
    /// Appends one file attribute.
    #[must_use]
    #[inline]
    pub fn add_attribute(mut self, value: FileAttribute) -> Self {
        self.attributes.push(value);
        self
    }

    /// Appends `attributes`.
    #[must_use]
    #[inline]
    pub fn add_attributes(mut self, attributes: Vec<FileAttribute>) -> Self {
        self.attributes.extend(attributes);
        self
    }

    /// Finalizes this builder into a [`FileAttributes`].
    ///
    /// Unset attributes default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> FileAttributes {
        FileAttributes {
            attributes: self.attributes,
        }
    }
}

/// Parses an extension subtype-17 (data file attributes) payload into
/// its list of [`FileAttribute`]s.
///
/// The payload is a single attribute set: one or more attributes
/// concatenated. Each attribute is a name (everything up to the next
/// `(`) followed, inside parentheses, by one or more values, each a
/// single-quoted string terminated by a line feed (`0x0a`). Only the
/// single outer quote pair is stripped from each value; interior
/// bytes (including any doubled `''`) are kept verbatim. Names are
/// preserved verbatim, including any `[n]` array-index suffix — the
/// index collapse is deferred to schema finalization. Both names and
/// values are decoded through `encoding`.
///
/// # Errors
/// - [`Field::ExtensionElementSize`] when `actual_size` isn't `1`.
/// - [`Field::FileAttribute`] on a structurally malformed attribute
///   (missing `(`, unterminated value group, or a value not properly
///   quoted) — exact policy pinned in the implementation.
fn parse(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<FileAttribute>> {
    if actual_size != DATA_FILE_ATTRIBUTES_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let mut cursor = payload;
    let mut attributes: Vec<FileAttribute> = Vec::new();
    // Subtype 17 is a single attribute set spanning the whole payload;
    // it has no `/` set separator, so `parse_attribute_set` consumes
    // everything in one pass. The loop only re-enters on the malformed
    // case where a stray `/` ended the set early, in which case we
    // simply continue with the remainder.
    while !cursor.is_empty() {
        let set = parse_attribute_set(&mut cursor, encoding, position, Field::FileAttribute)?;
        for (name, values) in set {
            let attribute = FileAttribute::builder()
                .name(name)
                .add_values(values)
                .build();
            attributes.push(attribute);
        }
    }
    Ok(attributes)
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
    fn parse_single_attribute_single_value() {
        let payload = b"attr('value'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name(), "attr");
        assert_eq!(result[0].values(), &["value".to_string()]);
    }

    #[test]
    fn parse_multiple_attributes_in_order() {
        let payload = b"a('1'\n)b('2'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name(), "a");
        assert_eq!(result[0].values(), &["1".to_string()]);
        assert_eq!(result[1].name(), "b");
        assert_eq!(result[1].values(), &["2".to_string()]);
    }

    #[test]
    fn parse_multiple_values_in_one_attribute() {
        let payload = b"a('1'\n'2'\n'3'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].values(),
            &["1".to_string(), "2".to_string(), "3".to_string()]
        );
    }

    #[test]
    fn parse_keeps_index_suffix_verbatim() {
        // PSPP stores multi-valued attributes as fred[1]/fred[2]; the
        // wire layer keeps the suffix verbatim, deferring the array
        // collapse to finalization.
        let payload = b"fred[1]('23'\n)fred[2]('34'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name(), "fred[1]");
        assert_eq!(result[0].values(), &["23".to_string()]);
        assert_eq!(result[1].name(), "fred[2]");
        assert_eq!(result[1].values(), &["34".to_string()]);
    }

    #[test]
    fn parse_strips_only_outer_quotes() {
        // Interior doubled quotes are kept verbatim (values are
        // line-feed-delimited, not quote-delimited), matching PSPP.
        let payload = b"a('it''s'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].values(), &["it''s".to_string()]);
    }

    #[test]
    fn parse_unquoted_value_kept_verbatim() {
        let payload = b"a(bare\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].values(), &["bare".to_string()]);
    }

    #[test]
    fn parse_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"a('caf\xE9'\n)";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].values(), &["café".to_string()]);
    }

    #[test]
    fn parse_empty_payload_yields_empty_vec() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"a('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn parse_rejects_missing_open_paren() {
        let err = parse(1, b"attr", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn parse_rejects_empty_name() {
        let err = parse(1, b"('1'\n)", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn parse_rejects_unterminated_value() {
        // No line feed before the closing paren / end of payload.
        let err = parse(1, b"a('1')", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn parse_rejects_missing_close_paren() {
        let err = parse(1, b"a('1'\n", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::FileAttribute);
    }

    #[test]
    fn reader_data_file_attributes() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"owner('Alice'\n)version('3'\n)";
        write_extension_record(
            &mut bytes,
            byte_order,
            17,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::FileAttributes(attributes)) = record
        else {
            panic!("expected FileAttributes, got {record:?}");
        };
        let attributes = attributes.attributes();
        assert_eq!(attributes.len(), 2);
        assert_eq!(attributes[0].name(), "owner");
        assert_eq!(attributes[0].values(), &["Alice".to_string()]);
        assert_eq!(attributes[1].name(), "version");
        assert_eq!(attributes[1].values(), &["3".to_string()]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 17, 4, 2, &[0; 8]);
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
