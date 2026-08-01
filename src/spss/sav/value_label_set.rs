//! A set of value-label mappings attached to a variable.

use std::cell::OnceCell;
use std::collections::HashMap;

use crate::spss::sav::value_label_entry::ValueLabelEntry;
use crate::spss::sav::value_label_value::ValueLabelValue;

/// Minimum entries in a set before [`ValueLabelSet::label_for`] builds
/// and caches a hash-indexed lookup. Below this, a linear scan beats
/// the cost of allocating the index.
const INDEX_THRESHOLD: usize = 10;

/// A collection of [`ValueLabelEntry`] mappings attached to a variable.
///
/// The SAV format has no named value-label sets — a type-3 record
/// simply lists `(value, label)` pairs and the type-4 record that
/// follows names the variables they apply to. So a set is reached
/// through the variable it belongs to
/// ([`SavVariable::value_labels`](crate::spss::sav::sav_variable::SavVariable::value_labels)),
/// never by name.
///
/// One set is shared, not copied, between the variables a single
/// type-3/type-4 pair covered.
///
/// Sets with at least ten entries lazily build a cached hash index on
/// the first lookup; smaller sets stay on a linear scan. On duplicate
/// keys the first entry wins, on either path.
#[derive(Debug, Clone, Default)]
pub struct ValueLabelSet {
    entries: Vec<ValueLabelEntry>,
    /// Built on first use for sets large enough to be worth indexing.
    index: OnceCell<HashMap<ValueLabelValue, String>>,
}

impl ValueLabelSet {
    pub(crate) fn new(entries: Vec<ValueLabelEntry>) -> Self {
        Self {
            entries,
            index: OnceCell::new(),
        }
    }

    /// The entries in this set, in the order the file declared them.
    #[must_use]
    #[inline]
    pub fn entries(&self) -> &[ValueLabelEntry] {
        &self.entries
    }

    /// `true` when this set holds no entries.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of entries in this set.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns the label for `value`, or `None` if no entry matches.
    ///
    /// Numeric keys compare by IEEE 754 bit pattern via
    /// [`ValueLabelValue`]'s `PartialEq`, matching how the SAV format
    /// itself compares values against
    /// [`MissingValueSpecification::Discrete`](crate::spss::sav::missing_value_specification::MissingValueSpecification::Discrete).
    /// A cell carrying a particular bit pattern therefore resolves the
    /// same way in the missing-value check and the label lookup.
    #[must_use]
    pub fn label_for(&self, value: &ValueLabelValue) -> Option<&str> {
        if self.entries.len() < INDEX_THRESHOLD {
            return self.scan(value);
        }
        let index = self.index.get_or_init(|| self.build_index());
        index.get(value).map(String::as_str)
    }

    fn scan(&self, value: &ValueLabelValue) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.value() == value)
            .map(ValueLabelEntry::label)
    }

    /// Builds a first-wins `ValueLabelValue → label` index.
    fn build_index(&self) -> HashMap<ValueLabelValue, String> {
        let mut index = HashMap::with_capacity(self.entries.len());
        for entry in &self.entries {
            index
                .entry(entry.value().clone())
                .or_insert_with(|| entry.label().to_owned());
        }
        index
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(value: ValueLabelValue, label: &str) -> ValueLabelEntry {
        ValueLabelEntry::new(value, label.to_owned())
    }

    fn numeric_set(count: u32) -> ValueLabelSet {
        let entries = (0..count)
            .map(|i| entry(ValueLabelValue::Numeric(f64::from(i)), &format!("lbl-{i}")))
            .collect();
        ValueLabelSet::new(entries)
    }

    #[test]
    fn entries_round_trip() {
        let set = ValueLabelSet::new(vec![entry(ValueLabelValue::Numeric(1.0), "one")]);
        assert_eq!(set.len(), 1);
        assert!(!set.is_empty());
        assert_eq!(set.entries()[0].label(), "one");
    }

    #[test]
    fn default_is_empty() {
        let set = ValueLabelSet::default();
        assert!(set.is_empty());
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(0.0)), None);
    }

    #[test]
    fn label_for_numeric_match_and_miss() {
        let set = numeric_set(3); // < INDEX_THRESHOLD
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(0.0)), Some("lbl-0"));
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(2.0)), Some("lbl-2"));
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(99.0)), None);
    }

    #[test]
    fn large_sets_use_the_cached_index() {
        let set = numeric_set(25); // >= INDEX_THRESHOLD
        assert_eq!(
            set.label_for(&ValueLabelValue::Numeric(15.0)),
            Some("lbl-15")
        );
        // Second lookup goes through the now-primed cache.
        assert_eq!(
            set.label_for(&ValueLabelValue::Numeric(24.0)),
            Some("lbl-24")
        );
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(99.0)), None);
    }

    #[test]
    fn threshold_boundary_uses_the_index() {
        let set = numeric_set(u32::try_from(INDEX_THRESHOLD).unwrap());
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(5.0)), Some("lbl-5"));
    }

    #[test]
    fn label_for_string_match() {
        let key = *b"Male    ";
        let set = ValueLabelSet::new(vec![entry(ValueLabelValue::String(key), "Male")]);
        assert_eq!(set.label_for(&ValueLabelValue::String(key)), Some("Male"));
    }

    #[test]
    fn label_for_long_string_match() {
        let key: Box<[u8]> = b"alpha                 ".to_vec().into_boxed_slice();
        let set = ValueLabelSet::new(vec![entry(
            ValueLabelValue::LongString(key.clone()),
            "First value",
        )]);
        assert_eq!(
            set.label_for(&ValueLabelValue::LongString(key)),
            Some("First value"),
        );
    }

    /// A short-string key and a long-string key never match, even byte
    /// for byte — they describe different variables.
    #[test]
    fn short_and_long_string_keys_are_distinct() {
        let set = ValueLabelSet::new(vec![entry(ValueLabelValue::String(*b"alpha   "), "short")]);
        let probe = ValueLabelValue::LongString(b"alpha   ".to_vec().into_boxed_slice());
        assert_eq!(set.label_for(&probe), None);
    }

    #[test]
    fn numeric_lookup_never_matches_a_string_entry() {
        let set = ValueLabelSet::new(vec![entry(ValueLabelValue::String([0; 8]), "S")]);
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(0.0)), None);
    }

    #[test]
    fn bit_pattern_equality_on_the_scan_path() {
        let set = ValueLabelSet::new(vec![entry(ValueLabelValue::Numeric(0.0), "pos-zero")]);
        assert_eq!(
            set.label_for(&ValueLabelValue::Numeric(0.0)),
            Some("pos-zero"),
        );
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(-0.0)), None);
    }

    #[test]
    fn bit_pattern_equality_on_the_cached_path() {
        let mut entries: Vec<ValueLabelEntry> = (1..10)
            .map(|i| entry(ValueLabelValue::Numeric(f64::from(i)), &format!("lbl-{i}")))
            .collect();
        entries.push(entry(ValueLabelValue::Numeric(0.0), "pos-zero"));
        let set = ValueLabelSet::new(entries);
        assert_eq!(
            set.label_for(&ValueLabelValue::Numeric(0.0)),
            Some("pos-zero"),
        );
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(-0.0)), None);
    }

    #[test]
    fn first_wins_on_duplicate_keys_scanning() {
        let set = ValueLabelSet::new(vec![
            entry(ValueLabelValue::Numeric(5.0), "first"),
            entry(ValueLabelValue::Numeric(5.0), "second"),
        ]);
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(5.0)), Some("first"));
    }

    #[test]
    fn first_wins_on_duplicate_keys_indexed() {
        let mut entries: Vec<ValueLabelEntry> = (0..9)
            .map(|i| entry(ValueLabelValue::Numeric(f64::from(i)), &format!("lbl-{i}")))
            .collect();
        entries.push(entry(ValueLabelValue::Numeric(3.0), "DUP"));
        let set = ValueLabelSet::new(entries);
        assert_eq!(set.label_for(&ValueLabelValue::Numeric(3.0)), Some("lbl-3"));
    }
}
