//! Subtype 20 — declared character encoding (value wrapper).

/// The character encoding label declared by an extension subtype-20
/// record.
///
/// A newtype over the decoded encoding `name` (e.g. `"UTF-8"`,
/// `"windows-1252"`) so the extension record's payload shape can gain
/// fields without changing the enum variant. The name is the label as
/// written on disk; it is not resolved to a concrete encoding here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CharacterEncoding {
    name: String,
}

impl CharacterEncoding {
    /// Returns a fresh [`CharacterEncodingBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> CharacterEncodingBuilder {
        CharacterEncodingBuilder::default()
    }

    /// The declared encoding label, verbatim.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Builder for [`CharacterEncoding`].
#[derive(Debug, Default, Clone)]
pub struct CharacterEncodingBuilder {
    name: Option<String>,
}

impl CharacterEncodingBuilder {
    /// Sets the declared encoding label.
    #[must_use]
    #[inline]
    pub fn name(mut self, value: impl Into<String>) -> Self {
        self.name = Some(value.into());
        self
    }

    /// Finalizes this builder into a [`CharacterEncoding`].
    ///
    /// An unset name defaults to the empty string.
    #[must_use]
    #[inline]
    pub fn build(self) -> CharacterEncoding {
        CharacterEncoding {
            name: self.name.unwrap_or_default(),
        }
    }
}
