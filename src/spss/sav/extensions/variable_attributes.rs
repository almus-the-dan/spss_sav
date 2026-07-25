//! Subtype 18 — per-variable custom attributes (collection wrapper).

use crate::spss::sav::extensions::variable_attribute_record::VariableAttributeRecord;

/// The per-variable custom attributes from one extension subtype-18
/// record.
///
/// A newtype over the parsed [`VariableAttributeRecord`]s (one per
/// variable), in on-disk order, so the extension record's payload
/// shape can gain fields without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableAttributes {
    records: Vec<VariableAttributeRecord>,
}

impl VariableAttributes {
    /// Returns a fresh [`VariableAttributesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableAttributesBuilder {
        VariableAttributesBuilder::default()
    }

    /// The per-variable attribute records, in on-disk order.
    #[must_use]
    #[inline]
    pub fn records(&self) -> &[VariableAttributeRecord] {
        &self.records
    }
}

/// Builder for [`VariableAttributes`].
#[derive(Debug, Default, Clone)]
pub struct VariableAttributesBuilder {
    records: Vec<VariableAttributeRecord>,
}

impl VariableAttributesBuilder {
    /// Appends one variable's attribute record.
    #[must_use]
    #[inline]
    pub fn record(mut self, value: VariableAttributeRecord) -> Self {
        self.records.push(value);
        self
    }

    /// Replaces the collection with `records`.
    #[must_use]
    #[inline]
    pub fn records(mut self, records: Vec<VariableAttributeRecord>) -> Self {
        self.records = records;
        self
    }

    /// Finalizes this builder into a [`VariableAttributes`].
    ///
    /// Unset records default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableAttributes {
        VariableAttributes {
            records: self.records,
        }
    }
}
