//! Free-text document lines from a SAV file.

/// One type-6 document record from a SAV file.
///
/// Document records carry free-text annotation lines authored by
/// the user (e.g., via SPSS's `DOCUMENT` command). Each line is
/// fixed-width — exactly 80 bytes, space-padded — and the
/// dictionary reader decodes each one through the file's active
/// encoding into a `String`. Trailing padding is preserved verbatim
/// because the spec permits it to be significant, and stripping
/// would break round-tripping.
///
/// Multiple type-6 records may appear in a single file in the
/// wild; the streaming reader yields one
/// [`DictionaryRecord::Document`](crate::spss::sav::dictionary_record::DictionaryRecord::Document)
/// per occurrence. The dictionary finalizer (Phase 5(e)) decides
/// how to reconcile multiple records into the final schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DocumentRecord {
    lines: Vec<String>,
}

impl DocumentRecord {
    /// Returns a fresh [`DocumentRecordBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> DocumentRecordBuilder {
        DocumentRecordBuilder::default()
    }

    /// The document lines, in their on-disk order. Each entry is
    /// decoded from one 80-byte slice of the record; trailing
    /// padding is preserved.
    #[must_use]
    #[inline]
    pub fn lines(&self) -> &[String] {
        &self.lines
    }
}

/// Builder for [`DocumentRecord`].
#[derive(Debug, Default, Clone)]
pub struct DocumentRecordBuilder {
    lines: Vec<String>,
}

impl DocumentRecordBuilder {
    /// Replaces the document lines with `lines`.
    #[must_use]
    #[inline]
    pub fn lines(mut self, lines: Vec<String>) -> Self {
        self.lines = lines;
        self
    }

    /// Appends one document line.
    #[must_use]
    #[inline]
    pub fn line(mut self, line: impl Into<String>) -> Self {
        self.lines.push(line.into());
        self
    }

    /// Finalizes this builder into a [`DocumentRecord`].
    #[must_use]
    #[inline]
    pub fn build(self) -> DocumentRecord {
        DocumentRecord { lines: self.lines }
    }
}
