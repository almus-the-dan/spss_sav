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

impl ByteOrder {
    /// Decodes a `u32` from a 4-byte array.
    #[allow(dead_code)] // exercised once the dictionary reader lands.
    #[must_use]
    pub(crate) fn read_u32(self, bytes: [u8; 4]) -> u32 {
        match self {
            Self::BigEndian => u32::from_be_bytes(bytes),
            Self::LittleEndian => u32::from_le_bytes(bytes),
        }
    }

    /// Decodes an `i32` from a 4-byte array.
    #[must_use]
    pub(crate) fn read_i32(self, bytes: [u8; 4]) -> i32 {
        match self {
            Self::BigEndian => i32::from_be_bytes(bytes),
            Self::LittleEndian => i32::from_le_bytes(bytes),
        }
    }

    /// Decodes an `f64` from an 8-byte array.
    #[must_use]
    pub(crate) fn read_f64(self, bytes: [u8; 8]) -> f64 {
        match self {
            Self::BigEndian => f64::from_be_bytes(bytes),
            Self::LittleEndian => f64::from_le_bytes(bytes),
        }
    }

    /// Decodes an `i64` from an 8-byte array.
    #[must_use]
    pub(crate) fn read_i64(self, bytes: [u8; 8]) -> i64 {
        match self {
            Self::BigEndian => i64::from_be_bytes(bytes),
            Self::LittleEndian => i64::from_le_bytes(bytes),
        }
    }
}
