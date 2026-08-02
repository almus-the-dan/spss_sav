//! Integration tests against real `.sav` files written by GNU PSPP.
//!
//! Hand-built payloads only exercise our own reading of the format;
//! they cannot catch a misinterpreted spec. These tests read fixtures
//! that PSPP actually wrote (see `tests/fixtures/*.sps` for the
//! generators) and assert the parsed dictionary, locking in
//! correctness against ground truth.
//!
//! The fixtures embed a generation-time creation timestamp (and any
//! DOCUMENT "(Entered ...)" line), so these tests deliberately avoid
//! asserting on those volatile values.

use std::fs::File;
use std::io::{BufReader, Cursor};

use spss_sav::spss::missing_value::MissingValue;
use spss_sav::spss::numeric::Numeric;
use spss_sav::spss::sav::byte_order::ByteOrder;
use spss_sav::spss::sav::compression::compression_kind::CompressionKind;
use spss_sav::spss::sav::dictionary_reader::DictionaryReader;
use spss_sav::spss::sav::dictionary_record::DictionaryRecord;
use spss_sav::spss::sav::document_record::DocumentRecord;
use spss_sav::spss::sav::encoding_provenance::EncodingProvenance;
use spss_sav::spss::sav::encoding_strategy::EncodingStrategy;
use spss_sav::spss::sav::extensions::category_label_source::CategoryLabelSource;
use spss_sav::spss::sav::extensions::extension_record::ExtensionRecord;
use spss_sav::spss::sav::extensions::float_sentinels::FloatSentinels;
use spss_sav::spss::sav::extensions::multiple_response_set::MultipleResponseSet;
use spss_sav::spss::sav::extensions::multiple_response_set_kind::MultipleResponseSetKind;
use spss_sav::spss::sav::float_format::FloatFormat;
use spss_sav::spss::sav::raw_missing_values::RawMissingValues;
use spss_sav::spss::sav::raw_value_label_set::RawValueLabelSet;
use spss_sav::spss::sav::record_reader::RecordReader;
use spss_sav::spss::sav::sav_error::{FormatErrorKind, SavError, Section};
use spss_sav::spss::sav::sav_reader::SavReader;
use spss_sav::spss::sav::sav_record::SavRecord;
use spss_sav::spss::sav::sav_variable_header::SavVariableHeader;
use spss_sav::spss::sav::sav_warning::SavWarning;
use spss_sav::spss::sav::text::Text;
use spss_sav::spss::sav::value::Value;
use spss_sav::spss::sav::variable_type::VariableType;

const COMPRESSION_NONE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/compression_none.sav"
);

const COMPREHENSIVE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/comprehensive.sav"
);

const WEIGHTED: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/weighted.sav");

const ENCODING_UTF8: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/encoding_utf8.sav"
);

const ENCODING_WINDOWS_1252: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/encoding_windows1252.sav"
);

/// Reads every dictionary record from `path`, asserting the header
/// along the way, and returns the records.
fn read_dictionary(path: &str) -> Vec<DictionaryRecord> {
    let header_reader = SavReader::new().from_path(path).expect("open fixture");
    let mut dictionary_reader = header_reader.read_header().expect("read header");

    assert_header(&dictionary_reader);

    let mut records = Vec::new();
    while let Some(record) = dictionary_reader
        .read_record()
        .expect("read dictionary record")
    {
        records.push(record);
    }
    records
}

fn assert_header(dictionary_reader: &DictionaryReader<BufReader<File>>) {
    let header = dictionary_reader.header();
    assert!(
        header.product_name().contains("pspp"),
        "product_name = {:?}",
        header.product_name()
    );
    assert_eq!(header.compression(), CompressionKind::Bytecode);
    assert_eq!(header.byte_order(), ByteOrder::LittleEndian);
    assert_eq!(header.float_format(), FloatFormat::Ieee754);
    assert_eq!(header.case_count(), Some(2));

    assert!(
        dictionary_reader.warnings().is_empty(),
        "unexpected header warnings: {:?}",
        dictionary_reader.warnings()
    );
}

/// Reads an encoding fixture, returning its decoded file label
/// alongside the dictionary records.
///
/// Unlike [`read_dictionary`] this asserts nothing about the header:
/// the encoding fixtures carry a different variable set than
/// `comprehensive.sav`, and the file label is the point of interest.
fn read_encoding_fixture(path: &str) -> (String, Vec<DictionaryRecord>) {
    let header_reader = SavReader::new().from_path(path).expect("open fixture");
    let mut dictionary_reader = header_reader.read_header().expect("read header");
    let file_label = dictionary_reader.header().file_label().to_owned();

    let mut records = Vec::new();
    while let Some(record) = dictionary_reader
        .read_record()
        .expect("read dictionary record")
    {
        records.push(record);
    }
    (file_label, records)
}

/// Asserts that the accented text in an encoding fixture decoded
/// correctly, covering each text-bearing record kind: the header file
/// label, variable labels (type 2), value labels (type 3), document
/// lines (type 6), and a file attribute value (subtype 17).
///
/// Both encoding fixtures carry identical text, so both must satisfy
/// this regardless of the encoding they were written in.
fn assert_accented_text_decoded(file_label: &str, records: &[DictionaryRecord]) {
    assert_eq!(file_label, "Fichier de démonstration");

    let variables = variables(records);
    assert_eq!(variables[0].label(), Some("Identifiant"));
    assert_eq!(variables[1].label(), Some("Prénom accentué"));

    let sets = value_label_sets(records);
    assert_eq!(sets.len(), 1);
    let entries = sets[0].entries();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].label(), "Café crème");
    assert_eq!(entries[1].label(), "Thé glacé");

    let documents = documents(records);
    let has_accented_line = documents.iter().any(|d| {
        d.lines()
            .iter()
            .any(|l| l.contains("ligne documentaire accentuée"))
    });
    assert!(has_accented_line, "documents = {documents:?}");

    let extensions = extensions(records);
    let file_attributes = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::FileAttributes(a) => Some(a.attributes()),
            _ => None,
        })
        .expect("file attributes");
    let author = file_attributes
        .iter()
        .find(|a| a.name() == "Auteur")
        .expect("Auteur attribute");
    assert_eq!(author.values(), ["Ångström".to_string()]);
}

fn variables(records: &[DictionaryRecord]) -> Vec<&SavVariableHeader> {
    records
        .iter()
        .filter_map(|r| match r {
            DictionaryRecord::Variable(v) => Some(v),
            _ => None,
        })
        .collect()
}

fn extensions(records: &[DictionaryRecord]) -> Vec<&ExtensionRecord> {
    records
        .iter()
        .filter_map(|r| match r {
            DictionaryRecord::Extension(e) => Some(e),
            _ => None,
        })
        .collect()
}

fn value_label_sets(records: &[DictionaryRecord]) -> Vec<&RawValueLabelSet> {
    records
        .iter()
        .filter_map(|r| match r {
            DictionaryRecord::ValueLabelSet(s) => Some(s),
            _ => None,
        })
        .collect()
}

fn documents(records: &[DictionaryRecord]) -> Vec<&DocumentRecord> {
    records
        .iter()
        .filter_map(|r| match r {
            DictionaryRecord::Document(d) => Some(d),
            _ => None,
        })
        .collect()
}

#[test]
fn comprehensive_variables_and_missing_values() {
    let records = read_dictionary(COMPREHENSIVE);
    let variables = variables(&records);

    // A 300-byte string (`longstr`) is stored by PSPP as two physical
    // segment variables (LONGSTR + LONGST_A); the very-long-strings
    // extension ties them together.
    let names: Vec<&str> = variables.iter().map(|v| v.short_name()).collect();
    assert_eq!(
        names,
        ["ID", "Q1", "Q2", "Q3", "LONGSTR", "LONGST_A", "SHORTSTR"]
    );

    let id = variables[0];
    assert_eq!(id.variable_type(), VariableType::Numeric);
    assert_eq!(id.label(), Some("Identifier"));
    match id.missing_values() {
        RawMissingValues::Discrete(values) => {
            assert_eq!(values.len(), 1);
            assert_eq!(values[0], 99.0f64.to_le_bytes());
        }
        other => panic!("expected one discrete missing value, got {other:?}"),
    }

    assert_eq!(variables[1].label(), Some("Question 1"));
    // First very-long-string segment is a full 255-byte slot.
    assert_eq!(variables[4].variable_type(), VariableType::String(255));
    assert_eq!(variables[6].variable_type(), VariableType::String(4));
}

#[test]
fn comprehensive_value_labels_and_documents() {
    let records = read_dictionary(COMPREHENSIVE);

    let sets = value_label_sets(&records);
    assert_eq!(sets.len(), 1);
    let set = sets[0];
    // No/Yes applied to q1, q2, q3 (logical indices 1, 2, 3).
    assert_eq!(set.segment_indices(), [1, 2, 3]);
    assert_eq!(set.entries().len(), 2);
    assert_eq!(set.entries()[0].label(), "No");
    assert_eq!(set.entries()[1].label(), "Yes");
    // 0.0 -> "No", 1.0 -> "Yes" (IEEE 754 little-endian bytes).
    assert_eq!(set.entries()[0].value(), 0.0f64.to_le_bytes());
    assert_eq!(set.entries()[1].value(), 1.0f64.to_le_bytes());

    let documents = documents(&records);
    let contains = documents.iter().any(|d| {
        d.lines()
            .iter()
            .any(|l| l.contains("documentary line for testing"))
    });
    assert!(contains, "documents = {documents:?}");
}

/// The subtype-4 sentinels PSPP wrote must be exactly what
/// [`FloatSentinels::spss_defaults`] produces for this file's
/// encoding. This is the ground-truth check that our idea of the
/// canonical triple matches what a real writer emits — including the
/// one-ULP gap between `LOWEST` and system-missing.
#[test]
fn comprehensive_float_sentinels_match_our_defaults() {
    let header_reader = SavReader::new()
        .from_path(COMPREHENSIVE)
        .expect("open fixture");
    let mut dictionary_reader = header_reader.read_header().expect("read header");
    let encoding = dictionary_reader.header().float_encoding();

    let mut sentinels = None;
    while let Some(record) = dictionary_reader
        .read_record()
        .expect("read dictionary record")
    {
        if let DictionaryRecord::Extension(ExtensionRecord::FloatInfo(found)) = record {
            sentinels = Some(found);
        }
    }
    let sentinels = sentinels.expect("float sentinels");

    assert_eq!(sentinels, FloatSentinels::spss_defaults(encoding));
    assert_eq!(
        sentinels.system_missing_as_f64(encoding).to_bits(),
        (-f64::MAX).to_bits(),
    );
    assert_eq!(
        sentinels.highest_as_f64(encoding).to_bits(),
        f64::MAX.to_bits(),
    );
    assert_eq!(
        sentinels.lowest_as_f64(encoding).to_bits(),
        (-f64::MAX).next_up().to_bits(),
    );
}

#[test]
fn comprehensive_metadata_extensions() {
    let records = read_dictionary(COMPREHENSIVE);
    let extensions = extensions(&records);

    // Subtype 3 — machine integer info (UTF-8 character code 65001).
    let machine = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::MachineIntegerInfo(m) => Some(m),
            _ => None,
        })
        .expect("machine integer info");
    assert_eq!(machine.character_code(), 65001);
    assert_eq!(machine.endianness_kind(), Some(ByteOrder::LittleEndian));

    // Subtype 20 — declared character encoding.
    let encoding = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::CharacterEncoding(name) => Some(name.name()),
            _ => None,
        })
        .expect("character encoding");
    assert_eq!(encoding, "UTF-8");

    // Subtype 13 — long variable names.
    let long_names = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::LongVariableNames(n) => Some(n.mappings()),
            _ => None,
        })
        .expect("long variable names");
    let long: Vec<(&str, &str)> = long_names
        .iter()
        .map(|m| (m.short_name(), m.long_name()))
        .collect();
    assert!(long.contains(&("LONGSTR", "longstr")));
    assert!(long.contains(&("ID", "id")));

    // Subtype 14 — very-long-string widths.
    let very_long = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::VeryLongStrings(v) => Some(v.strings()),
            _ => None,
        })
        .expect("very long strings");
    assert_eq!(very_long.len(), 1);
    assert_eq!(very_long[0].short_name(), "LONGSTR");
    assert_eq!(very_long[0].width(), 300);
}

#[test]
fn comprehensive_attribute_and_long_string_extensions() {
    let records = read_dictionary(COMPREHENSIVE);
    let extensions = extensions(&records);

    // Subtype 21 — long string value labels.
    let long_labels = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::LongValueLabels(l) => Some(l.records()),
            _ => None,
        })
        .expect("long value labels");
    assert_eq!(long_labels.len(), 1);
    let first_long_label = &long_labels[0];
    assert_eq!(first_long_label.variable_name(), "longstr");
    assert_eq!(first_long_label.width(), 300);
    let first_long_label_labels = first_long_label.labels();
    assert_eq!(first_long_label_labels.len(), 2);
    assert_eq!(first_long_label_labels[0].label(), "First value");
    assert_eq!(first_long_label_labels[1].label(), "Second value");
    // The value bytes are the padded string key.
    assert!(first_long_label_labels[0].value().starts_with(b"alpha"));

    // Subtype 22 — long string missing values.
    let long_missing = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::LongMissingValues(l) => Some(l.records()),
            _ => None,
        })
        .expect("long missing values");
    assert_eq!(long_missing.len(), 1);
    let first_long_missing = &long_missing[0];
    assert_eq!(first_long_missing.variable_name(), "longstr");
    let first_long_missing_values = first_long_missing.values();
    assert_eq!(first_long_missing_values.len(), 1);
    assert!(first_long_missing_values[0].starts_with(b"alpha"));

    // Subtype 17 — file attributes.
    let file_attributes = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::FileAttributes(a) => Some(a.attributes()),
            _ => None,
        })
        .expect("file attributes");
    let owner = file_attributes
        .iter()
        .find(|a| a.name() == "Owner")
        .expect("Owner attribute");
    assert_eq!(owner.values(), ["Alice".to_string()]);
    let project = file_attributes
        .iter()
        .find(|a| a.name() == "Project")
        .expect("Project attribute");
    assert_eq!(project.values(), ["Census".to_string()]);

    // Subtype 18 — variable attributes (custom MyAttr on `id`).
    let variable_attributes = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::VariableAttributes(a) => Some(a.records()),
            _ => None,
        })
        .expect("variable attributes");
    let id_attrs = variable_attributes
        .iter()
        .find(|r| r.variable_name() == "id")
        .expect("attributes for id");
    let my_attr = id_attrs
        .attributes()
        .iter()
        .find(|a| a.name() == "MyAttr")
        .expect("MyAttr");
    assert_eq!(my_attr.values(), ["hello world".to_string()]);
}

#[test]
fn comprehensive_multiple_response_sets() {
    // PSPP writes the C/D groups to subtype 7 and the E group to
    // subtype 19; both surface as MultipleResponseSets records. Flatten
    // them and assert each set against the real bytes.
    let records = read_dictionary(COMPREHENSIVE);
    let extensions = extensions(&records);
    let sets: Vec<&MultipleResponseSet> = extensions
        .iter()
        .filter_map(|e| match e {
            ExtensionRecord::MultipleResponseSets(s) => Some(s.sets()),
            _ => None,
        })
        .flatten()
        .collect();
    assert_eq!(sets.len(), 3);

    let find = |name: &str| -> &MultipleResponseSet {
        sets.iter()
            .copied()
            .find(|s| s.name() == name)
            .unwrap_or_else(|| panic!("no multiple response set named {name}"))
    };

    // $dich — multiple dichotomy, counted value "1", category labels
    // from variable labels (wire type D). Names keep their leading `$`.
    let dich = find("$dich");
    assert_eq!(dich.label(), "Dichotomy set");
    assert_eq!(
        dich.variables(),
        ["q1".to_string(), "q2".to_string(), "q3".to_string()]
    );
    let MultipleResponseSetKind::MultipleDichotomy {
        counted_value,
        category_labels,
    } = dich.kind()
    else {
        panic!("expected dichotomy, got {:?}", dich.kind());
    };
    assert_eq!(counted_value.as_str(), "1");
    assert_eq!(*category_labels, CategoryLabelSource::VariableLabels);

    // $cat — multiple category (wire type C), no counted value.
    let cat = find("$cat");
    assert_eq!(cat.label(), "Category set");
    assert_eq!(cat.variables(), ["q1".to_string(), "q2".to_string()]);
    assert_eq!(*cat.kind(), MultipleResponseSetKind::MultipleCategory);

    // $counted — multiple dichotomy with category labels from counted
    // values (wire type E, subtype 19), an empty label, label source 1.
    let counted = find("$counted");
    assert_eq!(counted.label(), "");
    assert_eq!(counted.variables(), ["q2".to_string(), "q3".to_string()]);
    let MultipleResponseSetKind::MultipleDichotomy {
        counted_value,
        category_labels,
    } = counted.kind()
    else {
        panic!("expected dichotomy, got {:?}", counted.kind());
    };
    assert_eq!(counted_value.as_str(), "1");
    assert_eq!(
        *category_labels,
        CategoryLabelSource::CountedValues { label_source: 1 }
    );
}

/// The windows-1252 fixture declares an encoding that happens to match
/// the reader's hard-coded fallback, so its text already decodes
/// correctly. This locks in the agreeing path so that resolving the
/// encoding from the file's own declaration cannot regress it.
#[test]
fn encoding_windows1252_decodes_accented_text() {
    let (file_label, records) = read_encoding_fixture(ENCODING_WINDOWS_1252);
    assert_accented_text_decoded(&file_label, &records);
}

/// The UTF-8 fixture declares `"UTF-8"` in a subtype-20 record that
/// sits last in the dictionary, after every string it governs — and
/// disagrees with the reader's default fallback, so nothing here decodes
/// correctly unless the declaration is genuinely honored.
///
/// This is the acceptance test for deferred decoding: the header fields
/// and every dictionary record are held undecoded until the declaration
/// is reached and resolved. Before that landed, `Café crème` read back
/// as `CafÃ© crÃ¨me`.
#[test]
fn encoding_utf8_decodes_accented_text() {
    let (file_label, records) = read_encoding_fixture(ENCODING_UTF8);
    assert_accented_text_decoded(&file_label, &records);
}

/// Both fixtures declare their encoding in a subtype-20 record, so the
/// reader reports it as [`EncodingProvenance::Label`] rather than falling
/// back — and reports it before any record has been read, since
/// resolving it is what `read_header` walked the dictionary for.
#[test]
fn encoding_fixtures_report_a_label_provenance() {
    let cases = [
        (ENCODING_UTF8, encoding_rs::UTF_8),
        (ENCODING_WINDOWS_1252, encoding_rs::WINDOWS_1252),
    ];
    for (path, expected) in cases {
        let dictionary_reader = SavReader::new()
            .from_path(path)
            .expect("open fixture")
            .read_header()
            .expect("read header");
        assert_eq!(
            dictionary_reader.encoding_provenance(),
            EncodingProvenance::Label(expected),
            "{path}"
        );
    }
}

/// An override wins over the file's own declaration, and the mismatch
/// surfaces as a warning when the declaring subtype-20 record reaches
/// the caller — not during resolution, so it arrives alongside the
/// record it is about.
#[test]
fn override_wins_and_warns_when_the_file_disagrees() {
    let mut dictionary_reader = SavReader::new()
        .encoding_strategy(EncodingStrategy::Override(encoding_rs::WINDOWS_1252))
        .from_path(ENCODING_UTF8)
        .expect("open fixture")
        .read_header()
        .expect("read header");

    assert_eq!(
        dictionary_reader.encoding_provenance(),
        EncodingProvenance::Overridden(encoding_rs::WINDOWS_1252)
    );

    let mut overridden = Vec::new();
    while let Some(_record) = dictionary_reader
        .read_record()
        .expect("read dictionary record")
    {
        overridden.extend(
            dictionary_reader
                .warnings()
                .iter()
                .filter(|w| matches!(w, SavWarning::EncodingOverridden { .. }))
                .map(|w| format!("{w:?}")),
        );
    }
    assert_eq!(overridden.len(), 1, "warnings = {overridden:?}");
    assert!(
        overridden[0].contains("UTF-8") && overridden[0].contains("windows-1252"),
        "warning = {}",
        overridden[0]
    );
}

// ---------------------------------------------------------------------
// Schema finalization
//
// These read the dictionary through to the record reader and assert the
// reconciled schema, which is where the extension records stop being
// separate payloads and become properties of a variable.
// ---------------------------------------------------------------------

/// Opens the comprehensive fixture and finalizes straight through to the
/// record reader without pulling a single dictionary record by hand.
fn finalize_comprehensive() -> spss_sav::spss::sav::record_reader::RecordReader<BufReader<File>> {
    SavReader::new()
        .from_path(COMPREHENSIVE)
        .expect("open fixture")
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize")
}

/// The fixture declares `longstr (A300)`, which PSPP stores as two
/// segments — `LONGSTR` at width 255 and `LONGST_A` at width 48. The
/// schema must show one variable at width 300.
#[test]
fn very_long_strings_collapse_into_one_variable() {
    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema built by default");

    let names: Vec<&str> = schema
        .variables()
        .iter()
        .map(spss_sav::spss::sav::sav_variable::SavVariable::full_name)
        .collect();
    assert_eq!(names, ["id", "q1", "q2", "q3", "longstr", "shortstr"]);

    let longstr = schema.variable_by_name("longstr").expect("longstr present");
    assert_eq!(longstr.variable_type(), VariableType::String(300));
    assert_eq!(longstr.index(), 4);
    assert_eq!(longstr.label(), Some("A long string variable"));
}

/// Subtype 13 maps short names to long ones, keyed by the short name.
#[test]
fn long_names_are_patched_onto_variables() {
    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema");
    let id = schema.variable_by_name("id").expect("id present");
    assert_eq!(id.short_name(), "ID");
    assert_eq!(id.long_name(), Some("id"));
    assert_eq!(id.full_name(), "id");
}

/// A type-3 / type-4 pair covering q1, q2 and q3 becomes one shared set.
#[test]
fn value_labels_attach_to_every_variable_the_pair_named() {
    use spss_sav::spss::sav::value_label_value::ValueLabelValue;

    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema");
    for name in ["q1", "q2", "q3"] {
        let variable = schema.variable_by_name(name).expect("variable present");
        let labels = variable.value_labels().expect("labels attached");
        assert_eq!(labels.len(), 2);
        assert_eq!(
            variable.label_for(&ValueLabelValue::Numeric(0.0)),
            Some("No"),
            "variable {name}",
        );
        assert_eq!(
            variable.label_for(&ValueLabelValue::Numeric(1.0)),
            Some("Yes"),
            "variable {name}",
        );
    }
    // A variable the pair did not name has none.
    assert!(
        schema
            .variable_by_name("id")
            .expect("id present")
            .value_labels()
            .is_none()
    );
}

/// Subtype 21 carries full-width keys for a very long string.
#[test]
fn long_value_labels_attach_with_full_width_keys() {
    use spss_sav::spss::sav::value_label_value::ValueLabelValue;

    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema");
    let longstr = schema.variable_by_name("longstr").expect("longstr present");
    let labels = longstr.value_labels().expect("labels attached");
    assert_eq!(labels.len(), 2);

    let mut key = vec![b' '; 300];
    key[..5].copy_from_slice(b"alpha");
    assert_eq!(
        longstr.label_for(&ValueLabelValue::LongString(key.into_boxed_slice())),
        Some("First value"),
    );
}

/// `MISSING VALUES id (99)` is numeric; `MISSING VALUES longstr
/// ('alpha')` goes through subtype 22 and stays raw bytes.
#[test]
fn missing_values_decode_per_variable_type() {
    use spss_sav::spss::sav::missing_value_specification::MissingValueSpecification;

    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema");

    let id = schema.variable_by_name("id").expect("id present");
    assert_eq!(
        id.missing_value_spec(),
        &MissingValueSpecification::Discrete(vec![99.0]),
    );

    let longstr = schema.variable_by_name("longstr").expect("longstr present");
    let MissingValueSpecification::String(values) = longstr.missing_value_spec() else {
        panic!(
            "expected string missing values, got {:?}",
            longstr.missing_value_spec()
        );
    };
    assert_eq!(values.len(), 1);
    assert_eq!(&*values[0], b"alpha   ");
}

/// Subtype 11 writes a tuple per *segment*; the leading segment's is the
/// one that describes the collapsed variable.
#[test]
fn display_parameters_slice_per_segment() {
    use spss_sav::spss::sav::alignment::Alignment;
    use spss_sav::spss::sav::measurement_level::MeasurementLevel;

    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema");

    let id = schema.variable_by_name("id").expect("id present");
    let display = id.display().expect("display attached");
    assert_eq!(display.measurement_level(), MeasurementLevel::Nominal);
    assert_eq!(display.display_width(), Some(8));
    assert_eq!(display.alignment(), Alignment::Right);

    // The very long string's tuple is the first segment's: (1, 32, 0).
    let longstr = schema.variable_by_name("longstr").expect("longstr present");
    let display = longstr.display().expect("display attached");
    assert_eq!(display.display_width(), Some(32));
    assert_eq!(display.alignment(), Alignment::Left);
}

/// Subtype 18 keys off the long name, and `$@Role` must survive the
/// `[n]` collapse untouched.
#[test]
fn variable_attributes_attach_by_long_name() {
    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema");
    let id = schema.variable_by_name("id").expect("id present");

    let attribute = id.attribute("MyAttr").expect("MyAttr present");
    assert_eq!(attribute.value(), Some("hello world"));
    assert!(id.attribute("$@Role").is_some(), "{:?}", id.attributes());
}

/// The header declares the case count; the fixture has two rows.
#[test]
fn case_count_comes_through_finalization() {
    let reader = finalize_comprehensive();
    assert_eq!(reader.case_count(), Some(2));
}

/// `build_schema(false)` drops the schema but must not disturb anything
/// the record reader needs.
#[test]
fn schema_building_can_be_turned_off() {
    let reader = SavReader::new()
        .build_schema(false)
        .from_path(COMPREHENSIVE)
        .expect("open fixture")
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize");
    assert!(reader.schema().is_none());
    assert_eq!(reader.case_count(), Some(2));
}

/// The load-bearing invariant: skipping every skippable record must not
/// change the layout the rows are read through. Subtype 14 is what
/// collapses the very long string, and skipping it must not un-collapse
/// anything the data reader depends on.
#[test]
fn skipping_everything_skippable_leaves_the_data_layout_intact() {
    use spss_sav::spss::sav::extensions::extension_subtype::ExtensionSubtype;
    use spss_sav::spss::sav::skippable_content::SkippableContent;

    let mut reader = SavReader::new().skip_dictionary_content(SkippableContent::Documents);
    reader = reader.skip_dictionary_content(SkippableContent::ValueLabels);
    for subtype in [
        ExtensionSubtype::MachineIntegerInfo,
        ExtensionSubtype::FloatInfo,
        ExtensionSubtype::VariableSets,
        ExtensionSubtype::MultipleResponseSets,
        ExtensionSubtype::MultipleResponseSetsExtended,
        ExtensionSubtype::DisplayParameters,
        ExtensionSubtype::LongVariableNames,
        ExtensionSubtype::VeryLongStrings,
        ExtensionSubtype::ExtendedNumberOfCases,
        ExtensionSubtype::FileAttributes,
        ExtensionSubtype::VariableAttributes,
        ExtensionSubtype::CharacterEncoding,
        ExtensionSubtype::LongValueLabels,
        ExtensionSubtype::LongMissingValues,
        ExtensionSubtype::Unrecognized,
    ] {
        reader = reader.skip_dictionary_content(SkippableContent::Extension(subtype));
    }

    let stripped = reader
        .from_path(COMPREHENSIVE)
        .expect("open fixture")
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize");

    // Encoding still resolves from the records that were "skipped".
    assert_eq!(
        stripped.encoding_provenance(),
        finalize_comprehensive().encoding_provenance(),
    );
    // And the very long string is still one variable at width 300.
    let schema = stripped.schema().expect("schema");
    let longstr = schema
        .variables()
        .iter()
        .find(|v| v.short_name() == "LONGSTR")
        .expect("longstr present");
    assert_eq!(longstr.variable_type(), VariableType::String(300));
    assert_eq!(schema.variable_count(), 6);
    assert_eq!(stripped.case_count(), Some(2));
}

/// The header declares the weight as an offset into the data row, which
/// in this fixture is 40 (1-based) because the `A300` variable ahead of
/// it occupies 38 units across two segments. Resolving that to a
/// variable crosses offset → segment → variable; conflating any two of
/// those spaces picks the wrong variable or none at all.
#[test]
fn weight_variable_resolves_across_all_three_index_spaces() {
    let reader = SavReader::new()
        .from_path(WEIGHTED)
        .expect("open fixture")
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize");

    let schema = reader.schema().expect("schema");
    let names: Vec<&str> = schema
        .variables()
        .iter()
        .map(spss_sav::spss::sav::sav_variable::SavVariable::full_name)
        .collect();
    assert_eq!(names, ["id", "descr", "wgt"]);

    let weight = schema.weight_variable().expect("weight declared");
    assert_eq!(weight.full_name(), "wgt");
    assert_eq!(weight.index(), 2);
}

/// A file that declares no weight reports none, rather than defaulting
/// to the first variable.
#[test]
fn a_file_without_a_weight_reports_none() {
    let reader = finalize_comprehensive();
    let schema = reader.schema().expect("schema");
    assert!(schema.weight_variable().is_none());
}

/// The weight lives on the schema, so turning schema building off takes
/// it with them — there is nothing left to resolve the offset against.
#[test]
fn no_schema_means_no_weight_variable() {
    let reader = SavReader::new()
        .build_schema(false)
        .from_path(WEIGHTED)
        .expect("open fixture")
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize");
    assert!(reader.schema().is_none());
}

/// The strongest form of the rule: on a real file, passing over every
/// record must produce exactly the schema that reading every record
/// does. `skip_record` withholds records from the caller; it does not
/// withhold them from the library.
#[test]
fn skipping_every_record_produces_the_same_schema_as_reading_them() {
    fn describe(schema: &spss_sav::spss::sav::sav_schema::SavSchema) -> Vec<String> {
        schema
            .variables()
            .iter()
            .map(|variable| {
                format!(
                    "{}|{:?}|{:?}|{}|{:?}",
                    variable.full_name(),
                    variable.variable_type(),
                    variable.missing_value_spec(),
                    variable
                        .value_labels()
                        .map_or(0, spss_sav::spss::sav::value_label_set::ValueLabelSet::len),
                    variable.display(),
                )
            })
            .collect()
    }

    let mut read_all = SavReader::new()
        .from_path(COMPREHENSIVE)
        .expect("open fixture")
        .read_header()
        .expect("read header");
    while read_all.read_record().expect("read record").is_some() {}
    let read_all = read_all.into_record_reader().expect("finalize");

    let mut skipped = SavReader::new()
        .from_path(COMPREHENSIVE)
        .expect("open fixture")
        .read_header()
        .expect("read header");
    while skipped.skip_record().expect("skip record").is_some() {}
    let skipped = skipped.into_record_reader().expect("finalize");

    assert_eq!(
        describe(skipped.schema().expect("schema")),
        describe(read_all.schema().expect("schema")),
    );
    assert_eq!(skipped.case_count(), read_all.case_count());
    assert_eq!(
        skipped
            .schema()
            .and_then(spss_sav::spss::sav::sav_schema::SavSchema::weight_variable)
            .map(spss_sav::spss::sav::sav_variable::SavVariable::full_name),
        read_all
            .schema()
            .and_then(spss_sav::spss::sav::sav_schema::SavSchema::weight_variable)
            .map(spss_sav::spss::sav::sav_variable::SavVariable::full_name),
    );
}

// ---------------------------------------------------------------------
// Data records — Uncompressed
//
// `compression.sps` writes the same three rows under all three
// compression schemes so one expected row set can be asserted against
// every one of them. Only the uncompressed file is read here; the
// bytecode and ZSAV fixtures join this suite as 6(b) and 6(c) land.
// ---------------------------------------------------------------------

/// The rows `compression.sps` writes, rendered by `describe_row`.
///
/// Note `blank`: row 1 declares eight spaces and row 2 a single space,
/// and both come back empty — SAV pads every string cell to its
/// declared width, so a trailing space is indistinguishable from fill.
const COMPRESSION_ROWS: [[&str; 8]; 3] = [
    [
        "1",
        "0",
        "1000000",
        "1.5",
        ".",
        "\"\"",
        "\"aa\"",
        "\"alpha\"",
    ],
    [
        "2",
        // MISSING VALUES small (151) -- a discrete numeric value.
        "user(151)",
        // MISSING VALUES big (LOWEST THRU 0) -- an open-ended range.
        "user(-999999)",
        "-0.25",
        ".",
        "\"\"",
        "\"bb\"",
        // MISSING VALUES longstr ('beta') -- reaches us through
        // extension subtype 22, keyed by the LONG name.
        "missing(\"beta\")",
    ],
    [
        "3",
        "-100",
        // The range is inclusive at its closed end, so 0 is missing.
        "user(0)",
        "0",
        ".",
        "\"xy\"",
        // MISSING VALUES text ('cc') -- a short-string value, whose key
        // PSPP NUL-pads to eight bytes.
        "missing(\"cc\")",
        "\"gamma\"",
    ],
];

fn record_reader(path: &str) -> RecordReader<BufReader<File>> {
    SavReader::new()
        .from_path(path)
        .expect("open fixture")
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize")
}

/// Renders one cell so a row reads as a single line of expectations.
fn describe_value(value: &Value<'_>) -> String {
    match value {
        Value::Numeric(Numeric::Present(number)) => format!("{number}"),
        Value::Numeric(Numeric::Missing(MissingValue::System)) => ".".to_owned(),
        Value::Numeric(Numeric::Missing(MissingValue::UserDefined(number))) => {
            format!("user({number})")
        }
        Value::String(Text::Present(string)) => format!("{:?}", string.text()),
        Value::String(Text::Missing(string)) => format!("missing({:?})", string.text()),
    }
}

fn describe_row(record: &SavRecord<'_>) -> Vec<String> {
    record.values().iter().map(describe_value).collect()
}

#[test]
fn uncompressed_rows_decode_to_their_written_values() {
    let mut reader = record_reader(COMPRESSION_NONE);
    assert_eq!(reader.case_count(), Some(3));

    let mut rows = Vec::new();
    while let Some(record) = reader.read_record().expect("read record") {
        rows.push(describe_row(&record));
    }
    assert_eq!(rows.len(), 3);
    for (row, expected) in rows.iter().zip(COMPRESSION_ROWS) {
        assert_eq!(row.as_slice(), expected.as_slice());
    }
}

/// The `A300` is stored as a 255-wide segment plus a 48-wide one, so the
/// eighth cell only reads correctly if reassembly skips the padding byte
/// between them. `describe_row` above already covers the value; this
/// pins the shape it came from.
#[test]
fn a_very_long_string_cell_reassembles_from_both_segments() {
    let mut reader = record_reader(COMPRESSION_NONE);
    let schema = reader.schema().expect("schema").clone();
    let longstr = schema.variable_by_name("longstr").expect("longstr");
    assert_eq!(longstr.variable_type(), VariableType::String(300));
    assert_eq!(longstr.index(), 7);

    let record = reader.read_record().expect("read").expect("a row");
    let cell = record.values()[longstr.index()]
        .text()
        .expect("a string cell");
    assert_eq!(cell.value().text(), "alpha");
    assert!(!cell.is_missing(), "row 1's longstr is 'alpha', not 'beta'");
}

/// Reading past the last row reports the end rather than erroring: PSPP
/// writes no end-of-data marker, so a clean EOF on a row boundary is the
/// only signal there is.
#[test]
fn reading_past_the_last_row_reports_the_end() {
    let mut reader = record_reader(COMPRESSION_NONE);
    for _ in 0..3 {
        assert!(reader.read_record().expect("read").is_some());
    }
    assert!(reader.read_record().expect("read past end").is_none());
    assert!(reader.read_record().expect("read past end twice").is_none());
}

/// The lazy reader must agree with the eager one cell for cell, and
/// report nothing past the last variable.
#[test]
fn lazy_records_agree_with_eager_ones() {
    let mut eager = record_reader(COMPRESSION_NONE);
    let mut lazy = record_reader(COMPRESSION_NONE);
    for expected in COMPRESSION_ROWS {
        let eager_row = describe_row(&eager.read_record().expect("read").expect("a row"));
        let record = lazy.read_lazy_record().expect("read").expect("a row");
        assert_eq!(record.len(), expected.len());
        assert!(!record.is_empty());
        let lazy_row: Vec<String> = (0..record.len())
            .map(|index| describe_value(&record.value(index).expect("a cell")))
            .collect();
        assert_eq!(lazy_row, eager_row);
        assert_eq!(lazy_row.as_slice(), expected.as_slice());
        assert!(record.value(record.len()).is_none(), "past the last cell");
    }
}

/// Skipping advances the same way reading does, so a caller can mix the
/// two and land on the rows it wanted.
#[test]
fn skipping_rows_advances_without_decoding_them() {
    let mut reader = record_reader(COMPRESSION_NONE);
    assert!(reader.skip_record().expect("skip"));
    assert!(reader.skip_record().expect("skip"));
    let record = reader.read_record().expect("read").expect("third row");
    assert_eq!(
        describe_row(&record).as_slice(),
        COMPRESSION_ROWS[2].as_slice()
    );
    assert!(!reader.skip_record().expect("skip past end"));
}

/// The reason `StringValue` keeps its bytes: a variable's value-label
/// and missing-value keys are raw, so matching a cell against them has
/// to be byte-for-byte rather than through decoded text.
#[test]
fn string_cells_expose_the_raw_bytes_behind_the_text() {
    let mut reader = record_reader(COMPRESSION_NONE);
    let record = reader.read_record().expect("read").expect("a row");
    let text = record.values()[6].text().expect("the text cell");
    assert_eq!(text.value().raw(), b"aa", "padding is already trimmed");
    assert_eq!(text.value().text(), "aa");
}

/// Bytes in one uncompressed row of the compression fixture: five
/// numerics, an `A8` and an `A4` at one unit each, and the `A300` as a
/// 256-byte segment plus a 48-byte one.
const COMPRESSION_ROW_LEN: usize = 5 * 8 + 8 + 8 + 256 + 48;

fn reader_over(bytes: Vec<u8>) -> RecordReader<Cursor<Vec<u8>>> {
    SavReader::new()
        .from_reader(Cursor::new(bytes))
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize")
}

/// A count that matches says nothing.
#[test]
fn a_row_count_matching_the_declaration_is_silent() {
    let mut reader = record_reader(COMPRESSION_NONE);
    while reader.read_record().expect("read").is_some() {}
    assert!(reader.warnings().is_empty(), "{:?}", reader.warnings());
}

/// A file holding fewer rows than it claims still hands back the rows it
/// does have — the shortfall is a warning, not a failure. `ReadStat`
/// errors on this; PSPP warns, and so do we.
#[test]
fn fewer_rows_than_declared_warns_once_the_data_ends() {
    let mut bytes = std::fs::read(COMPRESSION_NONE).expect("read fixture");
    bytes.truncate(bytes.len() - COMPRESSION_ROW_LEN);

    let mut reader = reader_over(bytes);
    assert_eq!(reader.case_count(), Some(3));
    let mut rows = 0;
    while reader.read_record().expect("read").is_some() {
        rows += 1;
    }
    assert_eq!(rows, 2, "the two whole rows still read");
    assert!(
        matches!(
            reader.warnings(),
            [SavWarning::RowCountMismatch {
                declared: 3,
                actual: 2,
            }],
        ),
        "{:?}",
        reader.warnings(),
    );
}

/// Stopping *partway through* a row is a different thing from stopping
/// on a boundary, and has to be an error rather than a quiet end.
#[test]
fn a_row_cut_in_half_is_a_truncation_error() {
    let mut bytes = std::fs::read(COMPRESSION_NONE).expect("read fixture");
    bytes.truncate(bytes.len() - COMPRESSION_ROW_LEN / 2);

    let mut reader = reader_over(bytes);
    assert!(reader.read_record().expect("first row").is_some());
    assert!(reader.read_record().expect("second row").is_some());
    let error = reader.read_record().expect_err("a partial row must error");
    match error {
        SavError::Format(format) => {
            assert_eq!(format.section(), Section::Records);
            assert_eq!(
                format.kind(),
                FormatErrorKind::Truncated {
                    expected: COMPRESSION_ROW_LEN as u64,
                    actual: (COMPRESSION_ROW_LEN / 2) as u64,
                },
            );
        }
        other => panic!("expected a format error, got {other:?}"),
    }
}

/// The reason the missing-value specs live on `DataLayout` rather than
/// only on `SavSchema`: with schema building switched off there is no
/// `SavVariable` to consult, and a row that reported a declared-missing
/// cell as present would be lying in a way the caller could not detect.
#[test]
fn missing_values_are_tagged_even_with_no_schema() {
    let mut reader = SavReader::new()
        .build_schema(false)
        .from_path(COMPRESSION_NONE)
        .expect("open fixture")
        .read_header()
        .expect("read header")
        .into_record_reader()
        .expect("finalize");
    assert!(reader.schema().is_none(), "schema building is off");

    let mut rows = Vec::new();
    while let Some(record) = reader.read_record().expect("read record") {
        rows.push(describe_row(&record));
    }
    for (row, expected) in rows.iter().zip(COMPRESSION_ROWS) {
        assert_eq!(row.as_slice(), expected.as_slice());
    }
}

/// The schema and the row reader derive missing values *independently*
/// — `SavSchemaBuilder` from the type-2 records plus subtype 22 keyed
/// through its own name index, `DataLayoutBuilder` from the skeleton
/// plus subtype 22 keyed through subtype 13. They agree on every
/// well-formed file, and this is what keeps them from drifting.
#[test]
fn the_schema_and_the_row_reader_agree_on_what_is_missing() {
    let mut reader = record_reader(COMPRESSION_NONE);
    let schema = reader.schema().expect("schema").clone();
    let mut checked = 0;
    while let Some(record) = reader.read_record().expect("read record") {
        for (index, value) in record.values().iter().enumerate() {
            let variable = &schema.variables()[index];
            let spec = variable.missing_value_spec();
            let by_schema = match value {
                // System-missing is a property of the cell, so no
                // declaration can rule it out.
                Value::Numeric(Numeric::Missing(MissingValue::System)) => true,
                Value::Numeric(
                    Numeric::Missing(MissingValue::UserDefined(number)) | Numeric::Present(number),
                ) => spec.matches_number(*number),
                Value::String(text) => spec.matches_bytes(text.value().raw()),
            };
            assert_eq!(
                by_schema,
                value.is_missing(),
                "variable {} disagrees on {value:?}",
                variable.full_name(),
            );
            checked += 1;
        }
    }
    assert_eq!(checked, 24, "three rows of eight variables");
}
