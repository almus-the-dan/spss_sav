//! Subtype 17 — file-level custom attributes.

/// One file-level custom attribute from extension record subtype 17.
///
/// File attributes are arbitrary `name(values)` pairs attached to the
/// dataset as a whole rather than to a specific variable. A single
/// attribute may carry more than one value: on disk a `name` is
/// followed, inside parentheses, by one or more single-quoted,
/// line-feed-terminated strings.
///
/// This is the wire-level form yielded during dictionary streaming.
/// The `name` is preserved verbatim, including any `[n]` array-index
/// suffix SPSS writes for multivalued attributes (e.g. `fred[1]`);
/// collapsing indexed names into a single logical array is deferred
/// to schema finalization. The values have had their single outer
/// quote pair stripped but are otherwise verbatim.
///
/// Both `name` and `values` are already decoded through the file's
/// active encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAttribute {
    name: String,
    values: Vec<String>,
}

impl FileAttribute {
    /// Returns a fresh [`FileAttributeBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> FileAttributeBuilder {
        FileAttributeBuilder::default()
    }

    /// Attribute name, verbatim from disk (may include a trailing
    /// `[n]` array-index suffix).
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The attribute's values, in on-disk order, each with its outer
    /// single-quote pair already stripped.
    #[must_use]
    #[inline]
    pub fn values(&self) -> &[String] {
        &self.values
    }
}

/// Builder for [`FileAttribute`].
#[derive(Debug, Default, Clone)]
pub struct FileAttributeBuilder {
    name: Option<String>,
    values: Vec<String>,
}

impl FileAttributeBuilder {
    /// Sets the attribute name.
    #[must_use]
    #[inline]
    pub fn name(mut self, value: impl Into<String>) -> Self {
        self.name = Some(value.into());
        self
    }

    /// Appends one value to the attribute.
    #[must_use]
    #[inline]
    pub fn add_value(mut self, value: impl Into<String>) -> Self {
        self.values.push(value.into());
        self
    }

    /// Appends `values` to the attribute.
    #[must_use]
    #[inline]
    pub fn add_values(mut self, values: Vec<String>) -> Self {
        self.values.extend(values);
        self
    }

    /// Finalizes this builder into a [`FileAttribute`].
    ///
    /// An unset name defaults to the empty string; unset values
    /// default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> FileAttribute {
        FileAttribute {
            name: self.name.unwrap_or_default(),
            values: self.values,
        }
    }
}
