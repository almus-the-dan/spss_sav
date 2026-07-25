//! Subtype 12 — file UUID.

/// A file UUID from extension record subtype 12.
///
/// SPSS (observed from version 13) writes a UUID in the RFC 4122
/// format as text — the 36-character hyphenated hexadecimal form,
/// which may mix upper and lower case. The reader keeps the string
/// verbatim (preserving case and formatting) and decodes it through
/// the file's active encoding; it is not parsed or validated against
/// RFC 4122.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Uuid {
    text: String,
}

impl Uuid {
    /// Returns a fresh [`UuidBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> UuidBuilder {
        UuidBuilder::default()
    }

    /// The UUID text, verbatim.
    #[must_use]
    #[inline]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Builder for [`Uuid`].
#[derive(Debug, Default, Clone)]
pub struct UuidBuilder {
    text: Option<String>,
}

impl UuidBuilder {
    /// Sets the UUID text.
    #[must_use]
    #[inline]
    pub fn text(mut self, value: impl Into<String>) -> Self {
        self.text = Some(value.into());
        self
    }

    /// Finalizes this builder into a [`Uuid`].
    ///
    /// An unset text defaults to the empty string.
    #[must_use]
    #[inline]
    pub fn build(self) -> Uuid {
        Uuid {
            text: self.text.unwrap_or_default(),
        }
    }
}
