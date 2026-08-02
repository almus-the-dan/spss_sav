//! A variable's missing-value specification.

use crate::spss::sav::dictionary_format::MISSING_VALUE_COUNT_MAX;
use crate::spss::sav::range_bound::RangeBound;
use crate::spss::sav::sav_error::{Result, SavError};
use crate::spss::sav::text_field::trim_trailing_padding;

/// The missing-value specification carried by a variable.
///
/// A numeric variable can declare 0–3 discrete missing values, an
/// inclusive range, or a range plus one additional discrete value. A
/// string variable can only declare discrete values, and they are raw
/// bytes rather than numbers — hence the separate
/// [`String`](Self::String) variant.
///
/// Use [`discrete`](Self::discrete) to construct the
/// [`Discrete`](Self::Discrete) variant — that constructor enforces
/// the SAV-format three-value cap.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum MissingValueSpecification {
    /// No declared missing values.
    None,
    /// 1–3 discrete numeric missing values.
    Discrete(Vec<f64>),
    /// A contiguous inclusive range, optionally with one additional
    /// discrete value outside the range.
    Range {
        /// Lower endpoint.
        low: RangeBound,
        /// Upper endpoint.
        high: RangeBound,
        /// Optional additional discrete missing value.
        extra: Option<f64>,
    },
    /// 1–3 discrete missing values for a string variable, as raw bytes
    /// in the file's declared encoding.
    ///
    /// Width depends on where the declaration came from. A short string
    /// declares them in its type-2 record, eight bytes each. A very
    /// long string declares them in extension subtype 22, which is also
    /// eight bytes each — SPSS compares only a long string's first
    /// eight bytes, whatever its declared width. Comparison is
    /// byte-for-byte against the cell's leading bytes, which is why
    /// these stay undecoded — see
    /// [`ValueLabelValue`](crate::spss::sav::value_label_value::ValueLabelValue),
    /// whose keys are raw for the same reason.
    ///
    /// Each value is a boxed slice because it is fixed once read.
    String(Vec<Box<[u8]>>),
}

impl MissingValueSpecification {
    /// Constructs a [`Discrete`](Self::Discrete) spec from a list of
    /// values.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::TooManyMissingValues`] if `values`
    /// contains more than three entries.
    pub fn discrete(values: Vec<f64>) -> Result<Self> {
        if values.len() > MISSING_VALUE_COUNT_MAX {
            return Err(SavError::TooManyMissingValues {
                actual: values.len(),
            });
        }
        Ok(Self::Discrete(values))
    }

    /// Constructs a [`String`](Self::String) spec from a list of raw
    /// byte values.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::TooManyMissingValues`] if `values`
    /// contains more than three entries.
    pub fn string(values: Vec<Box<[u8]>>) -> Result<Self> {
        if values.len() > MISSING_VALUE_COUNT_MAX {
            return Err(SavError::TooManyMissingValues {
                actual: values.len(),
            });
        }
        let result = Self::String(values);
        Ok(result)
    }

    /// Whether `value` is one of the numbers this variable declares
    /// missing.
    ///
    /// Says nothing about the system-missing value, which is a property
    /// of the cell rather than of the declaration and is recognized
    /// before this is ever consulted.
    ///
    /// A [`Range`](Self::Range) is inclusive at both ends, and an
    /// [`Unbounded`](RangeBound::Unbounded) end — written on disk as the
    /// `LOWEST` or `HIGHEST` sentinel — is open, so `LOWEST THRU 0`
    /// matches every number up to and including zero.
    #[must_use]
    pub fn matches_number(&self, value: f64) -> bool {
        match self {
            Self::None | Self::String(_) => false,
            Self::Discrete(values) => values.contains(&value),
            Self::Range { low, high, extra } => {
                in_range(*low, *high, value) || extra.as_slice().contains(&value)
            }
        }
    }

    /// Whether `raw` — a string cell's bytes — is one of the values
    /// this variable declares missing.
    ///
    /// Only the leading [`STRING_COMPARISON_LEN`] bytes take part,
    /// whatever the variable's declared width: SPSS stores a string
    /// missing value in an eight-byte slot and compares just that much,
    /// so a 300-wide variable's missing values are decided by the first
    /// eight bytes of a cell. Verified on a PSPP file — a
    /// `MISSING VALUES longstr ('beta')` on an `A300` reaches extension
    /// subtype 22 as exactly `b"beta    "`.
    ///
    /// Padding is trimmed from both sides before comparing, because the
    /// two sides are not padded the same way: PSPP writes a short
    /// string's key space-padded to the declared width and then
    /// NUL-padded to eight (`b"cc  \0\0\0\0"` for an `A4`), while
    /// subtype 22 pads with spaces throughout.
    #[must_use]
    pub fn matches_bytes(&self, raw: &[u8]) -> bool {
        let Self::String(keys) = self else {
            return false;
        };
        let cell = comparison_key(raw);
        keys.iter().any(|key| comparison_key(key) == cell)
    }
}

/// Leading bytes of a string cell that a missing-value comparison
/// considers, whatever the variable's declared width.
pub const STRING_COMPARISON_LEN: usize = 8;

/// Whether `value` falls inside an inclusive range with optionally open
/// ends.
fn in_range(low: RangeBound, high: RangeBound, value: f64) -> bool {
    let at_or_above = match low {
        RangeBound::Unbounded => true,
        RangeBound::Inclusive(low) => value >= low,
    };
    let at_or_below = match high {
        RangeBound::Unbounded => true,
        RangeBound::Inclusive(high) => value <= high,
    };
    at_or_above && at_or_below
}

/// The bytes a string comparison actually runs on: the leading
/// [`STRING_COMPARISON_LEN`], with padding trimmed.
fn comparison_key(bytes: &[u8]) -> &[u8] {
    let head = &bytes[..bytes.len().min(STRING_COMPARISON_LEN)];
    trim_trailing_padding(head)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discrete_accepts_zero_values() {
        let spec = MissingValueSpecification::discrete(vec![]).unwrap();
        assert_eq!(spec, MissingValueSpecification::Discrete(vec![]));
    }

    #[test]
    fn discrete_accepts_one_value() {
        let spec = MissingValueSpecification::discrete(vec![9.0]).unwrap();
        assert_eq!(spec, MissingValueSpecification::Discrete(vec![9.0]));
    }

    #[test]
    fn discrete_accepts_three_values() {
        let spec = MissingValueSpecification::discrete(vec![1.0, 2.0, 3.0]).unwrap();
        assert_eq!(
            spec,
            MissingValueSpecification::Discrete(vec![1.0, 2.0, 3.0])
        );
    }

    #[test]
    fn discrete_rejects_four_values() {
        let err = MissingValueSpecification::discrete(vec![1.0, 2.0, 3.0, 4.0]).unwrap_err();
        assert!(matches!(err, SavError::TooManyMissingValues { actual: 4 }));
    }

    #[test]
    fn string_accepts_up_to_three_values() {
        let values = vec![
            b"alpha   ".to_vec().into_boxed_slice(),
            b"beta    ".to_vec().into_boxed_slice(),
            b"gamma   ".to_vec().into_boxed_slice(),
        ];
        let spec = MissingValueSpecification::string(values.clone()).unwrap();
        assert_eq!(spec, MissingValueSpecification::String(values));
    }

    #[test]
    fn string_rejects_four_values() {
        let values = vec![b"a".to_vec().into_boxed_slice(); 4];
        let err = MissingValueSpecification::string(values).unwrap_err();
        assert!(matches!(err, SavError::TooManyMissingValues { actual: 4 }));
    }

    // ---- matching ----------------------------------------------------

    fn keys(values: &[&[u8]]) -> MissingValueSpecification {
        let boxed = values
            .iter()
            .map(|v| v.to_vec().into_boxed_slice())
            .collect();
        MissingValueSpecification::String(boxed)
    }

    #[test]
    fn none_matches_nothing() {
        let spec = MissingValueSpecification::None;
        assert!(!spec.matches_number(0.0));
        assert!(!spec.matches_bytes(b"cc"));
    }

    #[test]
    fn discrete_matches_only_the_declared_numbers() {
        let spec = MissingValueSpecification::Discrete(vec![151.0, 99.0]);
        assert!(spec.matches_number(151.0));
        assert!(spec.matches_number(99.0));
        assert!(!spec.matches_number(150.0));
        assert!(!spec.matches_number(0.0));
    }

    /// `LOWEST THRU 0`, the shape PSPP writes for an open-ended range.
    #[test]
    fn an_open_low_range_matches_everything_up_to_its_closed_end() {
        let spec = MissingValueSpecification::Range {
            low: RangeBound::Unbounded,
            high: RangeBound::Inclusive(0.0),
            extra: None,
        };
        assert!(spec.matches_number(f64::MIN));
        assert!(spec.matches_number(-999_999.0));
        assert!(spec.matches_number(0.0), "the closed end is inclusive");
        assert!(!spec.matches_number(0.5));
    }

    #[test]
    fn a_closed_range_is_inclusive_at_both_ends() {
        let spec = MissingValueSpecification::Range {
            low: RangeBound::Inclusive(1.0),
            high: RangeBound::Inclusive(3.0),
            extra: None,
        };
        for value in [1.0, 2.0, 3.0] {
            assert!(spec.matches_number(value), "{value}");
        }
        for value in [0.999, 3.001] {
            assert!(!spec.matches_number(value), "{value}");
        }
    }

    #[test]
    fn a_ranges_extra_value_matches_from_outside_it() {
        let spec = MissingValueSpecification::Range {
            low: RangeBound::Inclusive(1.0),
            high: RangeBound::Inclusive(3.0),
            extra: Some(99.0),
        };
        assert!(spec.matches_number(99.0));
        assert!(!spec.matches_number(98.0));
    }

    /// The two sides are padded differently on disk — PSPP NUL-pads a
    /// short string's key out to eight bytes (`b"cc  \0\0\0\0"` for an
    /// `A4`) while subtype 22 space-pads (`b"beta    "`). Both must
    /// match a trimmed cell.
    #[test]
    fn string_keys_match_regardless_of_how_they_were_padded() {
        assert!(keys(&[b"cc  \0\0\0\0"]).matches_bytes(b"cc"));
        assert!(keys(&[b"beta    "]).matches_bytes(b"beta"));
        assert!(keys(&[b"cc"]).matches_bytes(b"cc      "));
    }

    /// SPSS compares only the leading eight bytes, whatever the width,
    /// so a cell agreeing for eight bytes matches even though the rest
    /// differs.
    #[test]
    fn only_the_leading_eight_bytes_take_part() {
        let spec = keys(&[b"abcdefgh"]);
        assert!(spec.matches_bytes(b"abcdefgh"));
        assert!(spec.matches_bytes(b"abcdefghIGNORED"));
        assert!(
            !spec.matches_bytes(b"abcdefg"),
            "seven bytes is a different key"
        );
    }

    #[test]
    fn a_numeric_spec_never_matches_bytes_and_the_reverse() {
        assert!(!MissingValueSpecification::Discrete(vec![1.0]).matches_bytes(b"1"));
        assert!(!keys(&[b"1"]).matches_number(1.0));
    }

    #[test]
    fn discrete_rejects_many_values() {
        let err = MissingValueSpecification::discrete(vec![0.0; 100]).unwrap_err();
        assert!(matches!(
            err,
            SavError::TooManyMissingValues { actual: 100 }
        ));
    }
}
