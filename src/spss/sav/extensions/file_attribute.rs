//! Subtype 18 — file-level custom attributes.

/// One file-level custom attribute from extension record subtype
/// 18.
///
/// File attributes are arbitrary `(name, value)` pairs attached to
/// the dataset as a whole rather than to a specific variable.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct FileAttribute {}
