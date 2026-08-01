//! Reader for the data-record section of a SAV file.
//!
//! Third and final phase of the reader typestate chain, reached from
//! [`DictionaryReader::into_record_reader`](crate::spss::sav::dictionary_reader::DictionaryReader::into_record_reader).
//!
//! Row decoding itself lands with Phase 6, alongside the bytecode and
//! ZLIB decoders. What exists now is the boundary: the reader holds
//! everything a row read needs, and the dictionary phase is finished.

use crate::spss::sav::data_layout::DataLayout;
use crate::spss::sav::encoding_provenance::EncodingProvenance;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_schema::SavSchema;
use crate::spss::sav::sav_warning::SavWarning;

/// Reader for the data-record section of a SAV file.
///
/// Created by
/// [`DictionaryReader::into_record_reader`](crate::spss::sav::dictionary_reader::DictionaryReader::into_record_reader),
/// which consumes any dictionary records the caller left unread and
/// finalizes the schema before transitioning.
#[derive(Debug)]
pub struct RecordReader<R> {
    state: ReaderState<R>,
    header: SavHeader,
    encoding_provenance: EncodingProvenance,
    layout: DataLayout,
    schema: Option<SavSchema>,
}

impl<R> RecordReader<R> {
    pub(crate) fn new(
        state: ReaderState<R>,
        header: SavHeader,
        encoding_provenance: EncodingProvenance,
        layout: DataLayout,
        schema: Option<SavSchema>,
    ) -> Self {
        Self {
            state,
            header,
            encoding_provenance,
            layout,
            schema,
        }
    }

    /// The file header.
    ///
    /// Identical to the one
    /// [`DictionaryReader::header`](crate::spss::sav::dictionary_reader::DictionaryReader::header)
    /// reports: every field on it comes from the 176-byte preamble and
    /// is complete the moment that has been read. Anything the
    /// dictionary had to supply lives on
    /// [`schema`](Self::schema) instead — including the weight
    /// variable, which the preamble stores only as a row offset.
    #[must_use]
    #[inline]
    pub fn header(&self) -> &SavHeader {
        &self.header
    }

    /// The finalized schema, or `None` when the caller turned schema
    /// building off with
    /// [`SavReader::build_schema`](crate::spss::sav::sav_reader::SavReader::build_schema).
    ///
    /// Rows read the same either way — the layout they are read through
    /// is accumulated separately and is always complete.
    #[must_use]
    #[inline]
    pub fn schema(&self) -> Option<&SavSchema> {
        self.schema.as_ref()
    }

    /// The encoding the reader applied, and where it came from.
    #[must_use]
    #[inline]
    pub fn encoding_provenance(&self) -> EncodingProvenance {
        self.encoding_provenance
    }

    /// How many rows the file claims to hold, or `None` when it did not
    /// say.
    ///
    /// Reports the subtype-16 extended count in preference to the
    /// header's 32-bit field, which is what that record exists for.
    #[must_use]
    #[inline]
    pub fn case_count(&self) -> Option<u64> {
        self.layout.case_count()
    }

    /// Warnings accumulated by the transition from the dictionary
    /// phase, and later by the most recent row read.
    #[must_use]
    #[inline]
    pub fn warnings(&self) -> &[SavWarning] {
        self.state.warnings()
    }

    /// The data layout rows are read through.
    ///
    /// Crate-internal: callers get the same information, better
    /// presented, from [`schema`](Self::schema).
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub(crate) fn layout(&self) -> &DataLayout {
        &self.layout
    }

    /// The underlying reader, positioned at the first data row.
    #[allow(dead_code)] // exercised once row decoding lands.
    #[inline]
    pub(crate) fn state_mut(&mut self) -> &mut ReaderState<R> {
        &mut self.state
    }
}
