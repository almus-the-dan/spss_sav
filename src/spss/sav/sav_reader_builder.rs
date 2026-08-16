//! Entry point for reading a SAV file.
//!
//! [`SavReaderBuilder`](crate::spss::sav::sav_reader_builder::SavReaderBuilder)
//! configures how a SAV file is read. Set options with chained methods,
//! then open the file with
//! [`from_path`](crate::spss::sav::sav_reader_builder::SavReaderBuilder::from_path),
//! [`from_file`](crate::spss::sav::sav_reader_builder::SavReaderBuilder::from_file)
//! or
//! [`from_reader`](crate::spss::sav::sav_reader_builder::SavReaderBuilder::from_reader),
//! which hands back a
//! [`SavReader`](crate::spss::sav::sav_reader::SavReader).
//!
//! From there the chain forks, and which fork is taken is how a caller
//! says whether the dictionary's content is wanted:
//! [`SavReader::into_record_reader`](crate::spss::sav::sav_reader::SavReader::into_record_reader)
//! goes straight to the rows and retains none of it, while
//! [`SavReader::into_dictionary_reader`](crate::spss::sav::sav_reader::SavReader::into_dictionary_reader)
//! hands out every dictionary record first.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::reader_options::ReaderOptions;
use crate::spss::sav::sav_error::{Result, SavError, Section};
use crate::spss::sav::sav_reader::SavReader;

/// Configures and opens a SAV file for reading.
///
/// Chain configuration setters, then call a `from_*` method to open the
/// file. From the [`SavReader`] that returns, two paths lead onward:
/// reading the values only, or walking the dictionary first.
///
/// # Examples
///
/// Values only — the common case, and the cheaper one, since no
/// dictionary record is retained:
///
/// ```no_run
/// use spss_sav::spss::sav::sav_reader_builder::SavReaderBuilder;
///
/// let mut reader = SavReaderBuilder::new()
///     .from_path("data.sav")?
///     .into_record_reader()?;
/// # Ok::<(), spss_sav::spss::sav::sav_error::SavError>(())
/// ```
///
/// Or the dictionary first, when its content is wanted — value labels,
/// documents, attributes:
///
/// ```no_run
/// use spss_sav::spss::sav::sav_reader_builder::SavReaderBuilder;
///
/// let mut dictionary = SavReaderBuilder::new()
///     .from_path("data.sav")?
///     .into_dictionary_reader()?;
///
/// while let Some(record) = dictionary.read_record()? {
///     // ...
/// }
/// let mut reader = dictionary.into_record_reader()?;
/// # Ok::<(), spss_sav::spss::sav::sav_error::SavError>(())
/// ```
#[derive(Debug, Clone, Default)]
pub struct SavReaderBuilder {
    options: ReaderOptions,
}

impl SavReaderBuilder {
    /// Creates a new builder: honor the file's own encoding declaration,
    /// and nothing else set.
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets how the text encoding is chosen — whether the encoding the
    /// file declares is honored
    /// ([`EncodingStrategy::Declared`], the default) or a
    /// caller-supplied encoding wins regardless
    /// ([`EncodingStrategy::Override`]).
    #[must_use]
    #[inline]
    pub fn encoding_strategy(mut self, strategy: EncodingStrategy) -> Self {
        self.options.set_encoding_strategy(strategy);
        self
    }

    /// Opens the file at `path` as a SAV file, wrapping it in a
    /// [`BufReader`] automatically.
    ///
    /// # Errors
    ///
    /// Returns [`SavError::Io`] if the file cannot be opened.
    //noinspection RsSelfConvention
    #[inline]
    pub fn from_path(self, path: impl AsRef<Path>) -> Result<SavReader<BufReader<File>>> {
        let file = File::open(path).map_err(|e| SavError::io(Section::Header, e))?;
        Ok(self.from_file(file))
    }

    /// Opens an already-open [`File`] as a SAV file, wrapping it in a
    /// [`BufReader`] automatically.
    //noinspection RsSelfConvention
    #[must_use]
    #[inline]
    pub fn from_file(self, file: File) -> SavReader<BufReader<File>> {
        self.from_reader(BufReader::new(file))
    }

    /// Opens a SAV file from any reader, returning the [`SavReader`]
    /// that reading proceeds from.
    //noinspection RsSelfConvention
    #[must_use]
    #[inline]
    pub fn from_reader<R>(self, reader: R) -> SavReader<R> {
        SavReader::new(reader, self.options)
    }
}
