//! Subtypes 7 / 19 — multiple response sets (MRSETS).

use crate::spss::sav::extensions::multiple_response_set_kind::MultipleResponseSetKind;

/// One multiple response set declaration from extension record
/// subtype 7 (pre-v14: category and dichotomy sets) or subtype 19
/// (v14+: adds `CATEGORYLABELS=COUNTEDVALUES` dichotomy sets).
///
/// MRSETS group variables that together represent answers to a
/// "select all that apply" survey question. The set is metadata only:
/// the answers live in the member variables. Each set carries a name
/// (which begins with `$`), a set label, its [`MultipleResponseSetKind`]
/// (category vs dichotomy, with the counted value and label source),
/// and the member variables' long names.
///
/// The name, label, and member names are decoded through the file's
/// active encoding; the leading `$` on the name is preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MultipleResponseSet {
    name: String,
    label: String,
    kind: MultipleResponseSetKind,
    variables: Vec<String>,
}

impl MultipleResponseSet {
    /// Returns a fresh [`MultipleResponseSetBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> MultipleResponseSetBuilder {
        MultipleResponseSetBuilder::default()
    }

    /// The set's name, including its leading `$`.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The set's label (may be empty).
    #[must_use]
    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// The set's kind (category or dichotomy) and kind-specific data.
    #[must_use]
    #[inline]
    pub fn kind(&self) -> &MultipleResponseSetKind {
        &self.kind
    }

    /// The long names of the member variables, in on-disk order.
    #[must_use]
    #[inline]
    pub fn variables(&self) -> &[String] {
        &self.variables
    }
}

/// Builder for [`MultipleResponseSet`].
#[derive(Debug, Default, Clone)]
pub struct MultipleResponseSetBuilder {
    name: Option<String>,
    label: Option<String>,
    kind: Option<MultipleResponseSetKind>,
    variables: Vec<String>,
}

impl MultipleResponseSetBuilder {
    /// Sets the set's name (should include the leading `$`).
    #[must_use]
    #[inline]
    pub fn name(mut self, value: impl Into<String>) -> Self {
        self.name = Some(value.into());
        self
    }

    /// Sets the set's label.
    #[must_use]
    #[inline]
    pub fn label(mut self, value: impl Into<String>) -> Self {
        self.label = Some(value.into());
        self
    }

    /// Sets the set's kind.
    #[must_use]
    #[inline]
    pub fn kind(mut self, value: MultipleResponseSetKind) -> Self {
        self.kind = Some(value);
        self
    }

    /// Appends one member variable's long name.
    #[must_use]
    #[inline]
    pub fn add_variable(mut self, value: impl Into<String>) -> Self {
        self.variables.push(value.into());
        self
    }

    /// Appends `variables` to the set's members.
    #[must_use]
    #[inline]
    pub fn add_variables(mut self, variables: Vec<String>) -> Self {
        self.variables.extend(variables);
        self
    }

    /// Finalizes this builder into a [`MultipleResponseSet`].
    ///
    /// An unset name and label default to the empty string; an unset
    /// kind defaults to
    /// [`MultipleResponseSetKind::MultipleCategory`]; unset variables
    /// default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> MultipleResponseSet {
        MultipleResponseSet {
            name: self.name.unwrap_or_default(),
            label: self.label.unwrap_or_default(),
            kind: self
                .kind
                .unwrap_or(MultipleResponseSetKind::MultipleCategory),
            variables: self.variables,
        }
    }
}
