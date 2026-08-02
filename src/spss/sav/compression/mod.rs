//! Compression of a SAV file's data section.
//!
//! [`CompressionKind`](compression_kind::CompressionKind) names the
//! scheme; the other submodules implement the two that need
//! implementing. They are grouped here because a ZSAV file is not a
//! second encoding but a container around the first — the bytecode
//! decoder is shared, and only where its command units come from
//! differs.

/// Turning a bytecode command stream into row bytes.
pub(crate) mod bytecode_decoder;
/// Which compression scheme a SAV file's data section uses.
pub mod compression_kind;
/// Where the bytecode decoder's command units come from.
pub(crate) mod data_unit_source;
/// Command units read straight from the file.
pub(crate) mod file_units;
/// Producing raw row bytes, whichever way the file is compressed.
pub(crate) mod row_source;
/// The ZSAV block container.
pub(crate) mod zlib_blocks;
/// The header opening a ZSAV data section.
pub(crate) mod zsav_header;
