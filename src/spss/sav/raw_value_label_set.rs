//! Wire-level value-label set from a paired type-3 + type-4 record.

use crate::spss::sav::raw_value_label_entry::RawValueLabelEntry;

/// A paired type-3 + type-4 value-label set carried verbatim from
/// the dictionary section.
///
/// The dictionary reader yields a `RawValueLabelSet` for every
/// type-3 / type-4 pair it encounters (via
/// [`DictionaryRecord::ValueLabelSet`](crate::spss::sav::dictionary_record::DictionaryRecord::ValueLabelSet)),
/// holding both halves of the pair in one record: the value-label
/// [`entries`](Self::entries) (from the type-3 body) and the
/// [`segment_indices`](Self::segment_indices) those entries apply
/// to (from the type-4 body).
///
/// The indices are normalized at parse time — continuation records
/// have already been removed from the count, and dangling indices have
/// already errored as
/// [`FormatErrorKind::DanglingValueLabel`](crate::spss::sav::sav_error::FormatErrorKind::DanglingValueLabel).
///
/// The fully reconciled form attaches the entries directly to each
/// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable) during
/// the dictionary reader's finalization pass, once the referenced
/// variables' types are known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawValueLabelSet {
    entries: Vec<RawValueLabelEntry>,
    segment_indices: Vec<u32>,
}

impl RawValueLabelSet {
    /// Returns a fresh [`RawValueLabelSetBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> RawValueLabelSetBuilder {
        RawValueLabelSetBuilder::default()
    }

    /// Value-label entries from the type-3 record body, in their
    /// on-disk order.
    #[must_use]
    #[inline]
    pub fn entries(&self) -> &[RawValueLabelEntry] {
        &self.entries
    }

    /// 0-based *segment* indices of the variables this set applies to,
    /// from the type-4 record body.
    ///
    /// A segment index counts type-2 primary records — the same
    /// sequence [`DictionaryRecord::Variable`](crate::spss::sav::dictionary_record::DictionaryRecord::Variable)
    /// yields, continuation records excluded. For a file with no very
    /// long strings that is also the variable index, but the two part
    /// company as soon as one appears: a very long string occupies
    /// several segments and
    /// [`SavVariable::index`](crate::spss::sav::sav_variable::SavVariable::index)
    /// counts the collapsed variables. Finalization maps between them.
    ///
    /// Always non-empty in well-formed files (the SAV format requires
    /// at least one variable per type-4 record); the reader carries an
    /// empty vec through only if the user constructed one via the
    /// builder for testing.
    #[must_use]
    #[inline]
    pub fn segment_indices(&self) -> &[u32] {
        &self.segment_indices
    }
}

/// Builder for [`RawValueLabelSet`].
#[derive(Debug, Default, Clone)]
pub struct RawValueLabelSetBuilder {
    entries: Vec<RawValueLabelEntry>,
    segment_indices: Vec<u32>,
}

impl RawValueLabelSetBuilder {
    /// Appends `entries`.
    #[must_use]
    #[inline]
    pub fn add_entries(mut self, entries: Vec<RawValueLabelEntry>) -> Self {
        self.entries.extend(entries);
        self
    }

    /// Appends one value-label entry.
    #[must_use]
    #[inline]
    pub fn add_entry(mut self, entry: RawValueLabelEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Appends `indices`, which are expected to be 0-based segment
    /// positions (continuation records excluded); the on-disk 1-based
    /// physical form is a reader/writer concern. See
    /// [`RawValueLabelSet::segment_indices`].
    #[must_use]
    #[inline]
    pub fn add_segment_indices(mut self, indices: Vec<u32>) -> Self {
        self.segment_indices.extend(indices);
        self
    }

    /// Appends one segment index. See
    /// [`RawValueLabelSet::segment_indices`] for the convention.
    #[must_use]
    #[inline]
    pub fn add_segment_index(mut self, index: u32) -> Self {
        self.segment_indices.push(index);
        self
    }

    /// Finalizes this builder into a [`RawValueLabelSet`].
    #[must_use]
    #[inline]
    pub fn build(self) -> RawValueLabelSet {
        RawValueLabelSet {
            entries: self.entries,
            segment_indices: self.segment_indices,
        }
    }
}
