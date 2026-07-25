//! Subtype 14 — very-long-string widths (collection wrapper).

use crate::spss::sav::extensions::very_long_string::VeryLongString;

/// The very-long-string width declarations from one extension
/// subtype-14 record.
///
/// A newtype over the parsed [`VeryLongString`]s, in on-disk order, so
/// the extension record's payload shape can gain fields without
/// changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeryLongStrings {
    strings: Vec<VeryLongString>,
}

impl VeryLongStrings {
    /// Returns a fresh [`VeryLongStringsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VeryLongStringsBuilder {
        VeryLongStringsBuilder::default()
    }

    /// The very-long-string declarations, in on-disk order.
    #[must_use]
    #[inline]
    pub fn strings(&self) -> &[VeryLongString] {
        &self.strings
    }
}

/// Builder for [`VeryLongStrings`].
#[derive(Debug, Default, Clone)]
pub struct VeryLongStringsBuilder {
    strings: Vec<VeryLongString>,
}

impl VeryLongStringsBuilder {
    /// Appends one very-long-string declaration.
    #[must_use]
    #[inline]
    pub fn string(mut self, value: VeryLongString) -> Self {
        self.strings.push(value);
        self
    }

    /// Replaces the collection with `strings`.
    #[must_use]
    #[inline]
    pub fn strings(mut self, strings: Vec<VeryLongString>) -> Self {
        self.strings = strings;
        self
    }

    /// Finalizes this builder into a [`VeryLongStrings`].
    ///
    /// Unset strings default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VeryLongStrings {
        VeryLongStrings {
            strings: self.strings,
        }
    }
}
