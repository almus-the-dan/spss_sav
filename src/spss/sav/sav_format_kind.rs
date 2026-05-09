//! SAV variable display-format kind.

/// Display-format code from a SAV variable's print/write format.
///
/// Each named variant corresponds to a PSPP-canonical SPSS format
/// code. The on-disk encoding is a single byte;
/// [`Unspecified`](Self::Unspecified) is the canonical disk byte `0`
/// ("no format declared"), and any byte outside the recognized range
/// is preserved verbatim in [`Unknown`](Self::Unknown) for round-trip
/// fidelity.
///
/// Format kinds describe rendering semantics (numeric vs string,
/// date vs time, currency style, etc.) rather than storage. Storage is
/// captured separately by
/// [`VariableType`](crate::spss::sav::variable_type::VariableType).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SavFormatKind {
    /// No format declared (canonical disk byte `0`).
    Unspecified,
    /// `A` — fixed-width string.
    A,
    /// `AHEX` — hex-encoded string.
    AHex,
    /// `COMMA` — numeric with comma grouping.
    Comma,
    /// `DOLLAR` — currency with a dollar sign.
    Dollar,
    /// `F` — plain numeric.
    F,
    /// `IB` — signed integer binary.
    IB,
    /// `PIBHEX` — positive integer binary, hex display.
    PIBHex,
    /// `P` — packed decimal.
    P,
    /// `PIB` — positive integer binary.
    PIB,
    /// `PK` — unsigned packed decimal.
    PK,
    /// `RB` — raw binary `f64`.
    RB,
    /// `RBHEX` — raw binary `f64`, hex display.
    RBHex,
    /// `Z` — zoned decimal.
    Z,
    /// `N` — zero-padded numeric.
    N,
    /// `E` — scientific notation.
    E,
    /// `DATE` — `dd-mmm-yyyy`.
    Date,
    /// `TIME` — `hh:mm:ss`.
    Time,
    /// `DATETIME` — `dd-mmm-yyyy hh:mm:ss`.
    DateTime,
    /// `ADATE` — `mm/dd/yyyy`.
    ADate,
    /// `JDATE` — Julian date `yyyyddd`.
    JDate,
    /// `DTIME` — `dd hh:mm:ss`.
    DTime,
    /// `WKDAY` — day of week.
    WkDay,
    /// `MONTH` — month name.
    Month,
    /// `MOYR` — `mmm yyyy`.
    MoYr,
    /// `QYR` — `q Q yyyy`.
    QYr,
    /// `WKYR` — `ww WK yyyy`.
    WkYr,
    /// `PCT` — percentage.
    Pct,
    /// `DOT` — numeric with dot grouping (European style).
    Dot,
    /// `CCA` — custom currency A.
    CCA,
    /// `CCB` — custom currency B.
    CCB,
    /// `CCC` — custom currency C.
    CCC,
    /// `CCD` — custom currency D.
    CCD,
    /// `CCE` — custom currency E.
    CCE,
    /// `EDATE` — European `dd.mm.yyyy`.
    EDate,
    /// `SDATE` — sortable `yyyy/mm/dd`.
    SDate,
    /// `MTIME` — minute-precision time.
    MTime,
    /// `YMDHMS` — `yyyy-mm-dd hh:mm:ss`.
    YmdHms,
    /// An on-disk byte outside the recognized PSPP-canonical range.
    /// The raw byte is preserved for round-trip fidelity.
    Unknown(u8),
}

impl SavFormatKind {
    /// On-disk byte representation of this format kind.
    ///
    /// The mapping follows PSPP's canonical numeric assignments.
    #[must_use]
    pub(crate) fn to_byte(self) -> u8 {
        match self {
            Self::Unspecified => 0,
            Self::A => 1,
            Self::AHex => 2,
            Self::Comma => 3,
            Self::Dollar => 4,
            Self::F => 5,
            Self::IB => 6,
            Self::PIBHex => 7,
            Self::P => 8,
            Self::PIB => 9,
            Self::PK => 10,
            Self::RB => 11,
            Self::RBHex => 12,
            Self::Z => 15,
            Self::N => 16,
            Self::E => 17,
            Self::Date => 20,
            Self::Time => 21,
            Self::DateTime => 22,
            Self::ADate => 23,
            Self::JDate => 24,
            Self::DTime => 25,
            Self::WkDay => 26,
            Self::Month => 27,
            Self::MoYr => 28,
            Self::QYr => 29,
            Self::WkYr => 30,
            Self::Pct => 31,
            Self::Dot => 32,
            Self::CCA => 33,
            Self::CCB => 34,
            Self::CCC => 35,
            Self::CCD => 36,
            Self::CCE => 37,
            Self::EDate => 38,
            Self::SDate => 39,
            Self::MTime => 40,
            Self::YmdHms => 41,
            Self::Unknown(byte) => byte,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_byte_unspecified() {
        assert_eq!(SavFormatKind::Unspecified.to_byte(), 0);
    }

    #[test]
    fn to_byte_string_kinds() {
        assert_eq!(SavFormatKind::A.to_byte(), 1);
        assert_eq!(SavFormatKind::AHex.to_byte(), 2);
    }

    #[test]
    fn to_byte_numeric_kinds() {
        assert_eq!(SavFormatKind::Comma.to_byte(), 3);
        assert_eq!(SavFormatKind::Dollar.to_byte(), 4);
        assert_eq!(SavFormatKind::F.to_byte(), 5);
        assert_eq!(SavFormatKind::N.to_byte(), 16);
        assert_eq!(SavFormatKind::E.to_byte(), 17);
        assert_eq!(SavFormatKind::Pct.to_byte(), 31);
        assert_eq!(SavFormatKind::Dot.to_byte(), 32);
    }

    #[test]
    fn to_byte_binary_kinds() {
        assert_eq!(SavFormatKind::IB.to_byte(), 6);
        assert_eq!(SavFormatKind::PIBHex.to_byte(), 7);
        assert_eq!(SavFormatKind::P.to_byte(), 8);
        assert_eq!(SavFormatKind::PIB.to_byte(), 9);
        assert_eq!(SavFormatKind::PK.to_byte(), 10);
        assert_eq!(SavFormatKind::RB.to_byte(), 11);
        assert_eq!(SavFormatKind::RBHex.to_byte(), 12);
        assert_eq!(SavFormatKind::Z.to_byte(), 15);
    }

    #[test]
    fn to_byte_date_time_kinds() {
        assert_eq!(SavFormatKind::Date.to_byte(), 20);
        assert_eq!(SavFormatKind::Time.to_byte(), 21);
        assert_eq!(SavFormatKind::DateTime.to_byte(), 22);
        assert_eq!(SavFormatKind::ADate.to_byte(), 23);
        assert_eq!(SavFormatKind::JDate.to_byte(), 24);
        assert_eq!(SavFormatKind::DTime.to_byte(), 25);
        assert_eq!(SavFormatKind::WkDay.to_byte(), 26);
        assert_eq!(SavFormatKind::Month.to_byte(), 27);
        assert_eq!(SavFormatKind::MoYr.to_byte(), 28);
        assert_eq!(SavFormatKind::QYr.to_byte(), 29);
        assert_eq!(SavFormatKind::WkYr.to_byte(), 30);
        assert_eq!(SavFormatKind::EDate.to_byte(), 38);
        assert_eq!(SavFormatKind::SDate.to_byte(), 39);
        assert_eq!(SavFormatKind::MTime.to_byte(), 40);
        assert_eq!(SavFormatKind::YmdHms.to_byte(), 41);
    }

    #[test]
    fn to_byte_custom_currencies() {
        assert_eq!(SavFormatKind::CCA.to_byte(), 33);
        assert_eq!(SavFormatKind::CCB.to_byte(), 34);
        assert_eq!(SavFormatKind::CCC.to_byte(), 35);
        assert_eq!(SavFormatKind::CCD.to_byte(), 36);
        assert_eq!(SavFormatKind::CCE.to_byte(), 37);
    }

    #[test]
    fn to_byte_preserves_unknown() {
        assert_eq!(SavFormatKind::Unknown(13).to_byte(), 13);
        assert_eq!(SavFormatKind::Unknown(42).to_byte(), 42);
        assert_eq!(SavFormatKind::Unknown(255).to_byte(), 255);
    }
}
