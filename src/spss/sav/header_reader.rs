//! Reader for the 176-byte SAV file header.
//!
//! First phase of the SAV reader typestate chain. Created via
//! [`SavReader::from_path`](crate::spss::sav::sav_reader::SavReader::from_path)
//! (or the other `from_*` entry points). Call
//! [`read_header`](HeaderReader::read_header) to parse the header
//! and advance to the dictionary phase.

use std::io::Read;
use encoding_rs::Encoding;

use crate::spss::sav::dictionary_reader::DictionaryReader;
use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::sav_warning::SavWarning;

/// Entry point for reading a SAV file.
///
/// Created via
/// [`SavReader::from_path`](crate::spss::sav::sav_reader::SavReader::from_path)
/// (or [`from_file`](crate::spss::sav::sav_reader::SavReader::from_file)
/// /
/// [`from_reader`](crate::spss::sav::sav_reader::SavReader::from_reader)),
/// then call [`read_header`](Self::read_header) to parse the file
/// header and advance to the dictionary phase.
#[derive(Debug)]
pub struct HeaderReader<R> {
    state: ReaderState<R>,
    encoding_override: Option<&'static Encoding>,
    encoding_strategy: EncodingStrategy,
}

impl<R> HeaderReader<R> {
    /// Constructs a new header reader. The encoding override (if
    /// any) and encoding strategy are forwarded from the upstream
    /// [`SavReader`](crate::spss::sav::sav_reader::SavReader).
    ///
    /// The initial encoding stored on `ReaderState` is a placeholder
    /// — it is replaced once the dictionary phase resolves the
    /// declared encoding (or kept if the user supplied an override
    /// with [`EncodingStrategy::Override`]).
    #[allow(dead_code)] // exercised once SavReader lands.
    pub(crate) fn new(
        reader: R,
        encoding_override: Option<&'static Encoding>,
        encoding_strategy: EncodingStrategy,
    ) -> Self {
        let initial_encoding = encoding_override.unwrap_or(encoding_rs::WINDOWS_1252);
        let state = ReaderState::new(reader, initial_encoding);
        Self {
            state,
            encoding_override,
            encoding_strategy,
        }
    }

    /// The encoding override supplied via
    /// [`SavReader::encoding`](crate::spss::sav::sav_reader::SavReader::encoding),
    /// if any.
    #[must_use]
    #[inline]
    pub fn encoding_override(&self) -> Option<&'static Encoding> {
        self.encoding_override
    }

    /// The encoding strategy supplied via
    /// [`SavReader::encoding_strategy`](crate::spss::sav::sav_reader::SavReader::encoding_strategy).
    #[must_use]
    #[inline]
    pub fn encoding_strategy(&self) -> EncodingStrategy {
        self.encoding_strategy
    }

    /// Warnings accumulated so far. Empty before
    /// [`read_header`](Self::read_header) is called; populated only
    /// in the unlikely case that header construction surfaces a
    /// warning.
    #[allow(dead_code)] // exercised once the header reader body lands.
    #[must_use]
    #[inline]
    pub fn warnings(&self) -> &[SavWarning] {
        self.state.warnings()
    }
}

impl<R: Read> HeaderReader<R> {
    /// Parses the 176-byte file header and transitions to the
    /// dictionary phase.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Io`](crate::spss::sav::sav_error::SavError::Io)
    /// on read failures and
    /// [`SavError::Format`](crate::spss::sav::sav_error::SavError::Format)
    /// when the header bytes do not match a recognized SAV layout
    /// (bad magic, unreadable layout code, unknown float format,
    /// …).
    pub fn read_header(self) -> Result<DictionaryReader<R>> {
        todo!("body lands with the header reader phase")
    }
}
