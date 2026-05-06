//! Typed key for a value-label entry.

use encoding_rs::Encoding;

use crate::spss::sav::sav_error::{Result, SavError};

const VALUE_LABEL_KEY_WIDTH: usize = 8;

/// The 8-byte key of a value-label entry.
///
/// SAV value-label keys are always eight bytes on disk: an `f64`
/// for numeric variables, or a fixed eight-byte string slot
/// (right-padded with spaces) for short-string variables.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ValueLabelValue {
    /// Numeric key.
    Numeric(f64),
    /// String key — eight raw bytes from the file, in the file's
    /// declared encoding.
    String([u8; VALUE_LABEL_KEY_WIDTH]),
}

impl ValueLabelValue {
    /// Constructs a [`String`](Self::String) key by encoding `s` with
    /// `encoding`. The encoded bytes are right-padded with spaces
    /// (`0x20`) to fill the eight-byte slot.
    ///
    /// # Errors
    ///
    /// - [`SavError::InvalidEncoding`] when `s` contains characters
    ///   that cannot be represented in `encoding`.
    /// - [`SavError::StringTooLong`] when the encoded form exceeds
    ///   eight bytes.
    ///
    /// [`SavError::StringTooLong`]: crate::spss::sav::sav_error::SavError::StringTooLong
    /// [`SavError::InvalidEncoding`]: crate::spss::sav::sav_error::SavError::InvalidEncoding
    pub fn from_str(s: &str, encoding: &'static Encoding) -> Result<Self> {
        let (encoded, _, had_unmappable) = encoding.encode(s);
        if had_unmappable {
            return Err(SavError::InvalidEncoding);
        }
        if encoded.len() > VALUE_LABEL_KEY_WIDTH {
            return Err(SavError::StringTooLong {
                actual: encoded.len(),
            });
        }
        let mut bytes = [b' '; VALUE_LABEL_KEY_WIDTH];
        bytes[..encoded.len()].copy_from_slice(&encoded);
        Ok(Self::String(bytes))
    }
}

/// Compares two [`ValueLabelValue`]s using IEEE 754 bit-pattern
/// equality on the numeric variant.
///
/// Crate-internal — keeps the bit-pattern semantics off
/// [`ValueLabelValue`]'s public API while providing one canonical
/// implementation for both [`ValueLabelSet::label_for`] and the
/// cache-key wrapper inside `value_label_table`.
///
/// [`ValueLabelSet::label_for`]: crate::spss::sav::value_label_set::ValueLabelSet::label_for
pub(crate) fn bit_equals(a: &ValueLabelValue, b: &ValueLabelValue) -> bool {
    match (a, b) {
        (ValueLabelValue::Numeric(av), ValueLabelValue::Numeric(bv)) => {
            av.to_bits() == bv.to_bits()
        }
        (ValueLabelValue::String(ab), ValueLabelValue::String(bb)) => ab == bb,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use encoding_rs::{UTF_8, WINDOWS_1252};

    #[test]
    fn from_str_short_pads_with_spaces() {
        let v = ValueLabelValue::from_str("ab", UTF_8).unwrap();
        let expected = [b'a', b'b', b' ', b' ', b' ', b' ', b' ', b' '];
        assert_eq!(v, ValueLabelValue::String(expected));
    }

    #[test]
    fn from_str_exactly_eight_bytes() {
        let v = ValueLabelValue::from_str("abcdefgh", UTF_8).unwrap();
        let expected = *b"abcdefgh";
        assert_eq!(v, ValueLabelValue::String(expected));
    }

    #[test]
    fn from_str_empty_is_all_spaces() {
        let v = ValueLabelValue::from_str("", UTF_8).unwrap();
        assert_eq!(v, ValueLabelValue::String([b' '; 8]));
    }

    #[test]
    fn from_str_too_long_errors() {
        let err = ValueLabelValue::from_str("abcdefghi", UTF_8).unwrap_err();
        assert!(matches!(err, SavError::StringTooLong { actual: 9 }));
    }

    #[test]
    fn from_str_unmappable_errors() {
        // CJK character isn't representable in Windows-1252.
        let err = ValueLabelValue::from_str("中", WINDOWS_1252).unwrap_err();
        assert!(matches!(err, SavError::InvalidEncoding));
    }

    #[test]
    fn from_str_utf8_multibyte_within_eight_bytes() {
        // "café" in UTF-8: c=0x63, a=0x61, f=0x66, é=0xC3 0xA9 → 5 bytes
        let v = ValueLabelValue::from_str("café", UTF_8).unwrap();
        let expected = [0x63, 0x61, 0x66, 0xC3, 0xA9, b' ', b' ', b' '];
        assert_eq!(v, ValueLabelValue::String(expected));
    }

    #[test]
    fn from_str_windows_1252_with_accent_fits() {
        // "café" in Windows-1252: c=0x63, a=0x61, f=0x66, é=0xE9 → 4 bytes
        let v = ValueLabelValue::from_str("café", WINDOWS_1252).unwrap();
        let expected = [0x63, 0x61, 0x66, 0xE9, b' ', b' ', b' ', b' '];
        assert_eq!(v, ValueLabelValue::String(expected));
    }

    #[test]
    fn bit_equals_numeric_same_bits() {
        let a = ValueLabelValue::Numeric(1.5);
        let b = ValueLabelValue::Numeric(1.5);
        assert!(bit_equals(&a, &b));
    }

    #[test]
    fn bit_equals_numeric_distinguishes_pos_neg_zero() {
        // 0.0 == -0.0 in IEEE but their bit patterns differ.
        let a = ValueLabelValue::Numeric(0.0);
        let b = ValueLabelValue::Numeric(-0.0);
        assert!(!bit_equals(&a, &b));
    }

    #[test]
    fn bit_equals_nan_matches_same_bit_pattern() {
        let nan_a = ValueLabelValue::Numeric(f64::from_bits(0x7FF8_0000_0000_0001));
        let nan_b = ValueLabelValue::Numeric(f64::from_bits(0x7FF8_0000_0000_0001));
        assert!(bit_equals(&nan_a, &nan_b));
    }

    #[test]
    fn bit_equals_different_nans_dont_match() {
        let nan_a = ValueLabelValue::Numeric(f64::from_bits(0x7FF8_0000_0000_0001));
        let nan_b = ValueLabelValue::Numeric(f64::from_bits(0x7FF8_0000_0000_0002));
        assert!(!bit_equals(&nan_a, &nan_b));
    }

    #[test]
    fn bit_equals_string_match() {
        let a = ValueLabelValue::String(*b"Male    ");
        let b = ValueLabelValue::String(*b"Male    ");
        assert!(bit_equals(&a, &b));
    }

    #[test]
    fn bit_equals_numeric_and_string_never_match() {
        let a = ValueLabelValue::Numeric(0.0);
        let b = ValueLabelValue::String([0; 8]);
        assert!(!bit_equals(&a, &b));
    }
}
