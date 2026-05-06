//! Subtype 6 — float-format confirmation.

/// Float-format confirmation from extension record subtype 6.
///
/// Redundantly carries the system-missing, highest, and lowest
/// values that subtype 4 already declares; SPSS emits both for
/// cross-check. The reader exposes both rather than collapsing them.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct MachineFloatInfo {}
