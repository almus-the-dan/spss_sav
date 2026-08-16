/// Display alignment of a SAV variable.
pub mod alignment;
/// One buffered dictionary record, paired with the warnings it raised.
mod buffered_dictionary_record;
/// An undecoded type-6 document record.
mod buffered_document_record;
/// The undecoded payload of one buffered dictionary record.
mod buffered_record_payload;
/// One undecoded entry from a type-3 value-label record.
mod buffered_value_label_entry;
/// An undecoded type-3 / type-4 value-label record pair.
mod buffered_value_label_set;
/// A type-2 variable record held undecoded until the encoding is known.
mod buffered_variable_record;
/// Shared low-level reads over an in-memory byte slice.
mod byte_cursor;
/// Byte order of multibyte values in a SAV file.
pub mod byte_order;
/// Compression of a SAV file's data section.
pub mod compression;
/// Everything the record reader needs to decode a data row.
mod data_layout;
/// Buffering the dictionary section so its text can be decoded later.
mod dictionary_buffer;
/// On-disk byte layout of the SAV dictionary section.
mod dictionary_format;
/// Pure parse helpers for the SAV dictionary section.
mod dictionary_parse;
/// Streaming reader for the SAV dictionary section.
pub mod dictionary_reader;
/// One typed record from the SAV dictionary section.
pub mod dictionary_record;
/// Which kind of record a dictionary record is, without its payload.
pub mod dictionary_record_kind;
/// How much of the dictionary the buffering pass keeps.
mod dictionary_retention;
/// Free-text document lines from a SAV file.
pub mod document_record;
/// Where the text encoding the reader applied came from.
pub mod encoding_provenance;
/// Resolving a SAV file's declared encoding to a concrete encoding.
mod encoding_resolution;
/// Reader policy for choosing a text encoding.
pub mod encoding_strategy;
/// The decoded header of a type-7 extension record.
mod extension_envelope;
/// SAV extension records (subtypes 3, 4, 5, 7, 10, 11, 12, 13, 14, 16, 17, 18, 19, 20, 21, 22).
pub mod extensions;
/// How a SAV file encodes an `f64` on disk.
pub mod float_encoding;
/// On-disk floating-point representation of `f64` values.
pub mod float_format;
/// On-disk byte layout of the 176-byte SAV file header.
mod header_format;
/// Pure parse helpers for the SAV file header.
mod header_parse;
/// A single data record decoded on demand.
pub mod lazy_sav_record;
/// Measurement level of a SAV variable.
pub mod measurement_level;
/// Turning wire-level missing-value bytes into a typed specification.
mod missing_value_reconcile;
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
/// Crate-internal bundle of the options set on a `SavReaderBuilder`.
mod reader_options;
/// Crate-internal per-reader state.
mod reader_state;
/// On-disk byte layout of the SAV data-record section.
mod record_format;
/// Pure parse helpers for the SAV data-record section.
mod record_parse;
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
/// An opened SAV file, and the two directions reading can take from it.
pub mod sav_reader;
/// Entry point for reading a SAV file.
pub mod sav_reader_builder;
/// A single decoded SAV data record.
pub mod sav_record;
/// Schema of variables in a SAV file, and the accumulator that builds one.
pub mod sav_schema;
/// A SAV timestamp.
pub mod sav_timestamp;
/// A reconciled SAV variable.
pub mod sav_variable;
/// Wire-level fields of a single SAV type-2 variable record.
pub mod sav_variable_header;
/// Recoverable issues raised during SAV reading or writing.
pub mod sav_warning;
/// On-disk placement of one variable's bytes within a data row.
mod segment_layout;
/// A string cell's value: raw bytes, decoded on demand.
pub mod string_value;
/// Shared `#[cfg(test)]` helpers for building on-disk SAV byte
/// streams.
#[cfg(test)]
mod test_support;
/// A string cell, classified against its variable's missing values.
pub mod text;
/// Shared helpers for decoding fixed-width text fields.
mod text_field;
/// A single SAV cell value.
pub mod value;
/// A single value-label mapping.
pub mod value_label_entry;
/// A set of value-label mappings attached to a variable.
pub mod value_label_set;
/// Typed key for a value-label entry.
pub mod value_label_value;
/// One custom attribute on a SAV variable.
pub mod variable_attribute;
/// Collapsing indexed attribute names into array-valued attributes.
mod variable_attribute_reconcile;
/// On-disk placement of one logical variable within a data row.
mod variable_layout;
/// SAV variable storage type.
pub mod variable_type;
