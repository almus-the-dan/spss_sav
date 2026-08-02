//! One attribute inside an extension subtype-18 record.

/// A single custom attribute belonging to one variable's attribute
/// set in extension record subtype 18.
///
/// Structurally this mirrors
/// [`FileAttribute`](crate::spss::sav::extensions::file_attribute::FileAttribute)
/// — a `name` followed by one or more values — but it is kept as a
/// distinct type because it lives inside a
/// [`VariableAttributeRecord`](crate::spss::sav::extensions::variable_attribute_record::VariableAttributeRecord)
/// rather than standing on its own.
///
/// This is the wire-level form yielded during dictionary streaming.
/// The `name` is preserved verbatim, including any `[n]` array-index
/// suffix SPSS writes for multivalued attributes; collapsing indexed
/// names into a single logical array is deferred to schema
/// finalization. The values have had their single outer quote pair
/// stripped but are otherwise verbatim. Both `name` and `values` are
/// already decoded through the file's active encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableAttributeEntry {
    name: String,
    values: Vec<String>,
}

impl VariableAttributeEntry {
    /// Returns a fresh [`VariableAttributeEntryBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableAttributeEntryBuilder {
        VariableAttributeEntryBuilder::default()
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

/// Builder for [`VariableAttributeEntry`].
#[derive(Debug, Default, Clone)]
pub struct VariableAttributeEntryBuilder {
    name: Option<String>,
    values: Vec<String>,
}

impl VariableAttributeEntryBuilder {
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

    /// Finalizes this builder into a [`VariableAttributeEntry`].
    ///
    /// An unset name defaults to the empty string; unset values
    /// default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableAttributeEntry {
        VariableAttributeEntry {
            name: self.name.unwrap_or_default(),
            values: self.values,
        }
    }
}
