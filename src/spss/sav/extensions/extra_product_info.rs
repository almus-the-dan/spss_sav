//! Subtype 10 — extra product information.

/// Extra product information from extension record subtype 10.
///
/// A free-form text string identifying the product that wrote the
/// file, beyond the fixed 60-byte product name in the header. The
/// payload's byte length is exact (no padding), so the text is kept
/// verbatim — trailing whitespace is preserved — and decoded through
/// the file's active encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExtraProductInfo {
    text: String,
}

impl ExtraProductInfo {
    /// Returns a fresh [`ExtraProductInfoBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> ExtraProductInfoBuilder {
        ExtraProductInfoBuilder::default()
    }

    /// The product information text, verbatim.
    #[must_use]
    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Builder for [`ExtraProductInfo`].
#[derive(Debug, Default, Clone)]
pub struct ExtraProductInfoBuilder {
    text: Option<String>,
}

impl ExtraProductInfoBuilder {
    /// Sets the product information text.
    #[must_use]
    #[inline]
    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = Some(value.into());
        self
    }

    /// Finalizes this builder into an [`ExtraProductInfo`].
    ///
    /// An unset text defaults to the empty string.
    #[must_use]
    #[inline]
    pub fn build(self) -> ExtraProductInfo {
        ExtraProductInfo {
            text: self.text.unwrap_or_default(),
        }
    }
}
