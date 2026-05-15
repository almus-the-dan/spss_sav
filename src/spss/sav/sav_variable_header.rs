//! Wire-level fields from a single type-2 variable record.

use crate::spss::sav::raw_missing_values::RawMissingValues;
use crate::spss::sav::sav_format::SavFormat;
use crate::spss::sav::variable_type::VariableType;

/// The wire-level fields of a single SAV type-2 variable record,
/// after any continuation records have been collapsed.
///
/// `SavVariableHeader` is yielded transiently by the dictionary
/// reader (via
/// [`DictionaryRecord::Variable`](crate::spss::sav::dictionary_record::DictionaryRecord::Variable))
/// and carries only the fields present on the variable record itself.
/// The fully reconciled, extension-record-patched form is
/// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable), which
/// is what users obtain from `RecordReader::schema()` once the
/// dictionary phase has finalized. Notably absent from this type:
/// long name (extension subtype 13), display parameters (subtype 11),
/// custom attributes (subtypes 17 / 18), value-label-set linkage
/// (type 3 + type 4), and very long string width (subtype 14).
///
/// This split intentionally avoids a soft "this looks complete but
/// isn't" contract: the wire-level type and the reconciled type are
/// distinct.
///
/// Missing-value bytes are carried verbatim on
/// [`missing_values`](Self::missing_values); numeric sentinel
/// substitution and string-encoding decode happen only during the
/// dictionary reader's finalization pass.
#[derive(Debug, Clone)]
pub struct SavVariableHeader {
    short_name: String,
    variable_type: VariableType,
    label: Option<String>,
    missing_values: RawMissingValues,
    print_format: SavFormat,
    write_format: SavFormat,
}

impl SavVariableHeader {
    /// Returns a fresh [`SavVariableHeaderBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> SavVariableHeaderBuilder {
        SavVariableHeaderBuilder::default()
    }

    /// Short name (the 8-byte `name` field), decoded through the
    /// reader's active encoding and trimmed of trailing spaces and
    /// NULs.
    #[must_use]
    #[inline]
    pub fn short_name(&self) -> &str {
        &self.short_name
    }

    /// Variable storage type as declared on this record, after any
    /// continuation-record collapse.
    ///
    /// Widths above 255 only appear in the fully reconciled
    /// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable)
    /// (after very-long-string extension records are processed); here
    /// the width is bounded by the 8-bit on-disk encoding.
    #[must_use]
    #[inline]
    pub fn variable_type(&self) -> VariableType {
        self.variable_type
    }

    /// Variable label, if the record's `has_label` flag was set.
    #[must_use]
    #[inline]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Wire-level missing-value bytes, exactly as carried in the
    /// record (no sentinel substitution or encoding decode applied).
    #[must_use]
    #[inline]
    pub fn missing_values(&self) -> &RawMissingValues {
        &self.missing_values
    }

    /// Print format.
    #[must_use]
    #[inline]
    pub fn print_format(&self) -> SavFormat {
        self.print_format
    }

    /// Write format.
    #[must_use]
    #[inline]
    pub fn write_format(&self) -> SavFormat {
        self.write_format
    }
}

/// Builder for [`SavVariableHeader`].
#[derive(Debug, Default, Clone)]
pub struct SavVariableHeaderBuilder {
    short_name: Option<String>,
    variable_type: Option<VariableType>,
    label: Option<String>,
    missing_values: Option<RawMissingValues>,
    print_format: Option<SavFormat>,
    write_format: Option<SavFormat>,
}

impl SavVariableHeaderBuilder {
    /// Sets the short name (the 8-byte `name` field).
    #[must_use]
    #[inline]
    pub fn short_name(mut self, name: impl Into<String>) -> Self {
        self.short_name = Some(name.into());
        self
    }

    /// Sets the variable storage type.
    #[must_use]
    #[inline]
    pub fn variable_type(mut self, variable_type: VariableType) -> Self {
        self.variable_type = Some(variable_type);
        self
    }

    /// Sets the variable label.
    #[must_use]
    #[inline]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Clears the variable label.
    #[must_use]
    #[inline]
    pub fn clear_label(mut self) -> Self {
        self.label = None;
        self
    }

    /// Sets the wire-level missing values.
    #[must_use]
    #[inline]
    pub fn missing_values(mut self, missing_values: RawMissingValues) -> Self {
        self.missing_values = Some(missing_values);
        self
    }

    /// Sets the print format.
    #[must_use]
    #[inline]
    pub fn print_format(mut self, print_format: SavFormat) -> Self {
        self.print_format = Some(print_format);
        self
    }

    /// Sets the write format.
    #[must_use]
    #[inline]
    pub fn write_format(mut self, write_format: SavFormat) -> Self {
        self.write_format = Some(write_format);
        self
    }

    /// Finalizes this builder into a [`SavVariableHeader`].
    ///
    /// Unset fields take wire-canonical defaults: empty short name,
    /// numeric variable type, no label, and
    /// [`RawMissingValues::None`]. Print and write formats default to
    /// a freshly built [`SavFormat`]. Required vs. optional checks
    /// live at write time, not here.
    #[must_use]
    pub fn build(self) -> SavVariableHeader {
        todo!("body lands with the dictionary reader implementation")
    }
}
