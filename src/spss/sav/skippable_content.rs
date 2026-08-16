//! Dictionary content a reader can be told not to retain.

use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;

/// One category of dictionary content that
/// [`SavReader::skip_dictionary_content`](crate::spss::sav::sav_reader::SavReader::skip_dictionary_content)
/// can turn off.
///
/// Skipping is a **memory** win, not an I/O one. The reader has no
/// [`Seek`](std::io::Seek) bound, so a skipped record's bytes are still
/// read off the wire — they are discarded through a bounded window
/// instead of being retained, decoded, and handed out. What it saves is
/// holding the whole dictionary in memory at once, which is what
/// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header)
/// otherwise does.
///
/// Two invariants hold for anything skipped:
///
/// 1. **Skipping never changes whether a file parses.** Structural
///    validation still runs for skipped records — type-3 entries are
///    still walked, type-3/type-4 pairing still enforced, type-4
///    indices still normalized and still able to raise
///    [`DanglingValueLabel`](crate::spss::sav::sav_error::FormatErrorKind::DanglingValueLabel).
///    Only retention changes. The one honest exception is per-subtype
///    *payload* validation of a skipped extension, which cannot run
///    because the payload is never parsed. Warnings that a skipped
///    record would have raised are suppressed along with it.
/// 2. **Skipping means "don't yield it, don't retain it for me" — not
///    "don't read it."** The reader still absorbs whatever the schema
///    and the data layout require, so no combination of skips can break
///    a data read. Subtypes 3, 4, 13, 14, 16, 20 and 22 are
///    load-bearing: skipping them stops them reaching the caller but the
///    reader still consumes what it needs from them.
///
/// Variable records are deliberately absent. They are unskippable by
/// construction — their trailing blocks must be parsed to find the next
/// record at all, and they define the data layout — so leaving them out
/// beats offering an option that silently does nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SkippableContent {
    /// Type-3 + type-4 value-label sets.
    ValueLabels,
    /// Type-6 document records.
    Documents,
    /// Type-7 extension records of one subtype.
    ///
    /// [`ExtensionSubtype::Unrecognized`] covers every subtype the
    /// library does not parse, as a group.
    Extension(ExtensionSubtype),
}
