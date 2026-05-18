//! In-memory lookup table for value-label sets.

use std::cell::OnceCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::spss::sav::value_label_set::ValueLabelSet;
use crate::spss::sav::value_label_value::ValueLabelValue;

/// Minimum entries in a set before [`ValueLabelTable::label_for`]
/// builds and caches a hash-indexed lookup. Below this, a linear
/// scan beats the cost of allocating the index.
const INDEX_THRESHOLD: usize = 10;

/// Collection of [`ValueLabelSet`]s keyed by name.
///
/// Populated by draining a value-label reader or built up directly
/// by the caller. Sets with at least 10 entries lazily build a
/// cached hash index on the first hit; smaller sets stay on a linear
/// scan.
///
/// Numeric value-label keys are compared by IEEE 754 bit pattern via
/// [`ValueLabelValue`]'s `PartialEq`/`Hash`, matching how the SAV
/// format compares values against
/// [`MissingValueSpecification::Discrete`](crate::spss::sav::missing_value_specification::MissingValueSpecification::Discrete).
/// A cell value carrying a particular bit pattern resolves the same
/// way in both the missing-value check and the value-label lookup.
#[derive(Debug, Clone, Default)]
pub struct ValueLabelTable {
    sets: HashMap<Rc<str>, ValueLabelSet>,
    // Lazily built per set name. Kept in lockstep with `sets`: every
    // mutation that touches `sets` does the matching touch on
    // `indexes` so `label_for` can rely on the parallel structure.
    indexes: HashMap<Rc<str>, OnceCell<HashMap<ValueLabelValue, String>>>,
}

impl ValueLabelTable {
    /// Creates an empty table.
    #[must_use]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `set`, replacing any existing set with the same name
    /// and returning the displaced set. The cached index for that
    /// name is reset.
    pub fn insert(&mut self, set: ValueLabelSet) -> Option<ValueLabelSet> {
        let name: Rc<str> = Rc::from(set.name());
        self.indexes.insert(Rc::clone(&name), OnceCell::new());
        self.sets.insert(name, set)
    }

    /// Returns the set with the given name, if any.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&ValueLabelSet> {
        self.sets.get(name)
    }

    /// Removes and returns the set with the given name, if any.
    pub fn remove(&mut self, name: &str) -> Option<ValueLabelSet> {
        self.indexes.remove(name);
        self.sets.remove(name)
    }

    /// Number of sets in the table.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.sets.len()
    }

    /// `true` when the table holds no sets.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.sets.is_empty()
    }

    /// Yields the stored sets.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = &ValueLabelSet> {
        self.sets.values()
    }

    /// Returns the label for `value` in the set named `set_name`, or
    /// `None` when the set is missing or holds no matching entry.
    ///
    /// Sets with at least 10 entries lazily build a cached hash
    /// index on the first call; later lookups on the same set are
    /// O(1). Smaller sets stay on a linear scan. On duplicate keys,
    /// the first entry wins regardless of which path is taken.
    #[must_use]
    pub fn label_for(&self, set_name: &str, value: &ValueLabelValue) -> Option<&str> {
        let set = self.sets.get(set_name)?;
        if set.entries().len() < INDEX_THRESHOLD {
            return set.label_for(value);
        }
        // `indexes` is kept in lockstep with `sets`, so the get-cell
        // path is the expected one. The linear-scan fallback is
        // defensive against an invariant regression.
        let Some(cell) = self.indexes.get(set_name) else {
            return set.label_for(value);
        };
        let index = cell.get_or_init(|| build_index(set));
        index.get(value).map(String::as_str)
    }
}

/// Builds a first-wins `ValueLabelValue → label` index from a set's
/// entries.
fn build_index(set: &ValueLabelSet) -> HashMap<ValueLabelValue, String> {
    let mut index = HashMap::with_capacity(set.entries().len());
    for entry in set.entries() {
        index
            .entry(*entry.value())
            .or_insert_with(|| entry.label().to_owned());
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spss::sav::value_label_entry::ValueLabelEntry;

    fn entry(value: ValueLabelValue, label: &str) -> ValueLabelEntry {
        ValueLabelEntry::new(value, label.to_owned())
    }

    fn numeric_set(name: &str, count: u32) -> ValueLabelSet {
        let entries = (0..count)
            .map(|i| entry(ValueLabelValue::Numeric(f64::from(i)), &format!("lbl-{i}")))
            .collect();
        ValueLabelSet::new(name.to_owned(), entries)
    }

    // ---------------------- basic table operations ----------------------

    #[test]
    fn new_is_empty() {
        let table = ValueLabelTable::new();
        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }

    #[test]
    fn default_matches_new() {
        assert!(ValueLabelTable::default().is_empty());
    }

    #[test]
    fn insert_stores_set() {
        let mut table = ValueLabelTable::new();
        assert!(table.insert(numeric_set("a", 1)).is_none());
        assert_eq!(table.len(), 1);
        assert!(table.get("a").is_some());
    }

    #[test]
    fn insert_replaces_returns_previous() {
        let mut table = ValueLabelTable::new();
        table.insert(numeric_set("a", 1));
        let previous = table.insert(numeric_set("a", 2)).expect("previous");
        assert_eq!(previous.entries().len(), 1);
        assert_eq!(table.len(), 1);
        assert_eq!(table.get("a").unwrap().entries().len(), 2);
    }

    #[test]
    fn remove_returns_existing_and_drops_it() {
        let mut table = ValueLabelTable::new();
        table.insert(numeric_set("a", 1));
        let removed = table.remove("a").expect("removed");
        assert_eq!(removed.name(), "a");
        assert!(table.is_empty());
        assert!(table.get("a").is_none());
    }

    #[test]
    fn remove_missing_returns_none() {
        let mut table = ValueLabelTable::new();
        assert!(table.remove("ghost").is_none());
    }

    #[test]
    fn iter_yields_all_sets() {
        let mut table = ValueLabelTable::new();
        table.insert(numeric_set("a", 1));
        table.insert(numeric_set("b", 1));
        table.insert(numeric_set("c", 1));
        let mut names: Vec<&str> = table.iter().map(ValueLabelSet::name).collect();
        names.sort_unstable();
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    // ---------------------- label_for behavior --------------------------

    #[test]
    fn label_for_small_set_linear_scan() {
        let mut table = ValueLabelTable::new();
        table.insert(numeric_set("a", 3)); // < INDEX_THRESHOLD
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(0.0)),
            Some("lbl-0"),
        );
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(2.0)),
            Some("lbl-2"),
        );
        assert_eq!(table.label_for("a", &ValueLabelValue::Numeric(99.0)), None);
    }

    #[test]
    fn label_for_large_set_uses_cache() {
        let mut table = ValueLabelTable::new();
        table.insert(numeric_set("a", 25)); // ≥ INDEX_THRESHOLD
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(0.0)),
            Some("lbl-0"),
        );
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(15.0)),
            Some("lbl-15"),
        );
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(24.0)),
            Some("lbl-24"),
        );
        assert_eq!(table.label_for("a", &ValueLabelValue::Numeric(99.0)), None);
    }

    #[test]
    fn label_for_threshold_boundary_uses_cache() {
        let mut table = ValueLabelTable::new();
        let threshold = u32::try_from(INDEX_THRESHOLD).unwrap();
        table.insert(numeric_set("a", threshold)); // exactly INDEX_THRESHOLD
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(5.0)),
            Some("lbl-5"),
        );
    }

    #[test]
    fn label_for_missing_set_returns_none() {
        let table = ValueLabelTable::new();
        assert_eq!(
            table.label_for("nope", &ValueLabelValue::Numeric(0.0)),
            None,
        );
    }

    #[test]
    fn label_for_replace_invalidates_cache() {
        let mut table = ValueLabelTable::new();
        table.insert(numeric_set("a", 12)); // ≥ INDEX_THRESHOLD
        // Prime the cache.
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(3.0)),
            Some("lbl-3"),
        );

        // Replace with a different 12-entry set; the old cache must
        // not leak through.
        let entries: Vec<_> = (0..12)
            .map(|i| entry(ValueLabelValue::Numeric(f64::from(i)), &format!("new-{i}")))
            .collect();
        table.insert(ValueLabelSet::new("a".to_owned(), entries));

        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(3.0)),
            Some("new-3"),
        );
    }

    #[test]
    fn label_for_first_wins_on_duplicate_keys_small() {
        let entries = vec![
            entry(ValueLabelValue::Numeric(5.0), "first"),
            entry(ValueLabelValue::Numeric(5.0), "second"),
        ];
        let mut table = ValueLabelTable::new();
        table.insert(ValueLabelSet::new("a".to_owned(), entries));
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(5.0)),
            Some("first"),
        );
    }

    #[test]
    fn label_for_first_wins_on_duplicate_keys_cached() {
        // 10 entries trips the cached path. Last entry duplicates an
        // earlier value — first-wins should still hold.
        let mut entries: Vec<ValueLabelEntry> = (0..9)
            .map(|i| entry(ValueLabelValue::Numeric(f64::from(i)), &format!("lbl-{i}")))
            .collect();
        entries.push(entry(ValueLabelValue::Numeric(3.0), "DUP"));

        let mut table = ValueLabelTable::new();
        table.insert(ValueLabelSet::new("a".to_owned(), entries));
        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(3.0)),
            Some("lbl-3"),
        );
    }

    #[test]
    fn label_for_string_key_cached() {
        // String-keyed set, ≥ INDEX_THRESHOLD entries to exercise
        // the cache path's String hashing.
        let entries: Vec<_> = (0..12_u8)
            .map(|i| {
                let mut bytes = [b' '; 8];
                bytes[0] = b'A' + i;
                entry(ValueLabelValue::String(bytes), &format!("str-{i}"))
            })
            .collect();
        let mut table = ValueLabelTable::new();
        table.insert(ValueLabelSet::new("a".to_owned(), entries));

        let mut probe = [b' '; 8];
        probe[0] = b'C';
        assert_eq!(
            table.label_for("a", &ValueLabelValue::String(probe)),
            Some("str-2"),
        );
    }

    #[test]
    fn label_for_bit_pattern_equality_in_cached_path() {
        // 10+ entries → cached. Looking up -0.0 must NOT match an
        // entry stored under +0.0, since the bit patterns differ.
        let mut entries: Vec<ValueLabelEntry> = (1..10)
            .map(|i| entry(ValueLabelValue::Numeric(f64::from(i)), &format!("lbl-{i}")))
            .collect();
        entries.push(entry(ValueLabelValue::Numeric(0.0), "pos-zero"));

        let mut table = ValueLabelTable::new();
        table.insert(ValueLabelSet::new("a".to_owned(), entries));

        assert_eq!(
            table.label_for("a", &ValueLabelValue::Numeric(0.0)),
            Some("pos-zero"),
        );
        assert_eq!(table.label_for("a", &ValueLabelValue::Numeric(-0.0)), None);
    }
}
