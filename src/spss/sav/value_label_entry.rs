//! A single value→label mapping.

use crate::spss::sav::value_label_value::ValueLabelValue;

/// A single mapping from a [`ValueLabelValue`] key to a label
/// string.
#[derive(Debug, Clone)]
pub struct ValueLabelEntry {
    value: ValueLabelValue,
    label: String,
}

impl ValueLabelEntry {
    #[allow(dead_code)] // exercised once the value-label reader lands.
    pub(crate) fn new(value: ValueLabelValue, label: String) -> Self {
        Self { value, label }
    }

    /// The key of this entry.
    #[must_use]
    #[inline]
    pub fn value(&self) -> &ValueLabelValue {
        &self.value
    }

    /// The label, decoded using the file's encoding.
    #[must_use]
    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }
}
