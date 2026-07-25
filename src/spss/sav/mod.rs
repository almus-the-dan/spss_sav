/// Display alignment of a SAV variable.
pub mod alignment;
/// Byte order of multibyte values in a SAV file.
pub mod byte_order;
/// Shared low-level reads over an in-memory byte slice.
mod byte_cursor;
/// Compression scheme of a SAV file.
pub mod compression;
/// On-disk byte layout of the SAV dictionary section.
mod dictionary_format;
/// Pure parse helpers for the SAV dictionary section.
mod dictionary_parse;
/// Streaming reader for the SAV dictionary section.
pub mod dictionary_reader;
/// One typed record from the SAV dictionary section.
pub mod dictionary_record;
/// Free-text document lines from a SAV file.
pub mod document_record;
/// Reader policy for choosing a text encoding.
pub mod encoding_strategy;
/// The decoded header of a type-7 extension record.
mod extension_envelope;
/// SAV extension records (subtypes 3, 4, 5, 6, 7, 7B, 8, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, ...).
pub mod extensions;
/// What the SAV file declared about its text encoding.
pub mod file_encoding;
/// On-disk floating-point representation of `f64` values.
pub mod float_format;
/// On-disk byte layout of the 176-byte SAV file header.
mod header_format;
/// Pure parse helpers for the SAV file header.
mod header_parse;
/// Reader for the 176-byte SAV file header.
pub mod header_reader;
/// A single data record decoded on demand.
pub mod lazy_sav_record;
/// Measurement level of a SAV variable.
pub mod measurement_level;
/// A variable's missing-value specification.
pub mod missing_value_specification;
/// Endpoint of a missing-value range.
pub mod range_bound;
/// Wire-level missing-value bytes from a variable record.
pub mod raw_missing_values;
/// Wire-level value-label entry from a type-3 record.
pub mod raw_value_label_entry;
/// Wire-level value-label set from a paired type-3 + type-4 record.
pub mod raw_value_label_set;
/// Crate-internal per-reader state.
mod reader_state;
/// Reader for the data-record section of a SAV file.
pub mod record_reader;
/// Creation timestamp recorded in a SAV file header.
pub mod sav_creation_timestamp;
/// SAV-format-specific errors.
pub mod sav_error;
/// SAV variable display format.
pub mod sav_format;
/// SAV variable display-format kind.
pub mod sav_format_kind;
/// SAV file header.
pub mod sav_header;
/// Entry point for reading a SAV file.
pub mod sav_reader;
/// A single decoded SAV data record.
pub mod sav_record;
/// Schema of variables in a SAV file.
pub mod sav_schema;
/// A SAV timestamp.
pub mod sav_timestamp;
/// A reconciled SAV variable.
pub mod sav_variable;
/// Wire-level fields of a single SAV type-2 variable record.
pub mod sav_variable_header;
/// Recoverable issues raised during SAV reading or writing.
pub mod sav_warning;
/// Shared `#[cfg(test)]` helpers for building on-disk SAV byte
/// streams.
#[cfg(test)]
mod test_support;
/// Shared helpers for decoding fixed-width text fields.
mod text_field;
/// A single SAV cell value.
pub mod value;
/// A single value-label mapping.
pub mod value_label_entry;
/// A named set of value-label mappings.
pub mod value_label_set;
/// In-memory lookup table for value-label sets.
pub mod value_label_table;
/// Typed key for a value-label entry.
pub mod value_label_value;
/// One custom attribute on a SAV variable.
pub mod variable_attribute;
/// SAV variable storage type.
pub mod variable_type;
