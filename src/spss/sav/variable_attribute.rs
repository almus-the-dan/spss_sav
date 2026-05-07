//! User-facing custom attribute on a SAV variable.

/// One custom `(name, value)` attribute attached to a SAV variable.
///
/// Variable attributes are user-defined free-form metadata stored in
/// extension record subtype 17. Attribute names are typically dotted
/// identifiers (e.g. `$@Role`); the value is a single string that may
/// encode arbitrary content.
///
/// Distinct from
/// [`VariableAttributeRecord`](crate::spss::sav::extensions::variable_attribute_record::VariableAttributeRecord)
/// — that type is the wire-level extension-record entry, whereas this
/// type is the per-variable user-facing pair attached to a
/// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable).
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct VariableAttribute {}
