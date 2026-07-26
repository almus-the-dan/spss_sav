//! The decoded header of a type-7 extension record.

use crate::spss::sav::byte_order::ByteOrder;

/// The fixed header fields of a type-7 extension record plus its
/// payload, decoded once by
/// [`DictionaryReader::read_extension_record`](crate::spss::sav::dictionary_reader)
/// and consumed by the per-subtype `read` helper co-located with each
/// extension type, which extracts whichever fields its subtype
/// requires.
///
/// Deliberately carries no encoding. The encoding is not known until
/// the whole dictionary has been scanned, so an envelope that owned one
/// could only ever hold a stale guess; the resolved encoding is passed
/// to each `read` helper as an argument instead.
#[derive(Debug)]
pub(crate) struct ExtensionEnvelope {
    /// The 4-byte subtype code identifying the extension.
    pub(crate) subtype: i32,
    /// The declared size of each element, in bytes.
    pub(crate) element_size: u32,
    /// The declared number of elements.
    pub(crate) element_count: u32,
    /// `element_size` as a `usize`.
    pub(crate) element_size_usize: usize,
    /// `element_count` as a `usize`.
    pub(crate) element_count_usize: usize,
    /// Stream position of the `element_size` field, for error
    /// reporting.
    pub(crate) element_size_position: u64,
    /// The `element_size * element_count`-byte payload.
    pub(crate) payload: Vec<u8>,
    /// The file's byte order, for decoding multi-byte payload fields.
    pub(crate) byte_order: ByteOrder,
}
