//! Entry point for reading a SAV file.
//!
//! [`SavReader`] is a builder for configuring how a SAV file is
//! read. Set options with chained methods, then call a terminal
//! method ([`from_path`](SavReader::from_path),
//! [`from_file`](SavReader::from_file), or
//! [`from_reader`](SavReader::from_reader)) to obtain a
//! [`HeaderReader`] — the first phase of the reader typestate
//! chain.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use encoding_rs::Encoding;

use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::header_reader::HeaderReader;
use crate::spss::sav::sav_error::{Result, SavError, Section};

/// Builder for configuring and opening a SAV file reader.
///
/// Chain configuration setters, then call a terminal `from_*`
/// method to begin reading.
///
/// # Examples
///
/// ```no_run
/// use spss_sav::spss::sav::sav_reader::SavReader;
///
/// let header_reader = SavReader::new()
///     .from_path("data.sav")
///     .unwrap();
/// ```
#[derive(Debug, Clone)]
pub struct SavReader {
    encoding: Option<&'static Encoding>,
    encoding_strategy: EncodingStrategy,
}

impl SavReader {
    /// Creates a new builder with default values.
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self {
            encoding: None,
            encoding_strategy: EncodingStrategy::default(),
        }
    }

    /// Sets an explicit encoding. Combined with
    /// [`encoding_strategy`](Self::encoding_strategy), this controls
    /// whether the file's declared encoding wins
    /// ([`EncodingStrategy::Fallback`], default — the supplied
    /// encoding is used only when the file declares no encoding) or
    /// the supplied encoding wins ([`EncodingStrategy::Override`]).
    #[must_use]
    #[inline]
    pub fn encoding(mut self, encoding: &'static Encoding) -> Self {
        self.encoding = Some(encoding);
        self
    }

    /// Clears any previously-set encoding.
    #[must_use]
    #[inline]
    pub fn clear_encoding(mut self) -> Self {
        self.encoding = None;
        self
    }

    /// Sets the encoding strategy. Default is
    /// [`EncodingStrategy::Fallback`].
    #[must_use]
    #[inline]
    pub fn encoding_strategy(mut self, strategy: EncodingStrategy) -> Self {
        self.encoding_strategy = strategy;
        self
    }

    /// Opens the file at `path` and begins reading it as a SAV
    /// file, wrapping it in a [`BufReader`] automatically.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Io`] if the file cannot be opened.
    //noinspection RsSelfConvention
    #[inline]
    pub fn from_path(self, path: impl AsRef<Path>) -> Result<HeaderReader<BufReader<File>>> {
        let file = File::open(path).map_err(|e| SavError::io(Section::Header, e))?;
        Ok(self.from_file(file))
    }

    /// Begins reading a SAV file from a [`File`], wrapping it in a
    /// [`BufReader`] automatically.
    //noinspection RsSelfConvention
    #[must_use]
    #[inline]
    pub fn from_file(self, file: File) -> HeaderReader<BufReader<File>> {
        self.from_reader(BufReader::new(file))
    }

    /// Begins reading a SAV file from any reader, returning a
    /// [`HeaderReader`] for the first phase of parsing.
    //noinspection RsSelfConvention
    #[must_use]
    #[inline]
    pub fn from_reader<R>(self, reader: R) -> HeaderReader<R> {
        HeaderReader::new(reader, self.encoding, self.encoding_strategy)
    }
}

impl Default for SavReader {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
