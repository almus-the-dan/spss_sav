//! Subtype 4 — float sentinel values.

use crate::spss::sav::dictionary_format::{
    FLOAT_SENTINELS_ELEMENT_COUNT, FLOAT_SENTINELS_ELEMENT_SIZE, FLOAT_SENTINELS_HIGHEST_OFFSET,
    FLOAT_SENTINELS_LOWEST_OFFSET, FLOAT_SENTINELS_SYSTEM_MISSING_OFFSET,
};
use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_parse::validate_extension_shape;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::sav_error::Result;

/// Float sentinel values declared by extension record subtype 4.
///
/// Carries the file's system-missing bit pattern plus the `LOWEST`
/// and `HIGHEST` open-bound markers used by missing-value range
/// declarations. All three are preserved as raw 8-byte slabs in the
/// file's declared float format (IEEE 754, IBM HFP, or VAX), so
/// byte-equality comparisons against cell values stay unambiguous
/// regardless of float format, and roundtrip is bit-exact.
///
/// Consumers convert to `f64` using
/// [`SavHeader::float_format`](crate::spss::sav::sav_header::SavHeader::float_format)
/// and the file's byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatSentinels {
    system_missing: [u8; 8],
    highest: [u8; 8],
    lowest: [u8; 8],
}

impl FloatSentinels {
    /// Returns a fresh [`FloatSentinelsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> FloatSentinelsBuilder {
        FloatSentinelsBuilder::default()
    }

    /// Raw bytes of the system-missing sentinel.
    #[must_use]
    #[inline]
    pub fn system_missing(&self) -> [u8; 8] {
        self.system_missing
    }

    /// Raw bytes of the `HIGHEST` sentinel (upper open bound for
    /// missing-value range declarations).
    #[must_use]
    #[inline]
    pub fn highest(&self) -> [u8; 8] {
        self.highest
    }

    /// Raw bytes of the `LOWEST` sentinel (lower open bound for
    /// missing-value range declarations).
    #[must_use]
    #[inline]
    pub fn lowest(&self) -> [u8; 8] {
        self.lowest
    }
}

/// Builder for [`FloatSentinels`].
#[derive(Debug, Default, Clone, Copy)]
pub struct FloatSentinelsBuilder {
    system_missing: Option<[u8; 8]>,
    highest: Option<[u8; 8]>,
    lowest: Option<[u8; 8]>,
}

impl FloatSentinelsBuilder {
    /// Sets the system-missing sentinel.
    #[must_use]
    #[inline]
    pub fn system_missing(mut self, bytes: [u8; 8]) -> Self {
        self.system_missing = Some(bytes);
        self
    }

    /// Sets the `HIGHEST` sentinel.
    #[must_use]
    #[inline]
    pub fn highest(mut self, bytes: [u8; 8]) -> Self {
        self.highest = Some(bytes);
        self
    }

    /// Sets the `LOWEST` sentinel.
    #[must_use]
    #[inline]
    pub fn lowest(mut self, bytes: [u8; 8]) -> Self {
        self.lowest = Some(bytes);
        self
    }

    /// Finalizes this builder into a [`FloatSentinels`].
    ///
    /// Unset sentinels default to all-zero bytes.
    #[must_use]
    #[inline]
    pub fn build(self) -> FloatSentinels {
        let system_missing = self.system_missing.unwrap_or([0; 8]);
        let highest = self.highest.unwrap_or([0; 8]);
        let lowest = self.lowest.unwrap_or([0; 8]);
        FloatSentinels {
            system_missing,
            highest,
            lowest,
        }
    }
}

/// Reads a subtype-4 record from `envelope`, yielding the
/// [`FloatSentinels`] carried verbatim. Forwards the envelope's fields
/// to [`parse`].
#[inline]
pub(crate) fn read(envelope: &ExtensionEnvelope) -> Result<DictionaryRecord> {
    let sentinels = parse(
        envelope.element_size,
        envelope.element_count,
        &envelope.payload,
        envelope.element_size_position,
    )?;
    let record = ExtensionRecord::FloatInfo(sentinels);
    let extension = DictionaryRecord::Extension(record);
    Ok(extension)
}

/// Parses a subtype-4 payload (float sentinel values: system missing,
/// highest, lowest).
///
/// Validates the envelope against the subtype's spec shape
/// (`element_size == 8`, `element_count == 3`) and slices the 24-byte
/// payload into three 8-byte slabs. Bytes are carried verbatim — no
/// float-format decode is applied here, so the returned
/// [`FloatSentinels`] round-trips bit-exactly regardless of whether
/// the file uses IEEE 754, IBM HFP, or VAX.
///
/// # Errors
///
/// Returns [`FormatErrorKind::UnexpectedValue`](crate::spss::sav::sav_error::FormatErrorKind::UnexpectedValue)
/// when the envelope shape disagrees with the spec.
///
/// # Panics
///
/// Panics in debug builds if `payload.len()` does not equal `24`; the
/// caller reads the payload from the validated dimensions, so this is
/// a logic invariant.
fn parse(
    actual_size: u32,
    actual_count: u32,
    payload: &[u8],
    position: u64,
) -> Result<FloatSentinels> {
    validate_extension_shape(
        actual_size,
        actual_count,
        FLOAT_SENTINELS_ELEMENT_SIZE,
        FLOAT_SENTINELS_ELEMENT_COUNT,
        position,
    )?;
    debug_assert_eq!(payload.len(), 24);
    let slab = |offset: usize| -> [u8; 8] {
        payload[offset..offset + 8]
            .try_into()
            .expect("envelope validation guarantees a 24-byte payload")
    };
    let sentinels = FloatSentinels::builder()
        .system_missing(slab(FLOAT_SENTINELS_SYSTEM_MISSING_OFFSET))
        .highest(slab(FLOAT_SENTINELS_HIGHEST_OFFSET))
        .lowest(slab(FLOAT_SENTINELS_LOWEST_OFFSET))
        .build();
    Ok(sentinels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::byte_order::ByteOrder;
    use crate::spss::sav::sav_error::{Field, FormatErrorKind, SavError};
    use crate::spss::sav::test_support::{
        build_header, open, write_extension_record, write_terminator,
    };

    #[test]
    fn reader_float_sentinels() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        // Three known IEEE 754 bit patterns: a NaN (system missing in
        // IEEE files), -Inf (LOWEST), +Inf (HIGHEST).
        let sys = f64::from_bits(0xFFF8_0000_0000_0000);
        let high = f64::INFINITY;
        let low = f64::NEG_INFINITY;
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&sys.to_le_bytes());
        payload.extend_from_slice(&high.to_le_bytes());
        payload.extend_from_slice(&low.to_le_bytes());
        write_extension_record(&mut bytes, byte_order, 4, 8, 3, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let record = dict.read_record().unwrap().unwrap();
        let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(sentinels)) = record else {
            panic!("expected FloatInfo, got {record:?}");
        };
        assert_eq!(sentinels.system_missing(), sys.to_le_bytes());
        assert_eq!(sentinels.highest(), high.to_le_bytes());
        assert_eq!(sentinels.lowest(), low.to_le_bytes());
        assert!(dict.warnings().is_empty());
    }

    #[test]
    fn reader_preserves_arbitrary_bit_patterns() {
        // Use a non-canonical NaN to confirm the bytes are not
        // normalized through any IEEE decode path.
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        let sys = [0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x7F];
        let high = [0xDE, 0xAD, 0xBE, 0xEF, 0xDE, 0xAD, 0xBE, 0xEF];
        let low = [0xFE, 0xED, 0xFA, 0xCE, 0xFE, 0xED, 0xFA, 0xCE];
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&sys);
        payload.extend_from_slice(&high);
        payload.extend_from_slice(&low);
        write_extension_record(&mut bytes, byte_order, 4, 8, 3, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(sentinels)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected FloatInfo");
        };
        assert_eq!(sentinels.system_missing(), sys);
        assert_eq!(sentinels.highest(), high);
        assert_eq!(sentinels.lowest(), low);
    }

    #[test]
    fn reader_carries_bytes_verbatim_regardless_of_byte_order() {
        // Even with big-endian header byte order, the sentinel bytes
        // are stored verbatim — no byte-swapping is applied here
        // because the float-format decode is the consumer's concern.
        let byte_order = ByteOrder::BigEndian;
        let mut bytes = build_header(byte_order);
        let payload: [u8; 24] = std::array::from_fn(|i| u8::try_from(i).unwrap());
        write_extension_record(&mut bytes, byte_order, 4, 8, 3, &payload);
        write_terminator(&mut bytes, byte_order);

        let mut dict = open(bytes);
        let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(sentinels)) =
            dict.read_record().unwrap().unwrap()
        else {
            panic!("expected FloatInfo");
        };
        assert_eq!(sentinels.system_missing(), [0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(sentinels.highest(), [8, 9, 10, 11, 12, 13, 14, 15]);
        assert_eq!(sentinels.lowest(), [16, 17, 18, 19, 20, 21, 22, 23]);
    }

    #[test]
    fn reader_wrong_element_size_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 4, 4, 3, &[0; 12]);

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
    fn reader_wrong_element_count_errors() {
        let byte_order = ByteOrder::LittleEndian;
        let mut bytes = build_header(byte_order);
        write_extension_record(&mut bytes, byte_order, 4, 8, 2, &[0; 16]);

        let mut dict = open(bytes);
        let err = dict.read_record().unwrap_err();
        match err {
            SavError::Format(e) => assert_eq!(
                e.kind(),
                FormatErrorKind::UnexpectedValue {
                    field: Field::ExtensionElementCount,
                }
            ),
            _ => panic!("expected Format error, got {err:?}"),
        }
    }
}
