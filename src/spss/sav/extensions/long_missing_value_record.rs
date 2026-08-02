//! Subtype 22 — long missing values.

/// One long-missing-value record from extension record subtype 22.
///
/// Subtype 22 carries user-defined missing values for very-long-string
/// variables; the schema's per-variable `MissingValueSpecification`
/// slot is limited to 8-byte keys and cannot represent these directly.
/// Each record covers one variable: its long name and its 1–3 missing
/// values. On disk every value shares a single declared width, so the
/// raw bytes of each value all have that length.
///
/// The `values` are kept as raw bytes (mirroring the type-3
/// value-label key) to preserve trailing-space padding;
/// `variable_name` is decoded through the file's active encoding so it
/// can be matched against the schema during finalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LongMissingValueRecord {
    variable_name: String,
    values: Vec<Vec<u8>>,
}

impl LongMissingValueRecord {
    /// Returns a fresh [`LongMissingValueRecordBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> LongMissingValueRecordBuilder {
        LongMissingValueRecordBuilder::default()
    }

    /// The long name of the variable these missing values apply to.
    #[must_use]
    #[inline]
    pub fn variable_name(&self) -> &str {
        &self.variable_name
    }

    /// The missing values' raw bytes, in on-disk order (1–3 entries).
    #[must_use]
    #[inline]
    pub fn values(&self) -> &[Vec<u8>] {
        &self.values
    }
}

/// Builder for [`LongMissingValueRecord`].
#[derive(Debug, Default, Clone)]
pub struct LongMissingValueRecordBuilder {
    variable_name: Option<String>,
    values: Vec<Vec<u8>>,
}

impl LongMissingValueRecordBuilder {
    /// Sets the long variable name.
    #[must_use]
    #[inline]
    pub fn variable_name(mut self, value: impl Into<String>) -> Self {
        self.variable_name = Some(value.into());
        self
    }

    /// Appends one missing value's raw bytes.
    #[must_use]
    #[inline]
    pub fn add_value(mut self, value: Vec<u8>) -> Self {
        self.values.push(value);
        self
    }

    /// Appends `values` to the record.
    #[must_use]
    #[inline]
    pub fn add_values(mut self, values: impl IntoIterator<Item = Vec<u8>>) -> Self {
        self.values.extend(values);
        self
    }

    /// Finalizes this builder into a [`LongMissingValueRecord`].
    ///
    /// An unset variable name defaults to the empty string; unset
    /// values default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> LongMissingValueRecord {
        LongMissingValueRecord {
            variable_name: self.variable_name.unwrap_or_default(),
            values: self.values,
        }
    }
}
