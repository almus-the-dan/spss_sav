//! Storage type of a SAV variable.

use core::fmt;

/// Storage type of a SAV variable.
///
/// SAV recognises two storage classes: numeric (one `f64` per cell)
/// and string (a fixed-width byte slot per cell). String widths are
/// 1–32,767 bytes; widths above 255 are stored as segmented "very
/// long strings" via the long-string extension record (subtype 14),
/// but that segmentation is an internal reader/writer detail and the
/// public type stays the same.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VariableType {
    /// 8-byte IEEE 754 double-precision numeric.
    Numeric,
    /// Fixed-width string with the given maximum byte length.
    String(u16),
}

impl fmt::Display for VariableType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Numeric => f.write_str("numeric"),
            Self::String(width) => write!(f, "string({width})"),
        }
    }
}
