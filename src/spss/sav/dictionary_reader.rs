//! Streaming reader for the SAV dictionary section.
//!
//! Sits between [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader)
//! and the (future) record reader. Yields one
//! [`DictionaryRecord`] at a time —
//! variable records, value-label sets, document records, and
//! extension records freely interleaved between the header and the
//! `999` end-of-dictionary marker.
//!
//! This is a Phase 4 shell; the per-record parsing logic and the
//! finalization that produces a [`SavSchema`] land in Phase 5.

use std::io::Read;

use crate::spss::sav::dictionary_record::DictionaryRecord;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::record_reader::RecordReader;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::sav_header::SavHeader;
use crate::spss::sav::sav_warning::SavWarning;

/// Streaming reader for the SAV dictionary section.
///
/// Created by
/// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header).
/// Pull individual records via [`read_record`](Self::read_record)
/// until it returns `Ok(None)` (the `999` marker), or skip
/// straight to record reading via
/// [`into_record_reader`](Self::into_record_reader) which
/// auto-consumes any remaining dictionary records.
#[derive(Debug)]
pub struct DictionaryReader<R> {
    state: ReaderState<R>,
    header: SavHeader,
    #[allow(dead_code)] // exercised once the dictionary reader phase lands.
    weight_variable_index: Option<usize>,
}

impl<R> DictionaryReader<R> {
    pub(crate) fn new(
        state: ReaderState<R>,
        header: SavHeader,
        weight_variable_index: Option<usize>,
    ) -> Self {
        Self {
            state,
            header,
            weight_variable_index,
        }
    }

    /// 0-based index of the declared weight variable, if any.
    /// Surfaced via [`SavHeader::weight_variable`] (the long name)
    /// only after the dictionary phase finalizes; before then,
    /// callers can inspect the raw index here.
    #[allow(dead_code)] // exercised once the dictionary reader phase lands.
    #[must_use]
    #[inline]
    pub(crate) fn weight_variable_index(&self) -> Option<usize> {
        self.weight_variable_index
    }

    /// The file header parsed by the upstream
    /// [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader).
    /// `weight_variable` and any other extension-derived fields
    /// stay empty until the dictionary phase finalizes them.
    #[must_use]
    #[inline]
    pub fn header(&self) -> &SavHeader {
        &self.header
    }

    /// Warnings accumulated by the most recent
    /// [`read_record`](Self::read_record) call (or by
    /// [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header)
    /// for the first call). Cleared at the start of each
    /// `read_record` invocation.
    #[must_use]
    #[inline]
    pub fn warnings(&self) -> &[SavWarning] {
        self.state.warnings()
    }
}

impl<R: Read> DictionaryReader<R> {
    /// Reads the next dictionary record. Returns `Ok(None)` once
    /// the `999` end-of-dictionary marker has been consumed.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Io`](crate::spss::sav::sav_error::SavError::Io)
    /// on read failures and
    /// [`SavError::Format`](crate::spss::sav::sav_error::SavError::Format)
    /// when the bytes do not match a recognized record shape.
    #[allow(unused_mut, unused_variables)] // body lands in Phase 5.
    pub fn read_record(&mut self) -> Result<Option<DictionaryRecord>> {
        todo!("body lands with the dictionary reader phase")
    }

    /// Auto-consumes any remaining dictionary records, finalizes
    /// the schema, and transitions to record reading.
    ///
    /// # Errors
    ///
    /// Returns whatever [`read_record`](Self::read_record) would
    /// return for any record consumed during finalization.
    pub fn into_record_reader(self) -> Result<RecordReader<R>> {
        todo!("body lands with the record reader phase")
    }
}
