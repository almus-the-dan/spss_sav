/// SPSS Data Entry product information (subtype 15 per the spec
/// md; not confirmed against PSPP or `ReadStat`).
pub mod data_entry;
/// Extended number of cases (subtype 16).
pub mod extended_number_of_cases;
/// Top-level extension record enum.
pub mod extension_record;
/// File-level custom attributes (subtype 17).
pub mod file_attribute;
/// Float sentinel values (subtype 4).
pub mod float_sentinels;
/// Long missing values for very-long-string variables (subtype 22).
pub mod long_missing_value_record;
/// One value-label pair inside a long string value labels record
/// (subtype 21).
pub mod long_value_label;
/// Long value labels for very-long-string variables (subtype 21).
pub mod long_value_label_record;
/// Long variable name mappings (subtype 13).
pub mod long_variable_name;
/// Integer-typed environment metadata (subtype 3).
pub mod machine_integer_info;
/// Multiple response sets / MRSETS (subtype 7 pre-v14; subtype 19
/// post-v14 with `CATEGORYLABELS`).
pub mod multiple_response_set;
/// Wire-level payload of an extension subtype-11 record.
pub mod raw_display_parameters;
/// Catch-all for unrecognized extension subtypes.
pub mod unknown_extension;
/// One attribute inside a per-variable attributes record (subtype
/// 18).
pub mod variable_attribute_entry;
/// Per-variable custom attributes (subtype 18).
pub mod variable_attribute_record;
/// Per-variable display parameters (subtype 11).
pub mod variable_display;
/// Named variable groupings (subtype 5).
pub mod variable_sets;
/// Very-long-string widths (subtype 14).
pub mod very_long_string;
