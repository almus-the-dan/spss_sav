//! Subtype 4 — float sentinel values.

/// Float sentinel values declared by extension record subtype 4.
///
/// Carries the file's system-missing bit pattern plus the `LOWEST`
/// and `HIGHEST` open-bound markers used by missing-value range
/// declarations. All three are preserved as raw 8-byte slabs in the
/// file's declared float format (IEEE 754, IBM HFP, or VAX), so
/// byte-equality comparisons against cell values stay unambiguous
/// regardless of float format, and roundtrip is bit-exact.
///
/// Consumers convert to `f64` using
/// [`SavHeader::float_format`](crate::spss::sav::sav_header::SavHeader::float_format)
/// and the file's byte order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FloatSentinels {
    system_missing: [u8; 8],
    highest: [u8; 8],
    lowest: [u8; 8],
}

impl FloatSentinels {
    /// Returns a fresh [`FloatSentinelsBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> FloatSentinelsBuilder {
        FloatSentinelsBuilder::default()
    }

    /// Raw bytes of the system-missing sentinel.
    #[must_use]
    #[inline]
    pub fn system_missing(&self) -> [u8; 8] {
        self.system_missing
    }

    /// Raw bytes of the `HIGHEST` sentinel (upper open bound for
    /// missing-value range declarations).
    #[must_use]
    #[inline]
    pub fn highest(&self) -> [u8; 8] {
        self.highest
    }

    /// Raw bytes of the `LOWEST` sentinel (lower open bound for
    /// missing-value range declarations).
    #[must_use]
    #[inline]
    pub fn lowest(&self) -> [u8; 8] {
        self.lowest
    }
}

/// Builder for [`FloatSentinels`].
#[derive(Debug, Default, Clone, Copy)]
pub struct FloatSentinelsBuilder {
    system_missing: Option<[u8; 8]>,
    highest: Option<[u8; 8]>,
    lowest: Option<[u8; 8]>,
}

impl FloatSentinelsBuilder {
    /// Sets the system-missing sentinel.
    #[must_use]
    #[inline]
    pub fn system_missing(mut self, bytes: [u8; 8]) -> Self {
        self.system_missing = Some(bytes);
        self
    }

    /// Sets the `HIGHEST` sentinel.
    #[must_use]
    #[inline]
    pub fn highest(mut self, bytes: [u8; 8]) -> Self {
        self.highest = Some(bytes);
        self
    }

    /// Sets the `LOWEST` sentinel.
    #[must_use]
    #[inline]
    pub fn lowest(mut self, bytes: [u8; 8]) -> Self {
        self.lowest = Some(bytes);
        self
    }

    /// Finalizes this builder into a [`FloatSentinels`].
    ///
    /// Unset sentinels default to all-zero bytes.
    #[must_use]
    #[inline]
    pub fn build(self) -> FloatSentinels {
        let system_missing = self.system_missing.unwrap_or([0; 8]);
        let highest = self.highest.unwrap_or([0; 8]);
        let lowest = self.lowest.unwrap_or([0; 8]);
        FloatSentinels {
            system_missing,
            highest,
            lowest,
        }
    }
}
