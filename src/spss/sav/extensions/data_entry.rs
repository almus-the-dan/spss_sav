//! Subtype 15 — SPSS Data Entry product information.

/// SPSS Data Entry product metadata from extension record subtype
/// 15.
///
/// This subtype is product-specific and rarely seen in
/// general-purpose SAV files. The reader preserves the raw payload
/// for round-trip fidelity rather than parsing the SPSS Data Entry
/// internals.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct DataEntry {}
