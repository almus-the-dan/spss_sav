//! User-facing custom attribute on a SAV variable.

/// One custom attribute attached to a SAV variable.
///
/// Attributes are user-defined free-form metadata carried in extension
/// subtype 18. SPSS itself does not interpret them; they exist so other
/// software can round-trip its own annotations. A name is an SPSS
/// identifier, and so is matched case-insensitively; a leading `$` is
/// reserved for the writing application's own use and `@` or `$@` marks
/// one most SPSS commands will not display, which is why every variable
/// tends to carry a `$@Role`.
///
/// File-level attributes (subtype 17) have no reconciled form. They
/// reach a caller only as
/// [`FileAttribute`](crate::spss::sav::extensions::file_attribute::FileAttribute)
/// on the streamed record, along with the rest of the file-level
/// metadata.
///
/// An attribute holds a list of values rather than one, because SPSS
/// writes array-valued attributes as a run of indexed names —
/// `fred[1]`, `fred[2]` — which reconciliation collapses into a single
/// `fred` with two values, in index order. A plain scalar attribute is
/// simply a list of one.
///
/// Distinct from
/// [`VariableAttributeEntry`](crate::spss::sav::extensions::variable_attribute_entry::VariableAttributeEntry),
/// which is the wire-level form: it keeps the `[n]` suffix verbatim and
/// leaves the collapse to finalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableAttribute {
    name: String,
    values: Vec<String>,
}

impl VariableAttribute {
    /// Returns a fresh [`VariableAttributeBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableAttributeBuilder {
        VariableAttributeBuilder::default()
    }

    /// The attribute's name, with any `[n]` array suffix removed.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The attribute's values, in index order. A scalar attribute has
    /// exactly one.
    #[must_use]
    #[inline]
    pub fn values(&self) -> &[String] {
        &self.values
    }

    /// The sole value of a scalar attribute, or the first of an array
    /// one. `None` only when the attribute carried no values at all.
    #[must_use]
    #[inline]
    pub fn value(&self) -> Option<&str> {
        self.values.first().map(String::as_str)
    }
}

/// Builder for [`VariableAttribute`].
#[derive(Debug, Default, Clone)]
pub struct VariableAttributeBuilder {
    name: Option<String>,
    values: Vec<String>,
}

impl VariableAttributeBuilder {
    /// Sets the attribute name.
    #[must_use]
    #[inline]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Appends one value.
    #[must_use]
    #[inline]
    pub fn add_value(mut self, value: impl Into<String>) -> Self {
        self.values.push(value.into());
        self
    }

    /// Appends `values`.
    #[must_use]
    #[inline]
    pub fn add_values(mut self, values: Vec<String>) -> Self {
        self.values.extend(values);
        self
    }

    /// Finalizes this builder into a [`VariableAttribute`].
    ///
    /// An unset name becomes empty and an unset value list stays empty;
    /// neither is valid in a written file, but validation is a write-
    /// time concern.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableAttribute {
        VariableAttribute {
            name: self.name.unwrap_or_default(),
            values: self.values,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_attribute_exposes_its_single_value() {
        let attribute = VariableAttribute::builder()
            .name("MyAttr")
            .add_value("hello world")
            .build();
        assert_eq!(attribute.name(), "MyAttr");
        assert_eq!(attribute.values(), ["hello world"]);
        assert_eq!(attribute.value(), Some("hello world"));
    }

    #[test]
    fn array_attribute_keeps_index_order() {
        let attribute = VariableAttribute::builder()
            .name("fred")
            .add_values(vec!["first".to_owned(), "second".to_owned()])
            .build();
        assert_eq!(attribute.values(), ["first", "second"]);
        assert_eq!(attribute.value(), Some("first"));
    }

    /// The plural setter appends rather than replacing, so calls
    /// accumulate and mix freely with the singular one. A builder can
    /// only ever gain values, never silently lose the ones already
    /// added.
    #[test]
    fn add_values_appends_rather_than_replacing() {
        let attribute = VariableAttribute::builder()
            .name("fred")
            .add_value("first")
            .add_values(vec!["second".to_owned(), "third".to_owned()])
            .add_values(vec!["fourth".to_owned()])
            .add_value("fifth")
            .build();
        assert_eq!(
            attribute.values(),
            ["first", "second", "third", "fourth", "fifth"],
        );
    }

    #[test]
    fn valueless_attribute_reports_none() {
        let attribute = VariableAttribute::builder().name("empty").build();
        assert_eq!(attribute.value(), None);
        assert!(attribute.values().is_empty());
    }
}
