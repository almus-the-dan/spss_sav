//! A variable's missing-value specification.

use crate::spss::sav::dictionary_format::MISSING_VALUE_COUNT_MAX;
use crate::spss::sav::range_bound::RangeBound;
use crate::spss::sav::sav_error::{Result, SavError};

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

    #[test]
    fn discrete_rejects_many_values() {
        let err = MissingValueSpecification::discrete(vec![0.0; 100]).unwrap_err();
        assert!(matches!(
            err,
            SavError::TooManyMissingValues { actual: 100 }
        ));
    }
}
