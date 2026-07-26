//! An undecoded type-6 document record.

use crate::spss::sav::dictionary_format::DOCUMENT_LINE_LEN;

/// The fixed-width document lines of a type-6 record, held undecoded.
///
/// Every line is exactly `DOCUMENT_LINE_LEN` bytes on disk, so nothing
/// about this record can fail to traverse once the line count is read.
/// Trailing padding is trimmed when the lines are decoded.
#[allow(dead_code)] // populated when the header reader defers decoding.
pub(crate) struct BufferedDocumentRecord {
    /// Raw document lines in file order.
    pub(crate) lines: Vec<[u8; DOCUMENT_LINE_LEN]>,
}
