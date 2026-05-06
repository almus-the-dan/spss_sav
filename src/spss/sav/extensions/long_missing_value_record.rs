//! Subtype 22 — long missing values.

/// One long-missing-value record from extension record subtype 22.
///
/// Subtype 22 carries user-defined missing values for very-long-string
/// variables; the schema's per-variable `MissingValueSpec` slot is
/// limited to 8-byte keys and cannot represent these directly.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct LongMissingValueRecord {}
