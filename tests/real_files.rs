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
use std::io::BufReader;

use spss_sav::spss::sav::byte_order::ByteOrder;
use spss_sav::spss::sav::compression::Compression;
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
use spss_sav::spss::sav::sav_reader::SavReader;
use spss_sav::spss::sav::sav_variable_header::SavVariableHeader;
use spss_sav::spss::sav::sav_warning::SavWarning;
use spss_sav::spss::sav::variable_type::VariableType;

const COMPREHENSIVE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/comprehensive.sav"
);

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
    assert_eq!(header.compression(), Compression::Bytecode);
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
    assert_eq!(set.variable_indices(), [1, 2, 3]);
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
