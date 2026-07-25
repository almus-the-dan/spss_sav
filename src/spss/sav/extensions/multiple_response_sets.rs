//! Subtypes 7 / 19 — multiple response sets (collection wrapper).

use crate::spss::sav::extensions::multiple_response_set::MultipleResponseSet;

/// The multiple response sets from one extension subtype-7 or -19
/// record.
///
/// A newtype over the parsed [`MultipleResponseSet`]s, in on-disk
/// order, so the extension record's payload shape can gain fields
/// without changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipleResponseSets {
    sets: Vec<MultipleResponseSet>,
}

impl MultipleResponseSets {
    /// Returns a fresh [`MultipleResponseSetsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> MultipleResponseSetsBuilder {
        MultipleResponseSetsBuilder::default()
    }

    /// The multiple response sets, in on-disk order.
    #[must_use]
    #[inline]
    pub fn sets(&self) -> &[MultipleResponseSet] {
        &self.sets
    }
}

/// Builder for [`MultipleResponseSets`].
#[derive(Debug, Default, Clone)]
pub struct MultipleResponseSetsBuilder {
    sets: Vec<MultipleResponseSet>,
}

impl MultipleResponseSetsBuilder {
    /// Appends one multiple response set.
    #[must_use]
    #[inline]
    pub fn set(mut self, value: MultipleResponseSet) -> Self {
        self.sets.push(value);
        self
    }

    /// Replaces the collection with `sets`.
    #[must_use]
    #[inline]
    pub fn sets(mut self, sets: Vec<MultipleResponseSet>) -> Self {
        self.sets = sets;
        self
    }

    /// Finalizes this builder into a [`MultipleResponseSets`].
    ///
    /// Unset sets default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> MultipleResponseSets {
        MultipleResponseSets { sets: self.sets }
    }
}
