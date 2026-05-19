//! Wire-level payload of an extension subtype-11 record.

/// Raw display-parameter values carried verbatim from an extension
/// subtype-11 record.
///
/// Subtype 11 stores either 2 or 3 unsigned 32-bit values per
/// variable: `(measure, alignment)` in the 2-tuple form, or
/// `(measure, display_width, alignment)` in the 3-tuple form. The
/// reader doesn't decide which form is in play at streaming time —
/// it preserves the values verbatim and defers per-variable slicing
/// to schema finalization, which knows the dictionary's variable
/// count and can drive the 2 vs. 3-tuple split.
///
/// The typed, per-variable [`VariableDisplay`](crate::spss::sav::extensions::variable_display::VariableDisplay)
/// is what finalization produces and attaches to each `SavVariable`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawDisplayParameters {
    values: Vec<u32>,
}

impl RawDisplayParameters {
    /// Returns a fresh [`RawDisplayParametersBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> RawDisplayParametersBuilder {
        RawDisplayParametersBuilder::default()
    }

    /// Raw `u32` values in the order they appeared in the record's
    /// payload. Total length is `2 * variable_count` (no display
    /// width) or `3 * variable_count` (with display width).
    #[must_use]
    #[inline]
    pub fn values(&self) -> &[u32] {
        &self.values
    }
}

/// Builder for [`RawDisplayParameters`].
#[derive(Debug, Default, Clone)]
pub struct RawDisplayParametersBuilder {
    values: Vec<u32>,
}

impl RawDisplayParametersBuilder {
    /// Replaces the value list wholesale.
    #[must_use]
    #[inline]
    pub fn values(mut self, values: Vec<u32>) -> Self {
        self.values = values;
        self
    }

    /// Appends one value to the list.
    #[must_use]
    #[inline]
    pub fn value(mut self, value: u32) -> Self {
        self.values.push(value);
        self
    }

    /// Finalizes this builder into a [`RawDisplayParameters`].
    ///
    /// An empty list is permitted — it round-trips a subtype-11
    /// record with `element_count == 0`.
    #[must_use]
    #[inline]
    pub fn build(self) -> RawDisplayParameters {
        RawDisplayParameters {
            values: self.values,
        }
    }
}
