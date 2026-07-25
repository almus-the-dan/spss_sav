//! Subtype 22 — long string missing values (collection wrapper).

use crate::spss::sav::extensions::long_missing_value_record::LongMissingValueRecord;

/// The long string missing values from one extension subtype-22
/// record.
///
/// A newtype over the parsed [`LongMissingValueRecord`]s (one per
/// variable), in on-disk order, so the extension record's payload
/// shape can gain fields without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongMissingValues {
    records: Vec<LongMissingValueRecord>,
}

impl LongMissingValues {
    /// Returns a fresh [`LongMissingValuesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongMissingValuesBuilder {
        LongMissingValuesBuilder::default()
    }

    /// The per-variable long missing value records, in on-disk order.
    #[must_use]
    #[inline]
    pub fn records(&self) -> &[LongMissingValueRecord] {
        &self.records
    }
}

/// Builder for [`LongMissingValues`].
#[derive(Debug, Default, Clone)]
pub struct LongMissingValuesBuilder {
    records: Vec<LongMissingValueRecord>,
}

impl LongMissingValuesBuilder {
    /// Appends one variable's long missing value record.
    #[must_use]
    #[inline]
    pub fn record(mut self, value: LongMissingValueRecord) -> Self {
        self.records.push(value);
        self
    }

    /// Replaces the collection with `records`.
    #[must_use]
    #[inline]
    pub fn records(mut self, records: Vec<LongMissingValueRecord>) -> Self {
        self.records = records;
        self
    }

    /// Finalizes this builder into a [`LongMissingValues`].
    ///
    /// Unset records default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongMissingValues {
        LongMissingValues {
            records: self.records,
        }
    }
}
