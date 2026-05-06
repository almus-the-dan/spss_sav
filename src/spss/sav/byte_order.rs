//! Byte order primitive shared across SPSS binary formats.

/// Byte order (endianness) of multibyte values in a binary SPSS
/// file.
///
/// SAV detects byte order by reading a known integer (`2`) from the
/// header and seeing whether it matches in the native order or the
/// byte-swapped order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ByteOrder {
    /// Most-significant byte first.
    BigEndian,
    /// Least-significant byte first.
    LittleEndian,
}
