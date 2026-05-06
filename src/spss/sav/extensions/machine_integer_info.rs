//! Subtype 5 — integer-typed environment metadata.

/// Integer-typed environment metadata from extension record subtype
/// 5: version numbers, machine code, floating-point representation,
/// compression code, endianness, and character encoding code.
///
/// Some fields here duplicate information already carried in the
/// header or other extension records; the reader exposes both
/// without trying to reconcile them.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct MachineIntegerInfo {}
