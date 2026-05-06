/// SPSS Data Entry product information (subtype 15).
pub mod data_entry;
/// Top-level extension record enum.
pub mod extension_record;
/// File-level custom attributes (subtype 18).
pub mod file_attribute;
/// Float sentinel values (subtype 4).
pub mod float_sentinels;
/// Long missing values for very-long-string variables (subtype 22).
pub mod long_missing_value_record;
/// Long value labels for very-long-string variables (subtype 21).
pub mod long_value_label_record;
/// Long variable name mappings (subtype 13).
pub mod long_variable_name;
/// Float-format confirmation (subtype 6).
pub mod machine_float_info;
/// Integer-typed environment metadata (subtype 5).
pub mod machine_integer_info;
/// Multiple response sets / MRSETS (subtypes 19 / 7B).
pub mod multiple_response_set;
/// Catch-all for unrecognized extension subtypes.
pub mod unknown_extension;
/// Per-variable custom attributes (subtype 17).
pub mod variable_attribute_record;
/// Per-variable display parameters (subtype 11).
pub mod variable_display;
/// Named variable groupings (subtype 7).
pub mod variable_sets;
/// Very-long-string widths (subtype 14).
pub mod very_long_string;
