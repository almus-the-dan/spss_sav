//! A numeric SAV cell value.

use crate::spss::missing_value::MissingValue;

/// A numeric cell value: either a present `f64` or a missing-value
/// designation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Numeric {
    /// A present data value.
    Present(f64),
    /// A missing value (system or user-defined).
    Missing(MissingValue),
}

impl Numeric {
    /// Returns the underlying `f64` for [`Present`](Self::Present),
    /// or `None` for any [`Missing`](Self::Missing) variant.
    #[must_use]
    #[inline]
    pub fn present(self) -> Option<f64> {
        match self {
            Self::Present(value) => Some(value),
            Self::Missing(_) => None,
        }
    }
}
