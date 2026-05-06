//! Subtype 21 — long value labels.

/// One long-value-label record from extension record subtype 21.
///
/// Subtype 21 carries value labels for very-long-string variables,
/// which subtype-3 short-string value labels cannot represent
/// because their key is fixed at 8 bytes.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct LongValueLabelRecord {}
