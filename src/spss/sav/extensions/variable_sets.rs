//! Subtype 7 — named variable groupings.

/// Named variable groupings declared by extension record subtype 7.
///
/// SPSS uses these to organize variables into thematic sets in the
/// dataset editor. The on-disk format is a single text payload with
/// per-set line structure; the reader exposes the parsed structure
/// rather than the raw text.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct VariableSets {}
