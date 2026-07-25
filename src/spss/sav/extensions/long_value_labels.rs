//! Subtype 21 — long string value labels (collection wrapper).

use crate::spss::sav::extensions::long_value_label_record::LongValueLabelRecord;

/// The long string value labels from one extension subtype-21 record.
///
/// A newtype over the parsed [`LongValueLabelRecord`]s (one per
/// variable), in on-disk order, so the extension record's payload
/// shape can gain fields without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongValueLabels {
    records: Vec<LongValueLabelRecord>,
}

impl LongValueLabels {
    /// Returns a fresh [`LongValueLabelsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongValueLabelsBuilder {
        LongValueLabelsBuilder::default()
    }

    /// The per-variable long value label records, in on-disk order.
    #[must_use]
    #[inline]
    pub fn records(&self) -> &[LongValueLabelRecord] {
        &self.records
    }
}

/// Builder for [`LongValueLabels`].
#[derive(Debug, Default, Clone)]
pub struct LongValueLabelsBuilder {
    records: Vec<LongValueLabelRecord>,
}

impl LongValueLabelsBuilder {
    /// Appends one variable's long value label record.
    #[must_use]
    #[inline]
    pub fn record(mut self, value: LongValueLabelRecord) -> Self {
        self.records.push(value);
        self
    }

    /// Replaces the collection with `records`.
    #[must_use]
    #[inline]
    pub fn records(mut self, records: Vec<LongValueLabelRecord>) -> Self {
        self.records = records;
        self
    }

    /// Finalizes this builder into a [`LongValueLabels`].
    ///
    /// Unset records default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongValueLabels {
        LongValueLabels {
            records: self.records,
        }
    }
}
