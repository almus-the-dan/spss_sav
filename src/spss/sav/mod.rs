/// Byte order of multi-byte values in a SAV file.
pub mod byte_order;
/// Reader policy for choosing a text encoding.
pub mod encoding_strategy;
/// SAV extension records (subtypes 3, 4, 5, 6, 7, 7B, 8, 11, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, ...).
pub mod extensions;
/// What the SAV file declared about its text encoding.
pub mod file_encoding;
/// On-disk floating-point representation of `f64` values.
pub mod float_format;
/// A variable's missing-value specification.
pub mod missing_value_spec;
/// Endpoint of a missing-value range.
pub mod range_bound;
/// SAV-format-specific errors.
pub mod sav_error;
/// Recoverable issues raised during SAV reading or writing.
pub mod sav_warning;
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
/// Storage type of a SAV variable.
pub mod variable_type;
