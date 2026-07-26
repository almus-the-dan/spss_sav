//! A variable's missing-value specification.

use crate::spss::sav::dictionary_format::MISSING_VALUE_COUNT_MAX;
use crate::spss::sav::range_bound::RangeBound;
use crate::spss::sav::sav_error::{Result, SavError};

/// The missing-value specification carried by a numeric variable.
///
/// A variable can declare 0–3 discrete missing values, an inclusive
/// range, or a range plus one additional discrete value.
///
/// Use [`discrete`](Self::discrete) to construct the
/// [`Discrete`](Self::Discrete) variant — that constructor enforces
/// the SAV-format three-value cap.
#[derive(Debug, Clone, PartialEq)]
pub enum MissingValueSpecification {
    /// No declared missing values.
    None,
    /// 1–3 discrete missing values.
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
    fn discrete_rejects_many_values() {
        let err = MissingValueSpecification::discrete(vec![0.0; 100]).unwrap_err();
        assert!(matches!(
            err,
            SavError::TooManyMissingValues { actual: 100 }
        ));
    }
}
