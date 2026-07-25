//! Subtype 14 — very-long-string widths (collection wrapper).

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::{
    VERY_LONG_STRINGS_ELEMENT_SIZE, VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR,
    VERY_LONG_STRINGS_PAIR_PADDING, VERY_LONG_STRINGS_PAIR_SEPARATOR,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::very_long_string::VeryLongString;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};

/// Reads a subtype-14 record from `envelope`, yielding the
/// [`VeryLongStrings`] declarations. Forwards the envelope's fields to
/// [`parse`].
#[inline]
pub(crate) fn read(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let declarations = parse(
        envelope.element_size,
        &envelope.payload,
        envelope.encoding,
        envelope.element_size_position,
    )?;
    let strings = VeryLongStrings::builder().strings(declarations).build();
    let record = ExtensionRecord::VeryLongStrings(strings);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// The very-long-string width declarations from one extension
/// subtype-14 record.
///
/// A newtype over the parsed [`VeryLongString`]s, in on-disk order, so
/// the extension record's payload shape can gain fields without
/// changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeryLongStrings {
    strings: Vec<VeryLongString>,
}

impl VeryLongStrings {
    /// Returns a fresh [`VeryLongStringsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VeryLongStringsBuilder {
        VeryLongStringsBuilder::default()
    }

    /// The very-long-string declarations, in on-disk order.
    #[must_use]
    #[inline]
    pub fn strings(&self) -> &[VeryLongString] {
        &self.strings
    }
}

/// Builder for [`VeryLongStrings`].
#[derive(Debug, Default, Clone)]
pub struct VeryLongStringsBuilder {
    strings: Vec<VeryLongString>,
}

impl VeryLongStringsBuilder {
    /// Appends one very-long-string declaration.
    #[must_use]
    #[inline]
    pub fn string(mut self, value: VeryLongString) -> Self {
        self.strings.push(value);
        self
    }

    /// Replaces the collection with `strings`.
    #[must_use]
    #[inline]
    pub fn strings(mut self, strings: Vec<VeryLongString>) -> Self {
        self.strings = strings;
        self
    }

    /// Finalizes this builder into a [`VeryLongStrings`].
    ///
    /// Unset strings default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VeryLongStrings {
        VeryLongStrings {
            strings: self.strings,
        }
    }
}

/// Parses an extension subtype-14 payload (very-long-string widths)
/// into [`VeryLongString`] declarations.
///
/// The on-disk shape is `short=width` pairs joined by
/// [`VERY_LONG_STRINGS_PAIR_SEPARATOR`] (a tab), with the width
/// written as ASCII decimal digits. SPSS terminates each pair with a
/// NUL before the tab; any run of trailing NULs in a pair is
/// trimmed, and a trailing separator is permitted. The short name is
/// decoded through `encoding`; the width is *not* validated against
/// the schema (that it exceeds 255, or matches the variable's
/// declared segments) — finalization, which knows the variables,
/// reconciles the declarations.
///
/// # Errors
///
/// * [`FormatErrorKind::UnexpectedValue`] tagged
///   [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`FormatErrorKind::UnexpectedValue`] tagged
///   [`Field::VeryLongStringPair`] when a non-empty pair lacks a
///   [`VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR`], has an empty key or
///   width, or has a width that isn't decimal digits or doesn't fit
///   in a `u32`.
fn parse(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<VeryLongString>> {
    if actual_size != VERY_LONG_STRINGS_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    let pair_error = || {
        SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::VeryLongStringPair,
            },
        )
    };
    let mut declarations: Vec<VeryLongString> = Vec::new();
    let pairs = payload.split(|&b| b == VERY_LONG_STRINGS_PAIR_SEPARATOR);
    for pair in pairs {
        let trimmed_len = pair
            .iter()
            .rposition(|&b| b != VERY_LONG_STRINGS_PAIR_PADDING)
            .map_or(0, |index| index + 1);
        let pair = &pair[..trimmed_len];
        // A pair that's empty after trimming is the NUL terminator
        // of the preceding pair or the optional trailing separator,
        // not a malformed pair.
        if pair.is_empty() {
            continue;
        }
        let eq_index = pair
            .iter()
            .position(|&b| b == VERY_LONG_STRINGS_KEY_VALUE_SEPARATOR);
        let Some(eq_index) = eq_index else {
            return Err(pair_error());
        };
        let (key_bytes, rest) = pair.split_at(eq_index);
        let width_bytes = &rest[1..];
        if key_bytes.is_empty() || width_bytes.is_empty() {
            return Err(pair_error());
        }
        let mut width: u32 = 0;
        for &byte in width_bytes {
            if !byte.is_ascii_digit() {
                return Err(pair_error());
            }
            let digit = u32::from(byte - b'0');
            width = width
                .checked_mul(10)
                .and_then(|w| w.checked_add(digit))
                .ok_or_else(pair_error)?;
        }
        let (short_cow, _, _) = encoding.decode(key_bytes);
        let declaration = VeryLongString::builder()
            .short_name(short_cow.into_owned())
            .width(width)
            .build();
        declarations.push(declaration);
    }
    Ok(declarations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::test_support::{
        build_header, open, write_extension_record, write_terminator,
    };

    #[test]
    fn parse_single_pair() {
        let payload = b"RESPONSE=00226";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].short_name(), "RESPONSE");
        assert_eq!(result[0].width(), 226);
    }

    #[test]
    fn parse_spss_nul_terminated_pairs() {
        // SPSS terminates every pair with a NUL before the tab,
        // including the last.
        let payload = b"V1=00300\0\tV2=01000\0\t";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        let pairs: Vec<(&str, u32)> = result.iter().map(|d| (d.short_name(), d.width())).collect();
        assert_eq!(pairs, vec![("V1", 300), ("V2", 1000)]);
    }

    #[test]
    fn parse_plain_tab_separators() {
        let payload = b"V1=300\tV2=1000";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[1].width(), 1000);
    }

    #[test]
    fn parse_multiple_nuls_before_separator() {
        let payload = b"V1=300\0\0\0\tV2=1000";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].width(), 300);
    }

    #[test]
    fn parse_trailing_nuls_without_separator() {
        let payload = b"V1=300\0\0";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].width(), 300);
    }

    #[test]
    fn parse_empty_payload_yields_empty_vec() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_decodes_key_through_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"CAF\xE9=300";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].short_name(), "CAFé");
    }

    #[test]
    fn parse_preserves_duplicates_in_order() {
        let payload = b"V1=300\tV1=400";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].width(), 300);
        assert_eq!(result[1].width(), 400);
    }

    #[test]
    fn parse_maximum_width_accepted() {
        let payload = b"V1=4294967295";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].width(), u32::MAX);
    }

    #[test]
    fn parse_rejects_missing_equals() {
        let payload = b"V1=300\tV2only";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_rejects_empty_key() {
        let payload = b"=300";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_rejects_empty_width() {
        let payload = b"V1=";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_rejects_non_digit_width() {
        let payload = b"V1=3O0";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_rejects_width_overflowing_u32() {
        let payload = b"V1=4294967296";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn parse_interior_nul_preserved_in_key() {
        // NULs are trimmed only from a pair's end; an interior NUL
        // is not silently dropped into a different name. (Like
        // subtype 13, no character-class enforcement at streaming.)
        let payload = b"V\x001=300";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].short_name(), "V\u{0}1");
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"V1=300", encoding_rs::WINDOWS_1252, 0).unwrap_err();
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
    fn reader_single_declaration() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"RESPONSE=00226\0\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) = record
        else {
            panic!("expected VeryLongStrings, got {record:?}");
        };
        let declarations = declarations.strings();
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].short_name(), "RESPONSE");
        assert_eq!(declarations[0].width(), 226);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_multiple_declarations_in_order() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=00300\0\tV2=01000\0\tV3=32767\0\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected VeryLongStrings");
        };
        let declarations = declarations.strings();
        let pairs: Vec<(&str, u32)> = declarations
            .iter()
            .map(|d| (d.short_name(), d.width()))
            .collect();
        assert_eq!(pairs, vec![("V1", 300), ("V2", 1000), ("V3", 32767)]);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=00300\0\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected VeryLongStrings");
        };
        let declarations = declarations.strings();
        assert_eq!(declarations.len(), 1);
        assert_eq!(declarations[0].width(), 300);
    }

    #[test]
    fn reader_empty_payload_yields_empty_vec() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 14, 1, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::VeryLongStrings(declarations)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected VeryLongStrings");
        };
        let declarations = declarations.strings();
        assert!(declarations.is_empty());
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 14, 4, 2, &[0; 8]);

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

    #[test]
    fn reader_malformed_pair_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=300\tV2=abc";
        write_extension_record(
            &mut bytes,
            byte_order,
            14,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::VeryLongStringPair,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }
}
