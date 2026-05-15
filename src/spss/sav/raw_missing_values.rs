//! Wire-level missing-value bytes from a single SAV variable record.

/// The raw missing-value bytes carried in a type-2 variable record.
///
/// This is the wire-level shape: the SAV format reserves up to three
/// 8-byte slots that follow the variable record's body (and any
/// label block), encoded according to the variable's storage type —
/// `f64` in the file's byte order for numeric variables, padded byte
/// strings for string variables. No interpretation has been applied
/// yet: in particular, the numeric sentinel substitution for
/// system-missing / `HIGHEST` / `LOWEST` happens only when the
/// dictionary reader's finalization pass materializes a
/// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable) (the
/// sentinel values come from extension subtype 4, which may not yet
/// have been read).
///
/// `RawMissingValues` is carried on
/// [`SavVariableHeader`](crate::spss::sav::sav_variable_header::SavVariableHeader)
/// for round-trip fidelity. The user-facing decoded form lives on
/// [`SavVariable::missing_value_spec`](crate::spss::sav::sav_variable::SavVariable).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum RawMissingValues {
    /// No missing values (wire encoding: `n_missing_values == 0`).
    #[default]
    None,
    /// One to three discrete missing values, in their on-disk order
    /// (wire encoding: `n_missing_values ∈ 1..=3`).
    ///
    /// The contained [`Vec`] is always non-empty (use
    /// [`None`](Self::None) for the no-missing case) and never longer
    /// than three entries.
    Discrete(Vec<[u8; 8]>),
    /// A single low/high range (wire encoding: `n_missing_values ==
    /// -2`).
    Range {
        /// Low endpoint of the range, inclusive.
        low: [u8; 8],
        /// High endpoint of the range, inclusive.
        high: [u8; 8],
    },
    /// A low/high range plus one standalone discrete value (wire
    /// encoding: `n_missing_values == -3`).
    RangeWithDiscrete {
        /// Low endpoint of the range, inclusive.
        low: [u8; 8],
        /// High endpoint of the range, inclusive.
        high: [u8; 8],
        /// Standalone discrete missing value.
        discrete: [u8; 8],
    },
}
