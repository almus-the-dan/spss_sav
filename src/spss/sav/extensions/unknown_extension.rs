//! Catch-all extension record for unrecognized subtypes.

/// An extension record carrying a subtype this library does not yet
/// recognize.
///
/// The raw bytes are preserved verbatim. The reader emits a
/// [`SavWarning::UnknownExtensionSubtype`](crate::spss::sav::sav_warning::SavWarning::UnknownExtensionSubtype)
/// when one of these is encountered, and the writer accepts it as
/// input so a round-trip preserves bit-for-bit the original record.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct UnknownExtension {}
