//! Measurement level of a SAV variable.

/// Measurement level of a SAV variable.
///
/// SPSS distinguishes three measurement levels for analysis purposes:
/// *nominal* (unordered categories), *ordinal* (ranked categories
/// without uniform spacing), and *scale* (a.k.a. "interval/ratio" —
/// continuous numeric values). The level is informational metadata —
/// it does not change how values are stored — but downstream
/// statistical procedures use it to choose appropriate defaults.
///
/// [`Unspecified`](Self::Unspecified) corresponds to the canonical
/// PSPP "no level declared" byte (`0`) and is not a warning case.
/// [`Unknown`](Self::Unknown) preserves any on-disk byte outside the
/// recognized `0..=3` range verbatim, for round-trip fidelity, and
/// the reader emits
/// [`SavWarning::UnknownMeasurementLevel`](crate::spss::sav::sav_warning::SavWarning::UnknownMeasurementLevel)
/// in that case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MeasurementLevel {
    /// No measurement level declared (canonical disk byte `0`).
    Unspecified,
    /// Unordered categorical values.
    Nominal,
    /// Ranked categorical values without uniform spacing.
    Ordinal,
    /// Continuous numeric values (interval or ratio).
    Scale,
    /// An on-disk byte outside `0..=3`. The raw byte is preserved
    /// for round-trip fidelity.
    Unknown(u8),
}

impl MeasurementLevel {
    /// On-disk byte representation of this measurement level.
    #[must_use]
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Unspecified => 0,
            Self::Nominal => 1,
            Self::Ordinal => 2,
            Self::Scale => 3,
            Self::Unknown(byte) => byte,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_byte_canonical_levels() {
        assert_eq!(MeasurementLevel::Unspecified.to_byte(), 0);
        assert_eq!(MeasurementLevel::Nominal.to_byte(), 1);
        assert_eq!(MeasurementLevel::Ordinal.to_byte(), 2);
        assert_eq!(MeasurementLevel::Scale.to_byte(), 3);
    }

    #[test]
    fn to_byte_preserves_unknown() {
        assert_eq!(MeasurementLevel::Unknown(7).to_byte(), 7);
        assert_eq!(MeasurementLevel::Unknown(255).to_byte(), 255);
    }
}
