//! Free-text document lines from a SAV file.

/// One document record from a SAV file.
///
/// Document records carry free-text annotation lines authored by the
/// user. Each line is fixed-width (80 bytes on disk, space-padded),
/// and the order of lines is preserved.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct DocumentRecord {}
