//! Subtype 14 — very-long-string widths.

/// One declaration of a string variable's logical width when that
/// width exceeds 255 bytes.
///
/// On disk, very long strings are split into multiple
/// continuation-bearing 255-byte segments at the schema level;
/// subtype 14 records the original logical width so the reader can
/// reconstruct the user-facing variable.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct VeryLongString {}
