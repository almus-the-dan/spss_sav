//! One value-label pair inside a long string value labels record
//! (subtype 21).

/// A single `(value, label)` pair for a very-long-string variable,
/// from extension record subtype 21.
///
/// The `value` is kept as raw bytes (mirroring the type-3
/// value-label key, which is a raw `[u8; 8]`) so trailing-space
/// padding and non-round-trippable bytes are preserved; the `label`
/// is decoded through the file's active encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongValueLabel {
    value: Vec<u8>,
    label: String,
}

impl LongValueLabel {
    /// Returns a fresh [`LongValueLabelBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongValueLabelBuilder {
        LongValueLabelBuilder::default()
    }

    /// The value's raw bytes, verbatim from disk.
    #[must_use]
    #[inline]
    pub fn value(&self) -> &[u8] {
        &self.value
    }

    /// The label, decoded through the file's active encoding.
    #[must_use]
    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Builder for [`LongValueLabel`].
#[derive(Debug, Default, Clone)]
pub struct LongValueLabelBuilder {
    value: Vec<u8>,
    label: Option<String>,
}

impl LongValueLabelBuilder {
    /// Sets the value's raw bytes.
    #[must_use]
    #[inline]
    pub fn value(mut self, value: Vec<u8>) -> Self {
        self.value = value;
        self
    }

    /// Sets the label.
    #[must_use]
    #[inline]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Finalizes this builder into a [`LongValueLabel`].
    ///
    /// An unset value defaults to empty bytes; an unset label
    /// defaults to the empty string.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongValueLabel {
        LongValueLabel {
            value: self.value,
            label: self.label.unwrap_or_default(),
        }
    }
}
