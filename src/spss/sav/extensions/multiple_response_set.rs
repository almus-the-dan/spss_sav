//! Subtypes 19 / 7B — multiple response sets (MRSETS).

/// One multiple response set declaration from extension record
/// subtype 19 (or its variant 7B for extended encoding).
///
/// MRSETS group variables that together represent answers to a
/// "select multiple" survey question. The set carries a name, a
/// type (multiple-dichotomy or multiple-category), and the
/// participating variables.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct MultipleResponseSet {}
