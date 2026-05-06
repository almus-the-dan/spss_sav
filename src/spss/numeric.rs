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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn present_returns_inner_value() {
        assert_eq!(Numeric::Present(1.5).present(), Some(1.5));
    }

    #[test]
    fn present_returns_none_for_system_missing() {
        assert_eq!(Numeric::Missing(MissingValue::System).present(), None);
    }

    #[test]
    fn present_returns_none_for_user_defined_missing() {
        assert_eq!(
            Numeric::Missing(MissingValue::UserDefined(99.0)).present(),
            None,
        );
    }

    #[test]
    fn present_negative_zero_returns_some() {
        assert_eq!(Numeric::Present(-0.0).present(), Some(-0.0));
    }
}
