//! Subtype 18 — per-variable custom attributes.

use crate::spss::sav::extensions::variable_attribute_entry::VariableAttributeEntry;

/// One subtype-18 entry carrying a single variable's custom
/// attributes.
///
/// On disk a variable attributes record is a sequence of
/// `variable_name:attribute-set` groups delimited by `/`. Each group
/// becomes one `VariableAttributeRecord`: the long variable name it
/// applies to, plus that variable's list of
/// [`VariableAttributeEntry`] attributes.
///
/// Distinct from
/// [`VariableAttribute`](crate::spss::sav::variable_attribute::VariableAttribute):
/// `VariableAttribute` is the user-facing `(name, value)` pair
/// attached to a `SavVariable`, whereas `VariableAttributeRecord` is
/// the wire-level extension-record entry that pairs a variable name
/// with its raw attributes. `variable_name` is decoded through the
/// file's active encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariableAttributeRecord {
    variable_name: String,
    attributes: Vec<VariableAttributeEntry>,
}

impl VariableAttributeRecord {
    /// Returns a fresh [`VariableAttributeRecordBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableAttributeRecordBuilder {
        VariableAttributeRecordBuilder::default()
    }

    /// The long variable name these attributes apply to, verbatim
    /// from disk.
    #[must_use]
    #[inline]
    pub fn variable_name(&self) -> &str {
        &self.variable_name
    }

    /// This variable's attributes, in on-disk order.
    #[must_use]
    #[inline]
    pub fn attributes(&self) -> &[VariableAttributeEntry] {
        &self.attributes
    }
}

/// Builder for [`VariableAttributeRecord`].
#[derive(Debug, Default, Clone)]
pub struct VariableAttributeRecordBuilder {
    variable_name: Option<String>,
    attributes: Vec<VariableAttributeEntry>,
}

impl VariableAttributeRecordBuilder {
    /// Sets the long variable name these attributes apply to.
    #[must_use]
    #[inline]
    pub fn variable_name(mut self, value: impl Into<String>) -> Self {
        self.variable_name = Some(value.into());
        self
    }

    /// Appends one attribute to this variable's set.
    #[must_use]
    #[inline]
    pub fn add_attribute(mut self, value: VariableAttributeEntry) -> Self {
        self.attributes.push(value);
        self
    }

    /// Appends `attributes` to this variable.
    #[must_use]
    #[inline]
    pub fn add_attributes(mut self, attributes: Vec<VariableAttributeEntry>) -> Self {
        self.attributes.extend(attributes);
        self
    }

    /// Finalizes this builder into a [`VariableAttributeRecord`].
    ///
    /// An unset variable name defaults to the empty string; unset
    /// attributes default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableAttributeRecord {
        VariableAttributeRecord {
            variable_name: self.variable_name.unwrap_or_default(),
            attributes: self.attributes,
        }
    }
}
