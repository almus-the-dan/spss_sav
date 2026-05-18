//! A reconciled SAV variable.

use crate::spss::sav::extensions::variable_display::VariableDisplay;
use crate::spss::sav::missing_value_specification::MissingValueSpecification;
use crate::spss::sav::sav_format::SavFormat;
use crate::spss::sav::variable_attribute::VariableAttribute;
use crate::spss::sav::variable_type::VariableType;

/// A fully reconciled SAV variable.
///
/// `SavVariable` is the user-facing type returned from a finalized
/// [`SavSchema`](crate::spss::sav::sav_schema::SavSchema). It bundles
/// the wire-level fields read from a single type-2 record with the
/// information patched in from extension records — long names,
/// display parameters, custom attributes, and very long string
/// widths.
///
/// Distinct from
/// [`SavVariableHeader`](crate::spss::sav::sav_variable_header::SavVariableHeader),
/// which is the streaming-yielded wire-level type used during the
/// dictionary phase.
#[derive(Debug, Clone)]
pub struct SavVariable {
    short_name: String,
    long_name: Option<String>,
    variable_type: VariableType,
    print_format: SavFormat,
    write_format: SavFormat,
    label: Option<String>,
    missing_value_spec: MissingValueSpecification,
    value_label_set: Option<String>,
    display: Option<VariableDisplay>,
    attributes: Vec<VariableAttribute>,
    index: usize,
}

impl SavVariable {
    /// Returns a fresh [`SavVariableBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> SavVariableBuilder {
        SavVariableBuilder::default()
    }

    /// The 1–8 byte SAV short name.
    #[must_use]
    #[inline]
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// The long variable name, if one was declared via subtype 13.
    ///
    /// Empty until the dictionary phase has consumed subtype 13 and
    /// patched it onto this variable.
    #[must_use]
    #[inline]
    pub fn long_name(&self) -> Option<&str> {
        self.long_name.as_deref()
    }

    /// Convenience accessor returning [`long_name`](Self::long_name)
    /// when populated, otherwise [`short_name`](Self::short_name).
    #[must_use]
    pub fn full_name(&self) -> &str {
        self.long_name.as_deref().unwrap_or(&self.short_name)
    }

    /// Storage type (numeric or fixed-width string).
    #[must_use]
    #[inline]
    pub fn variable_type(&self) -> VariableType {
        self.variable_type
    }

    /// The print format used for default rendering.
    #[must_use]
    #[inline]
    pub fn print_format(&self) -> SavFormat {
        self.print_format
    }

    /// The write format used when serializing the value back to text.
    #[must_use]
    #[inline]
    pub fn write_format(&self) -> SavFormat {
        self.write_format
    }

    /// The user-facing variable label, if one was declared.
    #[must_use]
    #[inline]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// The missing-value specification.
    #[must_use]
    #[inline]
    pub fn missing_value_spec(&self) -> &MissingValueSpecification {
        &self.missing_value_spec
    }

    /// Name of the value-label set associated with this variable, if
    /// any.
    #[must_use]
    #[inline]
    pub fn value_label_set(&self) -> Option<&str> {
        self.value_label_set.as_deref()
    }

    /// Display parameters from extension subtype 11, if present.
    #[must_use]
    #[inline]
    pub fn display(&self) -> Option<&VariableDisplay> {
        self.display.as_ref()
    }

    /// Custom attributes from extension subtype 17.
    #[must_use]
    #[inline]
    pub fn attributes(&self) -> &[VariableAttribute] {
        &self.attributes
    }

    /// 0-based index of this variable in each data row.
    #[must_use]
    #[inline]
    pub fn index(&self) -> usize {
        self.index
    }
}

/// Builder for [`SavVariable`].
#[derive(Debug, Default, Clone)]
pub struct SavVariableBuilder {
    short_name: Option<String>,
    long_name: Option<String>,
    variable_type: Option<VariableType>,
    print_format: Option<SavFormat>,
    write_format: Option<SavFormat>,
    label: Option<String>,
    missing_value_spec: Option<MissingValueSpecification>,
    value_label_set: Option<String>,
    display: Option<VariableDisplay>,
    attributes: Vec<VariableAttribute>,
    index: usize,
}

impl SavVariableBuilder {
    /// Sets the SAV short name (1–8 bytes).
    #[must_use]
    #[inline]
    pub fn short_name(mut self, name: impl Into<String>) -> Self {
        self.short_name = Some(name.into());
        self
    }

    /// Sets the long variable name (typically populated by the
    /// dictionary reader after subtype 13 is processed).
    #[must_use]
    #[inline]
    pub fn long_name(mut self, name: impl Into<String>) -> Self {
        self.long_name = Some(name.into());
        self
    }

    /// Clears the long variable name.
    #[must_use]
    #[inline]
    pub fn clear_long_name(mut self) -> Self {
        self.long_name = None;
        self
    }

    /// Sets the storage type.
    #[must_use]
    #[inline]
    pub fn variable_type(mut self, variable_type: VariableType) -> Self {
        self.variable_type = Some(variable_type);
        self
    }

    /// Sets the print format.
    #[must_use]
    #[inline]
    pub fn print_format(mut self, format: SavFormat) -> Self {
        self.print_format = Some(format);
        self
    }

    /// Sets the write format.
    #[must_use]
    #[inline]
    pub fn write_format(mut self, format: SavFormat) -> Self {
        self.write_format = Some(format);
        self
    }

    /// Sets the user-facing variable label.
    #[must_use]
    #[inline]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Clears the user-facing variable label.
    #[must_use]
    #[inline]
    pub fn clear_label(mut self) -> Self {
        self.label = None;
        self
    }

    /// Sets the missing-value specification.
    #[must_use]
    #[inline]
    pub fn missing_value_spec(mut self, spec: MissingValueSpecification) -> Self {
        self.missing_value_spec = Some(spec);
        self
    }

    /// Sets the name of the associated value-label set.
    #[must_use]
    #[inline]
    pub fn value_label_set(mut self, name: impl Into<String>) -> Self {
        self.value_label_set = Some(name.into());
        self
    }

    /// Clears the name of the associated value-label set.
    #[must_use]
    #[inline]
    pub fn clear_value_label_set(mut self) -> Self {
        self.value_label_set = None;
        self
    }

    /// Sets the display parameters (typically attached by the
    /// dictionary reader after subtype 11 is processed).
    #[must_use]
    #[inline]
    pub fn display(mut self, display: VariableDisplay) -> Self {
        self.display = Some(display);
        self
    }

    /// Clears the display parameters.
    #[must_use]
    #[inline]
    pub fn clear_display(mut self) -> Self {
        self.display = None;
        self
    }

    /// Appends a custom attribute.
    #[must_use]
    #[inline]
    pub fn attribute(mut self, attribute: VariableAttribute) -> Self {
        self.attributes.push(attribute);
        self
    }

    /// Replaces the attribute list wholesale.
    #[must_use]
    #[inline]
    pub fn attributes(mut self, attributes: Vec<VariableAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Sets the 0-based variable index.
    ///
    /// Crate-internal — set by the dictionary reader / writer when
    /// the variable's position in the schema becomes known.
    #[allow(dead_code)] // exercised once the dictionary reader/writer lands.
    #[inline]
    pub(crate) fn index(mut self, index: usize) -> Self {
        self.index = index;
        self
    }

    /// Finalizes this builder into a [`SavVariable`].
    #[must_use]
    #[inline]
    pub fn build(self) -> SavVariable {
        todo!("body lands with the dictionary reader / writer")
    }
}
