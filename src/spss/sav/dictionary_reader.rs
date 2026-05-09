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
}

impl<R> DictionaryReader<R> {
    #[allow(dead_code)] // exercised once the header reader lands.
    pub(crate) fn new(state: ReaderState<R>, header: SavHeader) -> Self {
        Self { state, header }
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
    #[allow(dead_code)] // exercised once the dictionary reader body lands.
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

/// Placeholder record-reader type. Lands in full in Phase 6.
///
/// Defined here as a unit shell so
/// [`DictionaryReader::into_record_reader`] can name its return
/// type today. The real definition (with `header()`, `schema()`,
/// `read_record()`, etc.) lands alongside the data-section reader.
#[derive(Debug)]
#[allow(dead_code)] // exercised once the record reader lands.
pub struct RecordReader<R> {
    _placeholder: core::marker::PhantomData<R>,
}
