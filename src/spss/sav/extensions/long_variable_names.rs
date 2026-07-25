//! Subtype 13 — long variable name mappings (collection wrapper).

use crate::spss::sav::extensions::long_variable_name::LongVariableName;

/// The short-to-long variable-name mappings from one extension
/// subtype-13 record.
///
/// A newtype over the parsed [`LongVariableName`]s, in on-disk order,
/// so the extension record's payload shape can gain fields without
/// changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongVariableNames {
    mappings: Vec<LongVariableName>,
}

impl LongVariableNames {
    /// Returns a fresh [`LongVariableNamesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongVariableNamesBuilder {
        LongVariableNamesBuilder::default()
    }

    /// The name mappings, in on-disk order.
    #[must_use]
    #[inline]
    pub fn mappings(&self) -> &[LongVariableName] {
        &self.mappings
    }
}

/// Builder for [`LongVariableNames`].
#[derive(Debug, Default, Clone)]
pub struct LongVariableNamesBuilder {
    mappings: Vec<LongVariableName>,
}

impl LongVariableNamesBuilder {
    /// Appends one name mapping.
    #[must_use]
    #[inline]
    pub fn mapping(mut self, value: LongVariableName) -> Self {
        self.mappings.push(value);
        self
    }

    /// Replaces the collection with `mappings`.
    #[must_use]
    #[inline]
    pub fn mappings(mut self, mappings: Vec<LongVariableName>) -> Self {
        self.mappings = mappings;
        self
    }

    /// Finalizes this builder into a [`LongVariableNames`].
    ///
    /// Unset mappings default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongVariableNames {
        LongVariableNames {
            mappings: self.mappings,
        }
    }
}
