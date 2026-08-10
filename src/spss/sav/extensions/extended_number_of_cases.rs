//! Subtype 16 — extended number of cases.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    EXTENDED_NUMBER_OF_CASES_COUNT_OFFSET, EXTENDED_NUMBER_OF_CASES_ELEMENT_COUNT,
    EXTENDED_NUMBER_OF_CASES_ELEMENT_SIZE, EXTENDED_NUMBER_OF_CASES_VERSION_OFFSET,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::validate_extension_shape;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::sav_error::Result;

/// Extended number-of-cases record from extension record subtype 16.
///
/// Authoritative when the header's `case_count` field is `-1`
/// (i.e., the case count overflows the 32-bit field). The on-disk
/// payload is two `i64`s: a [`version`](Self::version) flag
/// (`ReadStat`'s writer always emits `1`) and the actual
/// [`count`](Self::count) of cases in the file.
///
/// The role of the version flag is not formally documented; the
/// reader surfaces it verbatim so consumers can inspect or
/// round-trip it without interpreting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtendedNumberOfCases {
    version: i64,
    count: i64,
}

impl ExtendedNumberOfCases {
    /// Returns a fresh [`ExtendedNumberOfCasesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> ExtendedNumberOfCasesBuilder {
        ExtendedNumberOfCasesBuilder::default()
    }

    /// Version flag from the record body. `ReadStat`'s writer always
    /// emits `1`; other values appear only in files written by
    /// other tools or in malformed files.
    #[must_use]
    #[inline]
    pub fn version(&self) -> i64 {
        self.version
    }

    /// Authoritative case count when the header's `case_count`
    /// field is `-1`.
    #[must_use]
    #[inline]
    pub fn count(&self) -> i64 {
        self.count
    }
}

/// Builder for [`ExtendedNumberOfCases`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ExtendedNumberOfCasesBuilder {
    version: Option<i64>,
    count: Option<i64>,
}

impl ExtendedNumberOfCasesBuilder {
    /// Sets the version flag.
    #[must_use]
    #[inline]
    pub fn version(mut self, value: i64) -> Self {
        self.version = Some(value);
        self
    }

    /// Sets the case count.
    #[must_use]
    #[inline]
    pub fn count(mut self, value: i64) -> Self {
        self.count = Some(value);
        self
    }

    /// Finalizes this builder into an [`ExtendedNumberOfCases`].
    ///
    /// Unset fields default to `0`.
    #[must_use]
    #[inline]
    pub fn build(self) -> ExtendedNumberOfCases {
        let version = self.version.unwrap_or(0);
        let count = self.count.unwrap_or(0);
        ExtendedNumberOfCases { version, count }
    }
}

/// Reads a subtype-16 record from `envelope`, yielding the
/// [`ExtendedNumberOfCases`]. Forwards the envelope's fields to
/// [`parse`].
#[inline]
pub(crate) fn read(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let extended = parse(
        envelope.element_size,
        envelope.element_count,
        &envelope.payload,
        envelope.byte_order,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::ExtendedNumberOfCases(extended);
    Ok(DictionaryRecord::Extension(record))
}

/// Parses a subtype-16 payload (two `i64` fields — a version flag plus
/// the authoritative case count).
///
/// Validates the envelope against the subtype's spec shape
/// (`element_size == 8`, `element_count == 2`) and decodes both `i64`s
/// in the file's byte order.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`](crate::spss::sav::sav_error::FormatErrorKind::UnexpectedValue)
/// when the envelope shape disagrees with the spec.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` does not equal `16`; the
/// caller reads the payload from the validated dimensions, so this is
/// a logic invariant.
fn parse(
    actual_size: u32,
    actual_count: u32,
    payload: &[u8],
    byte_order: ByteOrder,
    position: u64,
) -> Result<ExtendedNumberOfCases> {
    validate_extension_shape(
        actual_size,
        actual_count,
        EXTENDED_NUMBER_OF_CASES_ELEMENT_SIZE,
        EXTENDED_NUMBER_OF_CASES_ELEMENT_COUNT,
        position,
    )?;
    debug_assert_eq!(payload.len(), 16);
    let i64_at = |offset: usize| -> i64 {
        let bytes: [u8; 8] = payload[offset..offset + 8]
            .try_into()
            .expect("envelope validation guarantees a 16-byte payload");
        byte_order.read_i64(bytes)
    };
    let record = ExtendedNumberOfCases::builder()
        .version(i64_at(EXTENDED_NUMBER_OF_CASES_VERSION_OFFSET))
        .count(i64_at(EXTENDED_NUMBER_OF_CASES_COUNT_OFFSET))
        .build();
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;

    use crate::spss::sav::test_support::{
        assert_degraded_extension, build_header, open, write_extension_record, write_terminator,
    };

    fn build_payload(byte_order: ByteOrder, version: i64, count: i64) -> Vec<u8> {
        let to_bytes = |v: i64| match byte_order {
            ByteOrder::LittleEndian => v.to_le_bytes(),
            ByteOrder::BigEndian => v.to_be_bytes(),
        };
        let mut buf = Vec::with_capacity(16);
        buf.extend_from_slice(&to_bytes(version));
        buf.extend_from_slice(&to_bytes(count));
        buf
    }

    #[test]
    fn reader_extended_number_of_cases() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let payload = build_payload(byte_order, 1, 1_234_567_890);
        write_extension_record(&mut bytes, byte_order, 16, 8, 2, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::ExtendedNumberOfCases(extended)) = record
        else {
            panic!("expected ExtendedNumberOfCases, got {record:?}");
        };
        assert_eq!(extended.version(), 1);
        assert_eq!(extended.count(), 1_234_567_890);
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_big_endian() {
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload = build_payload(byte_order, 1, -42);
        write_extension_record(&mut bytes, byte_order, 16, 8, 2, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::ExtendedNumberOfCases(extended)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected ExtendedNumberOfCases");
        };
        assert_eq!(extended.count(), -42);
    }

    #[test]
    fn reader_wrong_element_size_degrades() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 16, 4, 2, &[0; 8]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        assert_degraded_extension(&mut dict, ExtensionSubtype::ExtendedNumberOfCases);
    }

    #[test]
    fn reader_wrong_element_count_degrades() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 16, 8, 1, &[0; 8]);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        assert_degraded_extension(&mut dict, ExtensionSubtype::ExtendedNumberOfCases);
    }
}
