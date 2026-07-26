//! An undecoded type-3 / type-4 value-label record pair.

use crate::spss::sav::buffered_value_label_entry::BufferedValueLabelEntry;

/// A type-3 value-label record together with its paired type-4
/// variable-index record, held undecoded.
///
/// The strict type-3-then-type-4 adjacency rule is enforced while
/// buffering, because the pair has to be traversed as a unit to find
/// where it ends. What is *not* checked here is whether the indices
/// point anywhere sensible: translating them from 1-based physical
/// positions to 0-based logical ones needs the variable records to have
/// been processed first, which happens at decode time.
#[allow(dead_code)] // populated when the header reader defers decoding.
pub(crate) struct BufferedValueLabelSet {
    /// Entries in the order the type-3 record listed them.
    pub(crate) entries: Vec<BufferedValueLabelEntry>,
    /// Variable indices from the paired type-4 record, still 1-based
    /// and physical — untranslated and unvalidated.
    pub(crate) variable_indices: Vec<u32>,
}
