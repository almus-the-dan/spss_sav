//! Successfully parsed SAV creation timestamp.

/// The components of a successfully parsed SAV creation timestamp.
///
/// All fields are stored as the raw on-disk values — `year` is the
/// two-digit value (0–99) before any base-year is applied, and no
/// calendar validation is performed at construction time. Use the
/// chrono adapter (gated on the `chrono` feature) to get a
/// validated `NaiveDateTime`.
///
/// Fields land in Phase 4.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParsedSavTimestamp {}
