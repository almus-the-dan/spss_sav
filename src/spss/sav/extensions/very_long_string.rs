//! Subtype 14 — very-long-string widths.

/// One declaration of a string variable's logical width when that
/// width exceeds 255 bytes.
///
/// On disk, very long strings are split into multiple 255-byte
/// segments at the schema level; subtype 14 records the original
/// logical width so the reader can reconstruct the user-facing
/// variable. Schema finalization later pairs each declaration with
/// its host variable and re-fuses the segments.
///
/// The short name is already decoded through the file's active
/// encoding. The streaming layer doesn't validate the width against
/// the schema (e.g., that it actually exceeds 255 or matches the
/// declared segments) — it just records what's on disk so
/// finalization can reconcile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VeryLongString {
    short_name: String,
    width: u32,
}

impl VeryLongString {
    /// Returns a fresh [`VeryLongStringBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VeryLongStringBuilder {
        VeryLongStringBuilder::default()
    }

    /// Short (8-byte) variable name, as it appears in the type-2
    /// variable record's short-name field.
    #[must_use]
    #[inline]
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// The variable's logical width in bytes.
    #[must_use]
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }
}

/// Builder for [`VeryLongString`].
#[derive(Debug, Default, Clone)]
pub struct VeryLongStringBuilder {
    short_name: Option<String>,
    width: Option<u32>,
}

impl VeryLongStringBuilder {
    /// Sets the short (8-byte) name.
    #[must_use]
    #[inline]
    pub fn short_name(mut self, value: impl Into<String>) -> Self {
        self.short_name = Some(value.into());
        self
    }

    /// Sets the logical width in bytes.
    #[must_use]
    #[inline]
    pub fn width(mut self, value: u32) -> Self {
        self.width = Some(value);
        self
    }

    /// Finalizes this builder into a [`VeryLongString`].
    ///
    /// An unset short name defaults to an empty string; an unset
    /// width defaults to zero.
    #[must_use]
    #[inline]
    pub fn build(self) -> VeryLongString {
        let short_name = self.short_name.unwrap_or_default();
        let width = self.width.unwrap_or_default();
        VeryLongString { short_name, width }
    }
}
