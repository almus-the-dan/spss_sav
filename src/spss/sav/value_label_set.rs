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
    /// Linear scan; numeric keys compare by IEEE 754 bit pattern via
    /// [`ValueLabelValue`]'s `PartialEq`. On duplicate keys, the first
    /// entry wins. For fast repeated lookups across many sets, use
    /// [`ValueLabelTable`](crate::spss::sav::value_label_table::ValueLabelTable).
    #[must_use]
    pub fn label_for(&self, value: &ValueLabelValue) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.value() == value)
            .map(ValueLabelEntry::label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(value: ValueLabelValue, label: &str) -> ValueLabelEntry {
        ValueLabelEntry::new(value, label.to_owned())
    }

    fn set_with(name: &str, entries: Vec<ValueLabelEntry>) -> ValueLabelSet {
        ValueLabelSet::new(name.to_owned(), entries)
    }

    #[test]
    fn name_and_entries_round_trip() {
        let set = set_with("lbl", vec![entry(ValueLabelValue::Numeric(1.0), "one")]);
        assert_eq!(set.name(), "lbl");
        assert_eq!(set.entries().len(), 1);
    }

    #[test]
    fn label_for_numeric_match() {
        let set = set_with(
            "lbl",
            vec![
                entry(ValueLabelValue::Numeric(1.0), "one"),
                entry(ValueLabelValue::Numeric(2.0), "two"),
            ],
        );
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(1.0)), Some("one"));
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(2.0)), Some("two"));
    }

    #[test]
    fn label_for_numeric_miss() {
        let set = set_with("lbl", vec![entry(ValueLabelValue::Numeric(1.0), "one")]);
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(99.0)), None);
    }

    #[test]
    fn label_for_string_match() {
        let key = *b"Male    ";
        let set = set_with("lbl", vec![entry(ValueLabelValue::String(key), "Male")]);
        assert_eq!(set.label_for(&ValueLabelValue::String(key)), Some("Male"),);
    }

    #[test]
    fn label_for_distinguishes_numeric_and_string() {
        let set = set_with("lbl", vec![entry(ValueLabelValue::String([0; 8]), "S")]);
        // A numeric lookup must not match a string entry, even if
        // the bits would happen to align.
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(0.0)), None);
    }

    #[test]
    fn label_for_uses_bit_pattern_equality() {
        // 0.0 and -0.0 are == in IEEE but have different bit patterns.
        let set = set_with(
            "lbl",
            vec![entry(ValueLabelValue::Numeric(0.0), "pos-zero")],
        );
        assert_eq!(
            set.label_for(&ValueLabelValue::Numeric(0.0)),
            Some("pos-zero"),
        );
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(-0.0)), None);
    }

    #[test]
    fn label_for_first_wins_on_duplicate_keys() {
        let set = set_with(
            "lbl",
            vec![
                entry(ValueLabelValue::Numeric(5.0), "first"),
                entry(ValueLabelValue::Numeric(5.0), "second"),
            ],
        );
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(5.0)), Some("first"));
    }

    #[test]
    fn label_for_empty_set_returns_none() {
        let set = set_with("empty", vec![]);
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(0.0)), None);
    }
}
