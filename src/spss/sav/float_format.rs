//! On-disk floating-point representation of `f64` values.

/// On-disk floating-point representation declared by a SAV file.
///
/// Most SAV files use IEEE 754. IBM HFP and VAX formats appear in
/// legacy files written on those platforms and are translated to
/// IEEE 754 on read (and back when written).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum FloatFormat {
    /// IEEE 754 binary64.
    Ieee754,
    /// IBM hexadecimal floating-point.
    IbmHfp,
    /// VAX floating-point.
    Vax,
}
