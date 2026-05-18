//! Subtype 16 — extended number of cases.

/// Extended number-of-cases record from extension record subtype 16.
///
/// Authoritative when the header's `case_count` field is `-1`
/// (i.e., the case count overflows the 32-bit field). The on-disk
/// payload is two `i64`s: a [`version`](Self::version) flag
/// (`ReadStat`'s writer always emits `1`) and the actual
/// [`count`](Self::count) of cases in the file.
///
/// The role of the version flag is not formally documented; the
/// reader surfaces it verbatim so consumers can inspect or
/// round-trip it without interpreting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ExtendedNumberOfCases {
    version: i64,
    count: i64,
}

impl ExtendedNumberOfCases {
    /// Returns a fresh [`ExtendedNumberOfCasesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> ExtendedNumberOfCasesBuilder {
        ExtendedNumberOfCasesBuilder::default()
    }

    /// Version flag from the record body. `ReadStat`'s writer always
    /// emits `1`; other values appear only in files written by
    /// other tools or in malformed files.
    #[must_use]
    #[inline]
    pub fn version(&self) -> i64 {
        self.version
    }

    /// Authoritative case count when the header's `case_count`
    /// field is `-1`.
    #[must_use]
    #[inline]
    pub fn count(&self) -> i64 {
        self.count
    }
}

/// Builder for [`ExtendedNumberOfCases`].
#[derive(Debug, Default, Clone, Copy)]
pub struct ExtendedNumberOfCasesBuilder {
    version: Option<i64>,
    count: Option<i64>,
}

impl ExtendedNumberOfCasesBuilder {
    /// Sets the version flag.
    #[must_use]
    #[inline]
    pub fn version(mut self, value: i64) -> Self {
        self.version = Some(value);
        self
    }

    /// Sets the case count.
    #[must_use]
    #[inline]
    pub fn count(mut self, value: i64) -> Self {
        self.count = Some(value);
        self
    }

    /// Finalizes this builder into an [`ExtendedNumberOfCases`].
    ///
    /// Unset fields default to `0`.
    #[must_use]
    #[inline]
    pub fn build(self) -> ExtendedNumberOfCases {
        let version = self.version.unwrap_or(0);
        let count = self.count.unwrap_or(0);
        ExtendedNumberOfCases { version, count }
    }
}
