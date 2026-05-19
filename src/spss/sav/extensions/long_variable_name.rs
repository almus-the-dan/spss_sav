//! Subtype 13 — long-variable-name mapping.

/// One mapping from a short (8-byte) variable name to its full
/// long name.
///
/// SPSS stores variable names in two places: the schema's variable
/// records (capped at 8 bytes) and this extension record (up to 64
/// bytes per the PSPP spec). Schema finalization later pairs each
/// mapping with its host variable, so the resulting `SavVariable`
/// carries both names.
///
/// Both fields are already decoded through the file's active
/// encoding. The streaming layer doesn't enforce PSPP's character-class
/// rules — it just records what's on disk so finalization or user code
/// can validate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongVariableName {
    short_name: String,
    long_name: String,
}

impl LongVariableName {
    /// Returns a fresh [`LongVariableNameBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongVariableNameBuilder {
        LongVariableNameBuilder::default()
    }

    /// Short (8-byte) variable name, as it appears in the type-2
    /// variable record's short-name field.
    #[must_use]
    #[inline]
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// Long variable name (up to 64 bytes per the PSPP spec).
    #[must_use]
    #[inline]
    pub fn long_name(&self) -> &str {
        &self.long_name
    }
}

/// Builder for [`LongVariableName`].
#[derive(Debug, Default, Clone)]
pub struct LongVariableNameBuilder {
    short_name: Option<String>,
    long_name: Option<String>,
}

impl LongVariableNameBuilder {
    /// Sets the short (8-byte) name.
    #[must_use]
    #[inline]
    pub fn short_name(mut self, value: impl Into<String>) -> Self {
        self.short_name = Some(value.into());
        self
    }

    /// Sets the long name.
    #[must_use]
    #[inline]
    pub fn long_name(mut self, value: impl Into<String>) -> Self {
        self.long_name = Some(value.into());
        self
    }

    /// Finalizes this builder into a [`LongVariableName`].
    ///
    /// Unset fields default to empty strings.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongVariableName {
        let short_name = self.short_name.unwrap_or_default();
        let long_name = self.long_name.unwrap_or_default();
        LongVariableName {
            short_name,
            long_name,
        }
    }
}
