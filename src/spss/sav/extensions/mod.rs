/// Where a multiple-dichotomy set's category labels come from
/// (subtypes 7 / 19).
pub mod category_label_source;
/// Declared character encoding (subtype 20).
pub mod character_encoding;
/// Extended number of cases (subtype 16).
pub mod extended_number_of_cases;
/// Parsing helpers shared across extension subtypes.
pub mod extension_parse;
/// Top-level extension record enum.
pub mod extension_record;
/// Wire-level discriminant of a type-7 extension record.
pub mod extension_subtype;
/// Extra product information (subtype 10).
pub mod extra_product_info;
/// File-level custom attributes (subtype 17).
pub mod file_attribute;
/// File-level custom attributes collection (subtype 17).
pub mod file_attributes;
/// Float sentinel values (subtype 4).
pub mod float_sentinels;
/// Long missing values for very-long-string variables (subtype 22).
pub mod long_missing_value_record;
/// Long string missing values collection (subtype 22).
pub mod long_missing_values;
/// One value-label pair inside a long string value labels record
/// (subtype 21).
pub mod long_value_label;
/// Long value labels for very-long-string variables (subtype 21).
pub mod long_value_label_record;
/// Long string value labels collection (subtype 21).
pub mod long_value_labels;
/// Long variable name mappings (subtype 13).
pub mod long_variable_name;
/// Long variable name mappings collection (subtype 13).
pub mod long_variable_names;
/// Integer-typed environment metadata (subtype 3).
pub mod machine_integer_info;
/// Multiple response sets / MRSETS (subtype 7 pre-v14; subtype 19
/// post-v14 with `CATEGORYLABELS`).
pub mod multiple_response_set;
/// The kind of multiple response set (subtypes 7 / 19).
pub mod multiple_response_set_kind;
/// Multiple response sets collection (subtypes 7 / 19).
pub mod multiple_response_sets;
/// Wire-level payload of an extension subtype-11 record.
pub mod raw_display_parameters;
/// Catch-all for unrecognized extension subtypes.
pub mod unknown_extension;
/// File UUID (subtype 12).
pub mod uuid;
/// One attribute inside a per-variable attributes record (subtype
/// 18).
pub mod variable_attribute_entry;
/// Per-variable custom attributes (subtype 18).
pub mod variable_attribute_record;
/// Per-variable custom attributes collection (subtype 18).
pub mod variable_attributes;
/// Per-variable display parameters (subtype 11).
pub mod variable_display;
/// One named variable grouping inside a variable sets record (subtype
/// 5).
pub mod variable_set;
/// Named variable groupings (subtype 5).
pub mod variable_sets;
/// Very-long-string widths (subtype 14).
pub mod very_long_string;
/// Very-long-string widths collection (subtype 14).
pub mod very_long_strings;
