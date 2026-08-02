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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accessors_return_stored_values() {
        let entry = ValueLabelEntry::new(ValueLabelValue::Numeric(1.5), "label-text".to_owned());
        assert_eq!(*entry.value(), ValueLabelValue::Numeric(1.5));
        assert_eq!(entry.label(), "label-text");
    }

    #[test]
    fn accessors_return_string_key() {
        let key = *b"Male    ";
        let entry = ValueLabelEntry::new(ValueLabelValue::String(key), "Male".to_owned());
        assert_eq!(*entry.value(), ValueLabelValue::String(key));
        assert_eq!(entry.label(), "Male");
    }
}
