//! Reader for the data-record section of a SAV file.
//!
//! Third and final phase of the reader typestate chain, reached from
//! [`DictionaryReader::into_record_reader`](crate::spss::sav::dictionary_reader::DictionaryReader::into_record_reader).
//!
//! Row decoding itself lands with Phase 6, alongside the bytecode and
//! ZLIB decoders. What exists now is the boundary: the reader holds
//! everything a row read needs, and the dictionary phase is finished.

use std::io::Read;

use crate::spss::sav::compression::row_source::RowSource;
use crate::spss::sav::data_layout::DataLayout;
use crate::spss::sav::encoding_provenance::EncodingProvenance;
use crate::spss::sav::lazy_sav_record::LazySavRecord;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_record::SavRecord;
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
    /// Fills [`row`](Self::row) according to the file's compression.
    source: RowSource,
    /// The most recently read row, at full uncompressed width. Reused
    /// across reads, which is what lets a cell borrow rather than
    /// allocate — and what invalidates the previous record.
    row: Vec<u8>,
    /// Rows handed out so far, for the cross-check against the
    /// declared case count.
    ///
    /// Deliberately not exposed yet. Reporting it only becomes useful
    /// alongside the rest of a progress/position story, and settling
    /// that is not what Phase 6 is for.
    rows_read: u64,
}

impl<R: Read> RecordReader<R> {
    /// Reads the next data row, decoding every cell.
    ///
    /// Returns `None` at the end of the data section. The returned
    /// record borrows the reader's row buffer, so it must be dropped
    /// before the next read.
    ///
    /// # Errors
    ///
    /// Returns a [`SavError`](crate::spss::sav::sav_error::SavError) if
    /// the underlying reader fails, or if the data section ends partway
    /// through a row.
    pub fn read_record(&mut self) -> Result<Option<SavRecord<'_>>> {
        todo!("body lands with Phase 6(a)")
    }

    /// Reads the next data row without decoding any of its cells.
    ///
    /// The cheaper choice when a row has many columns and only a few
    /// are wanted; see
    /// [`LazySavRecord`](crate::spss::sav::lazy_sav_record::LazySavRecord).
    ///
    /// # Errors
    ///
    /// As [`read_record`](Self::read_record).
    pub fn read_lazy_record(&mut self) -> Result<Option<LazySavRecord<'_>>> {
        todo!("body lands with Phase 6(a)")
    }

    /// Advances past the next data row without decoding it.
    ///
    /// Returns `false` at the end of the data section. The row's bytes
    /// still have to be read — the reader chain has no
    /// [`Seek`](std::io::Seek) bound, and under either compressed
    /// scheme a row's length is not known until it has been decoded —
    /// but no cell is split out and nothing is allocated.
    ///
    /// # Errors
    ///
    /// As [`read_record`](Self::read_record).
    pub fn skip_record(&mut self) -> Result<bool> {
        let _ = (&self.source, &self.row, self.rows_read);
        todo!("body lands with Phase 6(a)")
    }
}

impl<R> RecordReader<R> {
    pub(crate) fn new(
        state: ReaderState<R>,
        header: SavHeader,
        encoding_provenance: EncodingProvenance,
        layout: DataLayout,
        schema: Option<SavSchema>,
    ) -> Self {
        let source = RowSource::new(layout.compression());
        let row = Vec::with_capacity(layout.row_len());
        Self {
            state,
            header,
            encoding_provenance,
            layout,
            schema,
            source,
            row,
            rows_read: 0,
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
