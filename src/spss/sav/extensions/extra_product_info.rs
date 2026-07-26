//! Subtype 10 — extra product information.

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::EXTRA_PRODUCT_INFO_ELEMENT_SIZE;
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::unexpected_value_error;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::sav_error::{Field, Result};

/// Extra product information from extension record subtype 10.
///
/// A free-form text string identifying the product that wrote the
/// file, beyond the fixed 60-byte product name in the header. The
/// payload's byte length is exact (no padding), so the text is kept
/// verbatim — trailing whitespace is preserved — and decoded through
/// the file's active encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraProductInfo {
    text: String,
}

impl ExtraProductInfo {
    /// Returns a fresh [`ExtraProductInfoBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> ExtraProductInfoBuilder {
        ExtraProductInfoBuilder::default()
    }

    /// The product information text, verbatim.
    #[must_use]
    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Builder for [`ExtraProductInfo`].
#[derive(Debug, Default, Clone)]
pub struct ExtraProductInfoBuilder {
    text: Option<String>,
}

impl ExtraProductInfoBuilder {
    /// Sets the product information text.
    #[must_use]
    #[inline]
    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = Some(value.into());
        self
    }

    /// Finalizes this builder into an [`ExtraProductInfo`].
    ///
    /// An unset text defaults to the empty string.
    #[must_use]
    #[inline]
    pub fn build(self) -> ExtraProductInfo {
        ExtraProductInfo {
            text: self.text.unwrap_or_default(),
        }
    }
}

/// Reads a subtype-10 record from `envelope`, yielding the declared
/// [`ExtraProductInfo`]. Forwards the envelope's fields and `encoding` to [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let info = parse(
        envelope.element_size,
        &envelope.payload,
        encoding,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::ExtraProductInfo(info);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Parses a subtype-10 payload into an [`ExtraProductInfo`].
///
/// The payload is a free-form text string whose byte length is exact,
/// so it is decoded through `encoding` verbatim — no trailing padding
/// is trimmed.
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
) -> Result<ExtraProductInfo> {
    if actual_size != EXTRA_PRODUCT_INFO_ELEMENT_SIZE {
        return Err(unexpected_value_error(
            position,
            Field::ExtensionElementSize,
        ));
    }
    let (text, _, _) = encoding.decode(payload);
    let info = ExtraProductInfo::builder().text(text.into_owned()).build();
    Ok(info)
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
    fn parse_keeps_text_verbatim() {
        // Trailing spaces must be preserved (the length is exact).
        let payload = b"Acme Stats 4.3  ";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "Acme Stats 4.3  ");
    }

    #[test]
    fn parse_decodes_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"Caf\xE9 Analytics";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "Café Analytics");
    }

    #[test]
    fn parse_empty_payload_yields_empty_text() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.text(), "");
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"prod", encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_unexpected_value_error(&err, Field::ExtensionElementSize);
    }

    #[test]
    fn reader_extra_product_info() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"Acme Stats 4.3";
        write_extension_record(
            &mut bytes,
            byte_order,
            10,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::ExtraProductInfo(info)) = record else {
            panic!("expected ExtraProductInfo, got {record:?}");
        };
        assert_eq!(info.text(), "Acme Stats 4.3");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 10, 4, 2, &[0; 8]);

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
