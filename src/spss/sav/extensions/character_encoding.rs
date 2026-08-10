//! Subtype 20 — declared character encoding (value wrapper).

use crate::spss::sav::dictionary_format::CHARACTER_ENCODING_ELEMENT_SIZE;
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};
use crate::spss::sav::text_field::trim_trailing_padding;

/// The character encoding label declared by an extension subtype-20
/// record.
///
/// A newtype over the decoded encoding `name` (e.g. `"UTF-8"`,
/// `"windows-1252"`) so the extension record's payload shape can gain
/// fields without changing the enum variant. The name is the label as
/// written on disk; it is not resolved to a concrete encoding here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterEncoding {
    name: String,
}

impl CharacterEncoding {
    /// Returns a fresh [`CharacterEncodingBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> CharacterEncodingBuilder {
        CharacterEncodingBuilder::default()
    }

    /// The declared encoding label, verbatim.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Builder for [`CharacterEncoding`].
#[derive(Debug, Default, Clone)]
pub struct CharacterEncodingBuilder {
    name: Option<String>,
}

impl CharacterEncodingBuilder {
    /// Sets the declared encoding label.
    #[must_use]
    #[inline]
    pub fn name(mut self, value: impl Into<String>) -> Self {
        self.name = Some(value.into());
        self
    }

    /// Finalizes this builder into a [`CharacterEncoding`].
    ///
    /// An unset name defaults to the empty string.
    #[must_use]
    #[inline]
    pub fn build(self) -> CharacterEncoding {
        CharacterEncoding {
            name: self.name.unwrap_or_default(),
        }
    }
}

/// Reads a subtype-20 record from `envelope`, yielding the declared
/// [`CharacterEncoding`]. Forwards the envelope's fields to [`parse`].
#[inline]
pub(crate) fn read(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let name = parse(
        envelope.element_size,
        &envelope.payload,
        envelope.element_size_position,
    )?;
    let encoding = CharacterEncoding::builder().name(name).build();
    let record = ExtensionRecord::CharacterEncoding(encoding);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Extracts the declared encoding label from a subtype-20 envelope, for
/// resolving the file's encoding during the dictionary scan.
///
/// Separate from [`read`] because resolution happens before any record
/// is handed to the caller: the label has to be known to decode every
/// other record's text, including this one's own surrounding records.
///
/// # Errors
///
/// Same as [`parse`] — a `element_size` other than 1 is rejected.
#[inline]
pub(crate) fn declared_label(envelope: &ExtensionEnvelope) -> Result<String> {
    parse(
        envelope.element_size,
        &envelope.payload,
        envelope.element_size_position,
    )
}

/// Parses a subtype-20 payload into the declared encoding label.
///
/// The payload is the encoding name in ASCII; trailing spaces and NULs
/// are trimmed and the remaining bytes are decoded lossily as UTF-8
/// (non-ASCII bytes become U+FFFD rather than failing the read).
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`] tagged
/// [`Field::ExtensionElementSize`] when `actual_size != 1`.
fn parse(actual_size: u32, payload: &[u8], position: u64) -> Result<String> {
    if actual_size != CHARACTER_ENCODING_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    let trimmed = trim_trailing_padding(payload);
    let name = String::from_utf8_lossy(trimmed).into_owned();
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;
    use crate::spss::sav::test_support::{
        assert_degraded_extension, build_header, open, write_extension_record, write_terminator,
    };

    #[test]
    fn parse_decodes_ascii_name() {
        let name = parse(1, b"UTF-8", 0).unwrap();
        assert_eq!(name, "UTF-8");
    }

    #[test]
    fn parse_trims_trailing_spaces_and_nuls() {
        let name = parse(1, b"UTF-8 \0  ", 0).unwrap();
        assert_eq!(name, "UTF-8");
    }

    #[test]
    fn parse_accepts_empty_payload() {
        let name = parse(1, &[], 0).unwrap();
        assert!(name.is_empty());
    }

    #[test]
    fn parse_lossy_on_non_ascii_bytes() {
        // 0xFF is invalid UTF-8 and not a sensible encoding-name byte;
        // the parser should emit U+FFFD rather than failing the read.
        let name = parse(1, b"A\xFFB", 0).unwrap();
        assert_eq!(name, "A\u{FFFD}B");
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"UTF-", 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementSize
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn reader_utf8() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 5, b"UTF-8");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) = record else {
            panic!("expected CharacterEncoding, got {record:?}");
        };
        assert_eq!(name.name(), "UTF-8");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_windows_1252() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 12, b"windows-1252");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert_eq!(name.name(), "windows-1252");
    }

    #[test]
    fn reader_trims_trailing_padding() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 8, b"UTF-8\0  ");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert_eq!(name.name(), "UTF-8");
    }

    #[test]
    fn reader_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 5, b"UTF-8");
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert_eq!(name.name(), "UTF-8");
    }

    #[test]
    fn reader_empty_payload_yields_empty_string() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 1, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::CharacterEncoding(name)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected CharacterEncoding");
        };
        assert!(name.name().is_empty());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_degrades() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 20, 4, 2, &[0; 8]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        assert_degraded_extension(&mut dict, ExtensionSubtype::CharacterEncoding);
    }
}
