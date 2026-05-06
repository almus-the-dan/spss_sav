//! Endpoint of a missing-value range.

/// Endpoint of a missing-value range.
///
/// Modeled as a purpose-built two-variant enum rather than
/// [`std::ops::Bound`] (which carries an unused `Excluded`) or a raw
/// `f64` (which would conflict with SPSS's `LOWEST` and `HIGHEST`
/// sentinels written to disk for "open" endpoints).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RangeBound {
    /// Unbounded endpoint — written to disk as the SPSS `LOWEST` or
    /// `HIGHEST` sentinel.
    Unbounded,
    /// Endpoint at exactly this value, inclusive.
    Inclusive(f64),
}
