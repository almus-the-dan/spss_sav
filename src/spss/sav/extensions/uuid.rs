//! Subtype 12 — file UUID.

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::UUID_ELEMENT_SIZE;
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::unexpected_value_error;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::sav_error::{Field, Result};

/// A file UUID from extension record subtype 12.
///
/// SPSS (observed from version 13) writes a UUID in the RFC 4122
/// format as text — the 36-character hyphenated hexadecimal form,
/// which may mix upper and lower case. The reader keeps the string
/// verbatim (preserving case and formatting) and decodes it through
/// the file's active encoding; it is not parsed or validated against
/// RFC 4122.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uuid {
    text: String,
}

impl Uuid {
    /// Returns a fresh [`UuidBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> UuidBuilder {
        UuidBuilder::default()
    }

    /// The UUID text, verbatim.
    #[must_use]
    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Builder for [`Uuid`].
#[derive(Debug, Default, Clone)]
pub struct UuidBuilder {
    text: Option<String>,
}

impl UuidBuilder {
    /// Sets the UUID text.
    #[must_use]
    #[inline]
    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = Some(value.into());
        self
    }

    /// Finalizes this builder into a [`Uuid`].
    ///
    /// An unset text defaults to the empty string.
    #[must_use]
    #[inline]
    pub fn build(self) -> Uuid {
        Uuid {
            text: self.text.unwrap_or_default(),
        }
    }
}

/// Reads a subtype-12 record from `envelope`, yielding the file
/// [`Uuid`]. Forwards the envelope's fields and `encoding` to [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let uuid = parse(
        envelope.element_size,
        &envelope.payload,
        encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::Uuid(uuid);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Parses a subtype-12 payload into a [`Uuid`].
///
/// The payload is the UUID's RFC 4122 text form. It is decoded through
/// `encoding` verbatim — the string is neither trimmed nor validated
/// against RFC 4122, preserving its exact bytes and letter case.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`](crate::spss::sav::sav_error::FormatErrorKind::UnexpectedValue)
/// tagged [`Field::ExtensionElementSize`] when `actual_size != 1`.
fn parse(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Uuid> {
    if actual_size != UUID_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let (text, _, _) = encoding.decode(payload);
    let uuid = Uuid::builder().text(text.into_owned()).build();
    Ok(uuid)
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
    fn parse_keeps_mixed_case_text_verbatim() {
        let payload = b"F81D4fae-7DEC-11d0-a765-00A0C91E6BF6";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "F81D4fae-7DEC-11d0-a765-00A0C91E6BF6");
    }

    #[test]
    fn parse_does_not_validate_or_trim() {
        // Not a valid UUID and has trailing space; both are kept as-is.
        let payload = b"not-a-uuid ";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "not-a-uuid ");
    }

    #[test]
    fn parse_empty_payload_yields_empty_text() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "");
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"uuid", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn reader_uuid() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"F81D4fae-7DEC-11d0-a765-00A0C91E6BF6";
        write_extension_record(
            &mut bytes,
            byte_order,
            12,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::Uuid(uuid)) = record else {
            panic!("expected Uuid, got {record:?}");
        };
        assert_eq!(uuid.text(), "F81D4fae-7DEC-11d0-a765-00A0C91E6BF6");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 12, 4, 2, &[0; 8]);

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
