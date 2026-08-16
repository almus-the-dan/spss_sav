//! Entry point for reading a SAV file.
//!
//! [`SavReader`](crate::spss::sav::sav_reader::SavReader) is a builder
//! for configuring how a SAV file is read. Set options with chained
//! methods, then call a terminal method
//! ([`from_path`](crate::spss::sav::sav_reader::SavReader::from_path),
//! [`from_file`](crate::spss::sav::sav_reader::SavReader::from_file), or
//! [`from_reader`](crate::spss::sav::sav_reader::SavReader::from_reader))
//! to obtain a
//! [`HeaderReader`](crate::spss::sav::header_reader::HeaderReader) — the
//! first phase of the reader typestate chain.
//!
//! From there the chain forks, and which fork is taken is how a caller
//! says whether the dictionary's content is wanted:
//! [`HeaderReader::into_record_reader`](crate::spss::sav::header_reader::HeaderReader::into_record_reader)
//! goes straight to the rows and retains none of it, while
//! [`HeaderReader::read_header`](crate::spss::sav::header_reader::HeaderReader::read_header)
//! hands out every dictionary record first.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::header_reader::HeaderReader;
use crate::spss::sav::reader_options::ReaderOptions;
use crate::spss::sav::sav_error::{Result, SavError, Section};

/// Builder for configuring and opening a SAV file reader.
///
/// Chain configuration setters, then call a terminal `from_*`
/// method to begin reading. From the
/// [`HeaderReader`] that returns, two paths lead onward: reading the
/// values only, or walking the dictionary first.
///
/// # Examples
///
/// Values only — the common case, and the cheaper one, since no
/// dictionary record is retained:
///
/// ```no_run
/// use spss_sav::spss::sav::sav_reader::SavReader;
///
/// let mut reader = SavReader::new()
///     .from_path("data.sav")?
///     .into_record_reader()?;
/// # Ok::<(), spss_sav::spss::sav::sav_error::SavError>(())
/// ```
///
/// Or the dictionary first, when its content is wanted — value labels,
/// documents, attributes:
///
/// ```no_run
/// use spss_sav::spss::sav::sav_reader::SavReader;
///
/// let mut dictionary = SavReader::new()
///     .from_path("data.sav")?
///     .read_header()?;
///
/// while let Some(record) = dictionary.read_record()? {
///     // ...
/// }
/// let mut reader = dictionary.into_record_reader()?;
/// # Ok::<(), spss_sav::spss::sav::sav_error::SavError>(())
/// ```
#[derive(Debug, Clone)]
pub struct SavReader {
    options: ReaderOptions,
}

impl SavReader {
    /// Creates a new builder with default values.
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self {
            options: ReaderOptions::default(),
        }
    }

    /// Sets how the text encoding is chosen — whether the encoding the
    /// file declares is honored
    /// ([`EncodingStrategy::Declared`], the default) or a caller-
    /// supplied encoding wins regardless
    /// ([`EncodingStrategy::Override`]).
    #[must_use]
    #[inline]
    pub fn encoding_strategy(mut self, strategy: EncodingStrategy) -> Self {
        self.options.set_encoding_strategy(strategy);
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
        HeaderReader::new(reader, self.options)
    }
}

impl Default for SavReader {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}
