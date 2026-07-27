//! On-disk floating-point representation of `f64` values.

use core::fmt;

/// On-disk floating-point representation declared by a SAV file.
///
/// Most SAV files use IEEE 754. IBM HFP and VAX formats appear in
/// legacy files written on those platforms and are translated to
/// IEEE 754 on read (and back when written).
///
/// VAX supplies two distinct 64-bit encodings, `D_float` and
/// `G_float`, which differ in how they split the exponent and
/// mantissa. Both are separate variants here because the same eight
/// bytes decode to different numbers under each, so the distinction
/// cannot be deferred.
///
/// Byte order is a separate axis for IEEE 754 only. IBM HFP is always
/// big-endian and the VAX formats always use VAX word order, so
/// [`FloatEncoding`](crate::spss::sav::float_encoding::FloatEncoding)
/// ignores the file's declared byte order for those three.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FloatFormat {
    /// IEEE 754 binary64.
    Ieee754,
    /// IBM hexadecimal floating-point.
    IbmHfp,
    /// VAX `D_float`: 8-bit exponent, 55-bit mantissa.
    VaxDFloat,
    /// VAX `G_float`: 11-bit exponent, 52-bit mantissa.
    VaxGFloat,
}

impl fmt::Display for FloatFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ieee754 => "IEEE 754",
            Self::IbmHfp => "IBM HFP",
            Self::VaxDFloat => "VAX D_float",
            Self::VaxGFloat => "VAX G_float",
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn float_format_display() {
        assert_eq!(FloatFormat::Ieee754.to_string(), "IEEE 754");
        assert_eq!(FloatFormat::IbmHfp.to_string(), "IBM HFP");
        assert_eq!(FloatFormat::VaxDFloat.to_string(), "VAX D_float");
        assert_eq!(FloatFormat::VaxGFloat.to_string(), "VAX G_float");
    }
}
