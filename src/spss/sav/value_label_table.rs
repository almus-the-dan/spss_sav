//! In-memory lookup table for value-label sets.

use core::hash::{Hash, Hasher};
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
/// Numeric value-label keys are compared by IEEE 754 bit pattern,
/// matching how the SAV format compares values against
/// [`MissingValueSpec::Discrete`](crate::spss::sav::missing_value_spec::MissingValueSpec::Discrete).
/// A cell value carrying a particular bit pattern resolves the same
/// way in both the missing-value check and the value-label lookup.
#[derive(Debug, Clone, Default)]
pub struct ValueLabelTable {
    sets: HashMap<Rc<str>, ValueLabelSet>,
    // Lazily built per set name. Kept in lockstep with `sets`: every
    // mutation that touches `sets` does the matching touch on
    // `indexes` so `label_for` can rely on the parallel structure.
    indexes: HashMap<Rc<str>, OnceCell<HashMap<ValueLabelKey, String>>>,
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
    pub fn iter(&self) -> impl Iterator<Item = &ValueLabelSet> {
        self.sets.values()
    }

    /// Returns the label for `value` in the set named `set_name`, or
    /// `None` when the set is missing or holds no matching entry.
    ///
    /// Sets with at least 10 entries lazily build a cached hash
    /// index on the first call; later lookups on the same set are
    /// O(1). Smaller sets stay on a linear scan.
    #[must_use]
    pub fn label_for(&self, set_name: &str, value: &ValueLabelValue) -> Option<&str> {
        let _ = (set_name, value, INDEX_THRESHOLD);
        todo!()
    }
}

// ---------------------------------------------------------------------------
// Internal: hashable wrapper around ValueLabelValue for cache keys.
// ---------------------------------------------------------------------------

/// Bit-pattern-equality wrapper around [`ValueLabelValue`] for use
/// as a `HashMap` key.
///
/// Module-private — keeps the bit-pattern hash semantics out of
/// [`ValueLabelValue`]'s public API, where users would reasonably
/// expect numeric `PartialEq` to follow IEEE 754 rules.
#[derive(Debug, Clone, Copy)]
struct ValueLabelKey(ValueLabelValue);

impl PartialEq for ValueLabelKey {
    fn eq(&self, other: &Self) -> bool {
        match (self.0, other.0) {
            (ValueLabelValue::Numeric(a), ValueLabelValue::Numeric(b)) => {
                a.to_bits() == b.to_bits()
            }
            (ValueLabelValue::String(a), ValueLabelValue::String(b)) => a == b,
            _ => false,
        }
    }
}

impl Eq for ValueLabelKey {}

impl Hash for ValueLabelKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        match &self.0 {
            ValueLabelValue::Numeric(value) => {
                0u8.hash(state);
                value.to_bits().hash(state);
            }
            ValueLabelValue::String(bytes) => {
                1u8.hash(state);
                bytes.hash(state);
            }
        }
    }
}

impl From<ValueLabelValue> for ValueLabelKey {
    fn from(value: ValueLabelValue) -> Self {
        Self(value)
    }
}
