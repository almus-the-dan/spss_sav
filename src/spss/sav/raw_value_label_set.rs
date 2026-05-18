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
/// [`variable_indices`](Self::variable_indices) those entries apply
/// to (from the type-4 body).
///
/// The variable indices are normalized to 0-based logical positions
/// at parse time — continuation records have already been removed
/// from the count, and dangling indices have already errored as
/// [`FormatErrorKind::DanglingValueLabel`](crate::spss::sav::sav_error::FormatErrorKind::DanglingValueLabel).
///
/// The fully reconciled, typed form is
/// [`ValueLabelSet`](crate::spss::sav::value_label_set::ValueLabelSet);
/// it is built during the dictionary reader's finalization pass when
/// the referenced variables' types are known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawValueLabelSet {
    entries: Vec<RawValueLabelEntry>,
    variable_indices: Vec<u32>,
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

    /// 0-based logical indices of the variables this set applies to,
    /// from the type-4 record body. Always non-empty in well-formed
    /// files (the SAV format requires at least one variable per
    /// type-4 record); the reader carries an empty vec through only
    /// if the user constructed one via the builder for testing.
    #[must_use]
    #[inline]
    pub fn variable_indices(&self) -> &[u32] {
        &self.variable_indices
    }
}

/// Builder for [`RawValueLabelSet`].
#[derive(Debug, Default, Clone)]
pub struct RawValueLabelSetBuilder {
    entries: Vec<RawValueLabelEntry>,
    variable_indices: Vec<u32>,
}

impl RawValueLabelSetBuilder {
    /// Replaces the value-label entries with `entries`.
    #[must_use]
    #[inline]
    pub fn entries(mut self, entries: Vec<RawValueLabelEntry>) -> Self {
        self.entries = entries;
        self
    }

    /// Appends one value-label entry.
    #[must_use]
    #[inline]
    pub fn entry(mut self, entry: RawValueLabelEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Replaces the variable indices with `indices`. The indices are
    /// expected to be 0-based logical positions (continuation
    /// records excluded); the on-disk 1-based physical form is a
    /// reader/writer concern.
    #[must_use]
    #[inline]
    pub fn variable_indices(mut self, indices: Vec<u32>) -> Self {
        self.variable_indices = indices;
        self
    }

    /// Appends one variable index. See
    /// [`variable_indices`](Self::variable_indices) for the
    /// 0-based-logical convention.
    #[must_use]
    #[inline]
    pub fn variable_index(mut self, index: u32) -> Self {
        self.variable_indices.push(index);
        self
    }

    /// Finalizes this builder into a [`RawValueLabelSet`].
    #[must_use]
    #[inline]
    pub fn build(self) -> RawValueLabelSet {
        RawValueLabelSet {
            entries: self.entries,
            variable_indices: self.variable_indices,
        }
    }
}
