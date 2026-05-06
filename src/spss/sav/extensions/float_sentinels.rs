//! Subtype 4 — float sentinel values.

/// Float sentinel values declared by extension record subtype 4.
///
/// Carries the file's system-missing bit pattern plus the `LOWEST`
/// and `HIGHEST` open-bound markers used by missing-value range
/// declarations. The system-missing payload is preserved as raw
/// bytes, so the byte-equality comparison stays unambiguous regardless of
/// the file's float format.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct FloatSentinels {}
