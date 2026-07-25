//! Subtype 5 — named variable groupings.

use crate::spss::sav::extensions::variable_set::VariableSet;

/// Named variable groupings declared by extension record subtype 5.
///
/// SPSS uses these to organize variables into thematic sets in the
/// dataset editor. The on-disk format is a single text payload with
/// one set per line; the reader exposes the parsed structure rather
/// than the raw text. Wraps the parsed [`VariableSet`]s in declaration
/// order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableSets {
    sets: Vec<VariableSet>,
}

impl VariableSets {
    /// Returns a fresh [`VariableSetsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableSetsBuilder {
        VariableSetsBuilder::default()
    }

    /// The parsed variable sets, in on-disk (declaration) order.
    #[must_use]
    #[inline]
    pub fn sets(&self) -> &[VariableSet] {
        &self.sets
    }
}

/// Builder for [`VariableSets`].
#[derive(Debug, Default, Clone)]
pub struct VariableSetsBuilder {
    sets: Vec<VariableSet>,
}

impl VariableSetsBuilder {
    /// Appends one variable set.
    #[must_use]
    #[inline]
    pub fn set(mut self, value: VariableSet) -> Self {
        self.sets.push(value);
        self
    }

    /// Replaces the collection with `sets`.
    #[must_use]
    #[inline]
    pub fn sets(mut self, sets: Vec<VariableSet>) -> Self {
        self.sets = sets;
        self
    }

    /// Finalizes this builder into a [`VariableSets`].
    ///
    /// Unset sets default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableSets {
        VariableSets { sets: self.sets }
    }
}
