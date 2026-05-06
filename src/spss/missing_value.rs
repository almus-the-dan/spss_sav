//! A single cell's missing-value designation.

/// A missing value carried in a numeric cell.
///
/// SPSS distinguishes the system missing value from user-defined
/// missing values. Only [`UserDefined`](Self::UserDefined) preserves
/// the underlying `f64` payload — the system missing value is written
/// to disk as a sentinel bit pattern and represented here as the bare
/// [`System`](Self::System) variant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MissingValue {
    /// The system missing value.
    System,
    /// A user-declared missing value, carrying its on-disk payload.
    UserDefined(f64),
}

impl MissingValue {
    /// Returns the `f64` payload for [`UserDefined`](Self::UserDefined),
    /// or `None` for [`System`](Self::System).
    #[must_use]
    #[inline]
    pub fn user_defined(self) -> Option<f64> {
        match self {
            Self::System => None,
            Self::UserDefined(value) => Some(value),
        }
    }
}
