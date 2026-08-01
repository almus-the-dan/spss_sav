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

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use crate::spss::sav::encoding_strategy::EncodingStrategy;
use crate::spss::sav::header_reader::HeaderReader;
use crate::spss::sav::reader_options::ReaderOptions;
use crate::spss::sav::sav_error::{Result, SavError, Section};
use crate::spss::sav::skippable_content::SkippableContent;

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

    /// Asks the reader not to retain one category of dictionary
    /// content. Call once per category; nothing is skipped by default.
    ///
    /// See [`SkippableContent`] for what skipping does and does not
    /// change — in short, it drops retention and decoding but not the
    /// read, and it can never make a well-formed file fail to parse or
    /// a data read come out wrong.
    ///
    /// There is deliberately no "skip everything" shorthand: the set of
    /// content a caller does not want is worth stating outright.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// use spss_sav::spss::sav::extensions::extension_subtype::ExtensionSubtype;
    /// use spss_sav::spss::sav::sav_reader::SavReader;
    /// use spss_sav::spss::sav::skippable_content::SkippableContent;
    ///
    /// let header_reader = SavReader::new()
    ///     .skip_dictionary_content(SkippableContent::Documents)
    ///     .skip_dictionary_content(SkippableContent::Extension(
    ///         ExtensionSubtype::LongValueLabels,
    ///     ))
    ///     .from_path("data.sav")
    ///     .unwrap();
    /// ```
    #[must_use]
    #[inline]
    pub fn skip_dictionary_content(mut self, content: SkippableContent) -> Self {
        self.options.skip(content);
        self
    }

    /// Sets whether a
    /// [`SavSchema`](crate::spss::sav::sav_schema::SavSchema) is
    /// assembled as dictionary records are handed out. `true` by
    /// default.
    ///
    /// Pass `false` when you are folding the streamed records into your
    /// own structure and would otherwise pay to build both.
    /// [`RecordReader::schema`](crate::spss::sav::record_reader::RecordReader::schema)
    /// then returns `None`. The data layout the record reader needs is
    /// accumulated separately and is unaffected, so this cannot change
    /// how the rows read.
    ///
    /// This is the mirror of
    /// [`skip_dictionary_content`](Self::skip_dictionary_content):
    /// skipping controls what is retained on the way in, this controls
    /// what is assembled on the way out.
    #[must_use]
    #[inline]
    pub fn build_schema(mut self, build: bool) -> Self {
        self.options.set_build_schema(build);
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
