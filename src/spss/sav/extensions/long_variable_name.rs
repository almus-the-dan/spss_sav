//! Subtype 13 — long-variable-name mapping.

/// One mapping from a short (8-byte) variable name to its full
/// long name.
///
/// SPSS stores variable names in two places: the schema's variable
/// records (capped at 8 bytes) and this extension record (up to 64
/// bytes). The reader pairs them so each `SavVariable` carries both
/// a `short_name` and a `long_name` after schema finalization.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct LongVariableName {}
