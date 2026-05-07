//! Compression scheme of a SAV file.

/// Compression scheme used for the data records of a SAV file.
///
/// SAV's "bytecode" compression encodes runs of system-missing,
/// constant offsets, and uncompressed values inline; ZSAV files use
/// raw zlib over the data section instead. The two paths are
/// negotiated at the same point in the header but never combined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Compression {
    /// No compression — each cell occupies its full eight-byte slot.
    None,
    /// SAV bytecode compression.
    Bytecode,
    /// ZSAV zlib compression.
    Zlib,
}
