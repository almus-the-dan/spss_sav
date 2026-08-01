//! An undecoded type-3 / type-4 value-label record pair.

use crate::spss::sav::buffered_value_label_entry::BufferedValueLabelEntry;

/// A type-3 value-label record together with its paired type-4
/// variable-index record, held undecoded.
///
/// Both the strict type-3-then-type-4 adjacency rule and the index
/// translation are handled while buffering: the pair has to be traversed
/// as a unit to find where it ends, and the `primaries` map needed to
/// turn 1-based physical indices into 0-based segment ones is complete
/// by then. Only the entry labels are left undecoded.
#[derive(Debug)]
pub(crate) struct BufferedValueLabelSet {
    /// Entries in the order the type-3 record listed them.
    pub(crate) entries: Vec<BufferedValueLabelEntry>,
    /// Target indices, already normalized to 0-based segment
    /// positions with continuations excluded.
    pub(crate) segment_indices: Vec<u32>,
}
