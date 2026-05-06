//! A variable's missing-value specification.

use crate::spss::sav::range_bound::RangeBound;
use crate::spss::sav::sav_error::{Result, SavError};

/// Maximum number of discrete missing values the SAV format permits.
const MAX_DISCRETE: usize = 3;

/// The missing-value specification carried by a numeric variable.
///
/// A variable can declare 0–3 discrete missing values, an inclusive
/// range, or a range plus one additional discrete value.
///
/// Use [`discrete`](Self::discrete) to construct the
/// [`Discrete`](Self::Discrete) variant — that constructor enforces
/// the SAV-format three-value cap.
#[derive(Debug, Clone, PartialEq)]
pub enum MissingValueSpec {
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

impl MissingValueSpec {
    /// Constructs a [`Discrete`](Self::Discrete) spec from a list of
    /// values.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::TooManyMissingValues`] if `values`
    /// contains more than three entries.
    pub fn discrete(values: Vec<f64>) -> Result<Self> {
        if values.len() > MAX_DISCRETE {
            return Err(SavError::TooManyMissingValues {
                actual: values.len(),
            });
        }
        Ok(Self::Discrete(values))
    }
}
