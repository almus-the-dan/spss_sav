//! A named set of value-label mappings.

use crate::spss::sav::value_label_entry::ValueLabelEntry;
use crate::spss::sav::value_label_value::ValueLabelValue;

/// A named collection of [`ValueLabelEntry`] mappings.
///
/// Variables reference a value-label set by name. Resolution from a
/// `(variable, value)` pair to a label string is handled by
/// [`ValueLabelTable`](crate::spss::sav::value_label_table::ValueLabelTable).
#[derive(Debug, Clone)]
pub struct ValueLabelSet {
    name: String,
    entries: Vec<ValueLabelEntry>,
}

impl ValueLabelSet {
    #[allow(dead_code)] // exercised once the value-label reader lands.
    pub(crate) fn new(name: String, entries: Vec<ValueLabelEntry>) -> Self {
        Self { name, entries }
    }

    /// The name of this set.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The entries in this set.
    #[must_use]
    #[inline]
    pub fn entries(&self) -> &[ValueLabelEntry] {
        &self.entries
    }

    /// Returns the label for `value`, or `None` if no entry matches.
    ///
    /// Linear scan — for fast repeated lookups across many sets, use
    /// [`ValueLabelTable`](crate::spss::sav::value_label_table::ValueLabelTable).
    #[must_use]
    pub fn label_for(&self, value: &ValueLabelValue) -> Option<&str> {
        let _ = value;
        todo!()
    }
}
