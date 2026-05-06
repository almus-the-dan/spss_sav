//! Subtype 17 — per-variable custom attributes.

/// One subtype-17 record carrying a single variable's custom
/// attributes.
///
/// Distinct from
/// [`VariableAttribute`](crate::spss::sav::variable_attribute::VariableAttribute):
/// `VariableAttribute` is the user-facing `(name, value)` pair on
/// `SavVariable`, whereas `VariableAttributeRecord` is the
/// wire-level extension-record entry that pairs a variable index
/// with a list of those attributes.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct VariableAttributeRecord {}
