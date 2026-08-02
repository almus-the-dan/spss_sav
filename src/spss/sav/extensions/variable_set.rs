//! One named variable grouping inside a variable sets record
//! (subtype 5).

/// A single named variable set from extension record subtype 5.
///
/// SPSS uses variable sets to organize variables into thematic groups
/// in the dataset editor. Each set has a name and an ordered list of
/// member variables, referenced by their long names. A set may be
/// empty (no members).
///
/// Both `name` and the member names are decoded through the file's
/// active encoding; the member names are not validated against the
/// schema at streaming time (finalization, which knows the variables,
/// can reconcile them).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableSet {
    name: String,
    variables: Vec<String>,
}

impl VariableSet {
    /// Returns a fresh [`VariableSetBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableSetBuilder {
        VariableSetBuilder::default()
    }

    /// The variable set's name.
    #[must_use]
    #[inline]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The long names of the set's member variables, in on-disk order.
    #[must_use]
    #[inline]
    pub fn variables(&self) -> &[String] {
        &self.variables
    }
}

/// Builder for [`VariableSet`].
#[derive(Debug, Default, Clone)]
pub struct VariableSetBuilder {
    name: Option<String>,
    variables: Vec<String>,
}

impl VariableSetBuilder {
    /// Sets the variable set's name.
    #[must_use]
    #[inline]
    pub fn name(mut self, value: impl Into<String>) -> Self {
        self.name = Some(value.into());
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
    pub fn add_variables(mut self, variables: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let variables = variables.into_iter().map(Into::into);
        self.variables.extend(variables);
        self
    }

    /// Finalizes this builder into a [`VariableSet`].
    ///
    /// An unset name defaults to the empty string; unset members
    /// default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableSet {
        VariableSet {
            name: self.name.unwrap_or_default(),
            variables: self.variables,
        }
    }
}
