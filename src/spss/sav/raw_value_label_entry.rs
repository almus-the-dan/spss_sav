//! Wire-level value-label entry from a type-3 value-label record.

/// One value-label mapping carried verbatim from a type-3 record.
///
/// The 8-byte `value` is held as raw bytes (no `f64` decode, no
/// encoding decode) because the SAV format defers the
/// numeric vs. string decision to the paired type-4 record — the
/// type-3 record alone doesn't know which storage shape the bytes
/// belong to. Type narrowing into
/// [`ValueLabelValue`](crate::spss::sav::value_label_value::ValueLabelValue)
/// happens during the dictionary reader's finalization pass, when
/// the referenced variables' types are known.
///
/// The `label` has already been decoded through the file's active
/// encoding and stripped of any trailing padding; only the
/// label-value pairing is wire-level here, not the label text itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawValueLabelEntry {
    value: [u8; 8],
    label: String,
}

impl RawValueLabelEntry {
    /// Returns a fresh [`RawValueLabelEntryBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> RawValueLabelEntryBuilder {
        RawValueLabelEntryBuilder::default()
    }

    /// Raw 8-byte value as it appeared in the type-3 record. Carries
    /// either an `f64` in the file's byte order (numeric variables)
    /// or padded byte string (string variables); the interpretation
    /// is fixed only once the paired type-4 record ties this set to
    /// a typed variable.
    #[must_use]
    #[inline]
    pub fn value(&self) -> [u8; 8] {
        self.value
    }

    /// Decoded label string, with trailing padding stripped.
    #[must_use]
    #[inline]
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Builder for [`RawValueLabelEntry`].
#[derive(Debug, Default, Clone)]
pub struct RawValueLabelEntryBuilder {
    value: Option<[u8; 8]>,
    label: Option<String>,
}

impl RawValueLabelEntryBuilder {
    /// Sets the raw 8-byte value.
    #[must_use]
    #[inline]
    pub fn value(mut self, value: [u8; 8]) -> Self {
        self.value = Some(value);
        self
    }

    /// Sets the decoded label string.
    #[must_use]
    #[inline]
    pub fn label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Finalizes this builder into a [`RawValueLabelEntry`].
    ///
    /// Unset fields take wire-canonical defaults: an all-zero value
    /// and an empty label.
    #[must_use]
    #[inline]
    pub fn build(self) -> RawValueLabelEntry {
        let value = self.value.unwrap_or([0; 8]);
        let label = self.label.unwrap_or_default();
        RawValueLabelEntry {
            value,
            label,
        }
    }
}
