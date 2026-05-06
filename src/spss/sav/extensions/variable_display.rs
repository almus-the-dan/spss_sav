//! Subtype 11 — per-variable display parameters.

/// Display parameters for a single variable: measurement level,
/// display width, and alignment.
///
/// Subtype 11 carries one of these per variable in declaration
/// order; consumers re-attach them to their corresponding
/// `SavVariable` during schema finalization.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct VariableDisplay {}
