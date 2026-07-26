//! Subtype 13 — long variable name mappings (collection wrapper).

use encoding_rs::Encoding;

use crate::spss::sav::dictionary_format::{
    LONG_VARIABLE_NAMES_ELEMENT_SIZE, LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR,
    LONG_VARIABLE_NAMES_PAIR_SEPARATOR,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::extensions::long_variable_name::LongVariableName;
use crate::spss::sav::sav_error::{Field, FormatErrorKind, Result, SavError, Section};

/// The short-to-long variable-name mappings from one extension
/// subtype-13 record.
///
/// A newtype over the parsed [`LongVariableName`]s, in on-disk order,
/// so the extension record's payload shape can gain fields without
/// changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongVariableNames {
    mappings: Vec<LongVariableName>,
}

impl LongVariableNames {
    /// Returns a fresh [`LongVariableNamesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongVariableNamesBuilder {
        LongVariableNamesBuilder::default()
    }

    /// The name mappings, in on-disk order.
    #[must_use]
    #[inline]
    pub fn mappings(&self) -> &[LongVariableName] {
        &self.mappings
    }
}

/// Builder for [`LongVariableNames`].
#[derive(Debug, Default, Clone)]
pub struct LongVariableNamesBuilder {
    mappings: Vec<LongVariableName>,
}

impl LongVariableNamesBuilder {
    /// Appends one name mapping.
    #[must_use]
    #[inline]
    pub fn mapping(mut self, value: LongVariableName) -> Self {
        self.mappings.push(value);
        self
    }

    /// Replaces the collection with `mappings`.
    #[must_use]
    #[inline]
    pub fn mappings(mut self, mappings: Vec<LongVariableName>) -> Self {
        self.mappings = mappings;
        self
    }

    /// Finalizes this builder into a [`LongVariableNames`].
    ///
    /// Unset mappings default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongVariableNames {
        LongVariableNames {
            mappings: self.mappings,
        }
    }
}

/// Reads a subtype-13 record from `envelope`, yielding the
/// [`LongVariableNames`] mappings. Forwards the envelope's fields to
/// [`parse`].
#[inline]
pub(crate) fn read(
    envelope: &ExtensionEnvelope,
    encoding: &'static Encoding,
) -> Result<DictionaryRecord> {
    let mappings = parse(
        envelope.element_size,
        &envelope.payload,
        encoding,
        envelope.element_size_position,
    )?;
    let names = LongVariableNames::builder().mappings(mappings).build();
    let record = ExtensionRecord::LongVariableNames(names);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Parses a subtype-13 payload (long-variable-name mappings).
///
/// The on-disk shape is a fixed-`element_size`-of-1 byte stream of
/// [`LONG_VARIABLE_NAMES_PAIR_SEPARATOR`]-separated pairs, each holding
/// a `short`[`LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR`]`long` mapping.
/// A trailing pair separator (the optional terminating tab PSPP's
/// grammar permits) is accepted without warning.
///
/// This validates `actual_size == 1` and decodes each half through
/// `encoding`. PSPP's character-class constraints and length limits
/// are *not* enforced — finalization or user code validates that the
/// short names match real variables. Duplicate short names are
/// preserved in declaration order.
///
/// # Errors
///
/// * [`Field::ExtensionElementSize`] when `actual_size != 1`.
/// * [`Field::LongVariableNamePair`] when a non-empty pair lacks a
///   separator, has an empty key, or has an empty value.
fn parse(
    actual_size: u32,
    payload: &[u8],
    encoding: &'static Encoding,
    position: u64,
) -> Result<Vec<LongVariableName>> {
    if actual_size != LONG_VARIABLE_NAMES_ELEMENT_SIZE {
        let error = SavError::format(
            Section::Dictionary,
            position,
            FormatErrorKind::UnexpectedValue {
                field: Field::ExtensionElementSize,
            },
        );
        return Err(error);
    }
    let mut mappings: Vec<LongVariableName> = Vec::new();
    let pairs = payload.split(|&b| b == LONG_VARIABLE_NAMES_PAIR_SEPARATOR);
    for pair in pairs {
        // A trailing pair separator yields a final empty segment; that's
        // the optional terminator PSPP's grammar permits, not a
        // malformed pair.
        if pair.is_empty() {
            continue;
        }
        let eq_index = pair
            .iter()
            .position(|&b| b == LONG_VARIABLE_NAMES_KEY_VALUE_SEPARATOR);
        let Some(eq_index) = eq_index else {
            let error = SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair,
                },
            );
            return Err(error);
        };
        let (key_bytes, rest) = pair.split_at(eq_index);
        let value_bytes = &rest[1..];
        if key_bytes.is_empty() || value_bytes.is_empty() {
            let error = SavError::format(
                Section::Dictionary,
                position,
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair,
                },
            );
            return Err(error);
        }
        let (short_cow, _, _) = encoding.decode(key_bytes);
        let (long_cow, _, _) = encoding.decode(value_bytes);
        let mapping = LongVariableName::builder()
            .short_name(short_cow.into_owned())
            .long_name(long_cow.into_owned())
            .build();
        mappings.push(mapping);
    }
    Ok(mappings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::dictionary_record::DictionaryRecord;
    use crate::spss::sav::extensions::extension_record::ExtensionRecord;
    use crate::spss::sav::test_support::{
        build_header, open, write_extension_record, write_terminator,
    };

    #[test]
    fn parse_single_pair() {
        let payload = b"V1=Variable1";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].short_name(), "V1");
        assert_eq!(result[0].long_name(), "Variable1");
    }

    #[test]
    fn parse_multiple_pairs() {
        let payload = b"V1=Variable1\tV2=Variable2\tV3=Variable3";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].short_name(), "V1");
        assert_eq!(result[1].short_name(), "V2");
        assert_eq!(result[2].long_name(), "Variable3");
    }

    #[test]
    fn parse_trailing_separator_accepted() {
        let payload = b"V1=Variable1\tV2=Variable2\t";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn parse_embedded_equals_in_value_split_on_first() {
        let payload = b"K=v1=more";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].short_name(), "K");
        assert_eq!(result[0].long_name(), "v1=more");
    }

    #[test]
    fn parse_empty_payload_yields_empty_vec() {
        let result = parse(1, &[], encoding_rs::WINDOWS_1252, 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_uses_supplied_encoding() {
        // 0xE9 = é in Windows-1252, invalid in standalone UTF-8.
        let payload = b"K=caf\xE9";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result[0].long_name(), "café");
    }

    #[test]
    fn parse_preserves_duplicates_in_order() {
        let payload = b"V1=First\tV1=Second";
        let result = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].long_name(), "First");
        assert_eq!(result[1].long_name(), "Second");
    }

    #[test]
    fn parse_rejects_missing_equals() {
        let payload = b"V1=ok\tV2only";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_pair_error(&err);
    }

    #[test]
    fn parse_rejects_empty_key() {
        let payload = b"=Variable1";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_pair_error(&err);
    }

    #[test]
    fn parse_rejects_empty_value() {
        let payload = b"V1=";
        let err = parse(1, payload, encoding_rs::WINDOWS_1252, 0).unwrap_err();
        assert_pair_error(&err);
    }

    #[test]
    fn parse_rejects_wrong_element_size() {
        let err = parse(4, b"V1=L", encoding_rs::WINDOWS_1252, 0).unwrap_err();
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

    fn assert_pair_error(err: &SavError) {
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::LongVariableNamePair
                }
            ),
            _ => panic!("expected Format error"),
        }
    }

    #[test]
    fn reader_single_mapping() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Variable1";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(names)) = record else {
            panic!("expected LongVariableNames, got {record:?}");
        };
        let mappings = names.mappings();
        assert_eq!(mappings.len(), 1);
        assert_eq!(mappings[0].short_name(), "V1");
        assert_eq!(mappings[0].long_name(), "Variable1");
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_multiple_mappings_in_order() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Alpha\tV2=Beta\tV3=Gamma";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(names)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        let names: Vec<(&str, &str)> = names
            .mappings()
            .iter()
            .map(|m| (m.short_name(), m.long_name()))
            .collect();
        assert_eq!(
            names,
            vec![("V1", "Alpha"), ("V2", "Beta"), ("V3", "Gamma")]
        );
    }

    #[test]
    fn reader_optional_trailing_separator() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Alpha\tV2=Beta\t";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(names)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert_eq!(names.mappings().len(), 2);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=Long";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(names)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert_eq!(names.mappings()[0].long_name(), "Long");
    }

    #[test]
    fn reader_decoded_through_active_encoding() {
        // 0xE9 = é in Windows-1252 (the test header's default
        // encoding), invalid as standalone UTF-8.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=caf\xE9";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
            1,
            u32::try_from(payload.len()).unwrap(),
            payload,
        );
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(names)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert_eq!(names.mappings()[0].long_name(), "café");
    }

    #[test]
    fn reader_empty_payload_yields_empty_vec() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 13, 1, 0, &[]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::LongVariableNames(names)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected LongVariableNames");
        };
        assert!(names.mappings().is_empty());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_missing_equals_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = b"V1=ok\tBADPAIR";
        write_extension_record(
            &mut bytes,
            byte_order,
            13,
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
                    field: Field::LongVariableNamePair,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 13, 4, 2, &[0; 8]);

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
