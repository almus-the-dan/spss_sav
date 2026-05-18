//! Subtype 6 — float-format confirmation.

/// Float-format confirmation from extension record subtype 6.
///
/// Carries the same three sentinel values that subtype 4
/// ([`FloatSentinels`](crate::spss::sav::extensions::float_sentinels::FloatSentinels))
/// declares — system missing, `HIGHEST`, `LOWEST` — preserved as
/// raw 8-byte slabs in the file's declared float format. SPSS
/// emits both records for redundancy; the dictionary reader
/// surfaces them separately and emits
/// [`SavWarning::FloatSentinelsCrossCheckMismatch`](crate::spss::sav::sav_warning::SavWarning::FloatSentinelsCrossCheckMismatch)
/// when the two records disagree.
///
/// Consumers convert to `f64` using
/// [`SavHeader::float_format`](crate::spss::sav::sav_header::SavHeader::float_format)
/// and the file's byte order, exactly as for `FloatSentinels`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineFloatInfo {
    system_missing: [u8; 8],
    highest: [u8; 8],
    lowest: [u8; 8],
}

impl MachineFloatInfo {
    /// Returns a fresh [`MachineFloatInfoBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> MachineFloatInfoBuilder {
        MachineFloatInfoBuilder::default()
    }

    /// Raw bytes of the system-missing sentinel.
    #[must_use]
    #[inline]
    pub fn system_missing(&self) -> [u8; 8] {
        self.system_missing
    }

    /// Raw bytes of the `HIGHEST` sentinel.
    #[must_use]
    #[inline]
    pub fn highest(&self) -> [u8; 8] {
        self.highest
    }

    /// Raw bytes of the `LOWEST` sentinel.
    #[must_use]
    #[inline]
    pub fn lowest(&self) -> [u8; 8] {
        self.lowest
    }
}

/// Builder for [`MachineFloatInfo`].
#[derive(Debug, Default, Clone, Copy)]
pub struct MachineFloatInfoBuilder {
    system_missing: Option<[u8; 8]>,
    highest: Option<[u8; 8]>,
    lowest: Option<[u8; 8]>,
}

impl MachineFloatInfoBuilder {
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

    /// Finalizes this builder into a [`MachineFloatInfo`].
    ///
    /// Unset sentinels default to all-zero bytes.
    #[must_use]
    #[inline]
    pub fn build(self) -> MachineFloatInfo {
        let system_missing = self.system_missing.unwrap_or([0; 8]);
        let highest = self.highest.unwrap_or([0; 8]);
        let lowest = self.lowest.unwrap_or([0; 8]);
        MachineFloatInfo {
            system_missing,
            highest,
            lowest,
        }
    }
}
