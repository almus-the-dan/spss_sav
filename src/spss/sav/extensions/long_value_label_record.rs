//! Subtype 21 — long value labels.

use crate::spss::sav::extensions::long_value_label::LongValueLabel;

/// One long-value-label record from extension record subtype 21.
///
/// Subtype 21 carries value labels for very-long-string variables,
/// which subtype-3 short-string value labels cannot represent because
/// their key is fixed at 8 bytes. Each record covers one variable:
/// its long name, its declared string `width`, and the list of
/// `(value, label)` pairs.
///
/// `variable_name` is decoded through the file's active encoding (as
/// with subtype-13 long names) so it can be matched against the
/// schema during finalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongValueLabelRecord {
    variable_name: String,
    width: u32,
    labels: Vec<LongValueLabel>,
}

impl LongValueLabelRecord {
    /// Returns a fresh [`LongValueLabelRecordBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongValueLabelRecordBuilder {
        LongValueLabelRecordBuilder::default()
    }

    /// The long name of the variable these labels apply to.
    #[must_use]
    #[inline]
    pub fn variable_name(&self) -> &str {
        &self.variable_name
    }

    /// The variable's declared string width, as recorded on disk.
    #[must_use]
    #[inline]
    pub fn width(&self) -> u32 {
        self.width
    }

    /// The `(value, label)` pairs, in on-disk order.
    #[must_use]
    #[inline]
    pub fn labels(&self) -> &[LongValueLabel] {
        &self.labels
    }
}

/// Builder for [`LongValueLabelRecord`].
#[derive(Debug, Default, Clone)]
pub struct LongValueLabelRecordBuilder {
    variable_name: Option<String>,
    width: Option<u32>,
    labels: Vec<LongValueLabel>,
}

impl LongValueLabelRecordBuilder {
    /// Sets the long variable name.
    #[must_use]
    #[inline]
    pub fn variable_name(mut self, value: impl Into<String>) -> Self {
        self.variable_name = Some(value.into());
        self
    }

    /// Sets the variable's declared string width.
    #[must_use]
    #[inline]
    pub fn width(mut self, width: u32) -> Self {
        self.width = Some(width);
        self
    }

    /// Appends one `(value, label)` pair.
    #[must_use]
    #[inline]
    pub fn add_label(mut self, label: LongValueLabel) -> Self {
        self.labels.push(label);
        self
    }

    /// Appends `labels` to the record.
    #[must_use]
    #[inline]
    pub fn add_labels(mut self, labels: Vec<LongValueLabel>) -> Self {
        self.labels.extend(labels);
        self
    }

    /// Finalizes this builder into a [`LongValueLabelRecord`].
    ///
    /// An unset variable name defaults to the empty string; an unset
    /// width defaults to `0`; unset labels default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongValueLabelRecord {
        LongValueLabelRecord {
            variable_name: self.variable_name.unwrap_or_default(),
            width: self.width.unwrap_or_default(),
            labels: self.labels,
        }
    }
}
