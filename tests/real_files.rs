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

use spss_sav::spss::sav::byte_order::ByteOrder;
use spss_sav::spss::sav::compression::Compression;
use spss_sav::spss::sav::dictionary_record::DictionaryRecord;
use spss_sav::spss::sav::document_record::DocumentRecord;
use spss_sav::spss::sav::extensions::extension_record::ExtensionRecord;
use spss_sav::spss::sav::float_format::FloatFormat;
use spss_sav::spss::sav::raw_missing_values::RawMissingValues;
use spss_sav::spss::sav::raw_value_label_set::RawValueLabelSet;
use spss_sav::spss::sav::sav_reader::SavReader;
use spss_sav::spss::sav::sav_variable_header::SavVariableHeader;
use spss_sav::spss::sav::variable_type::VariableType;

const COMPREHENSIVE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/comprehensive.sav"
);

/// Reads every dictionary record from `path`, asserting the header
/// along the way, and returns the records.
fn read_dictionary(path: &str) -> Vec<DictionaryRecord> {
    let header_reader = SavReader::new().from_path(path).expect("open fixture");
    let mut dict = header_reader.read_header().expect("read header");

    {
        let header = dict.header();
        assert!(
            header.product_name().contains("pspp"),
            "product_name = {:?}",
            header.product_name()
        );
        assert_eq!(header.compression(), Compression::Bytecode);
        assert_eq!(header.byte_order(), ByteOrder::LittleEndian);
        assert_eq!(header.float_format(), FloatFormat::Ieee754);
        assert_eq!(header.case_count(), Some(2));
    }
    assert!(
        dict.warnings().is_empty(),
        "unexpected header warnings: {:?}",
        dict.warnings()
    );

    let mut records = Vec::new();
    while let Some(record) = dict.read_record().expect("read dictionary record") {
        records.push(record);
    }
    records
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
    assert!(
        documents
            .iter()
            .any(|d| d.lines().iter().any(|l| l.contains("documentary line for testing"))),
        "documents = {documents:?}"
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
            ExtensionRecord::CharacterEncoding(name) => Some(name.as_str()),
            _ => None,
        })
        .expect("character encoding");
    assert_eq!(encoding, "UTF-8");

    // Subtype 13 — long variable names.
    let long_names = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::LongVariableNames(n) => Some(n),
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
            ExtensionRecord::VeryLongStrings(v) => Some(v),
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
            ExtensionRecord::LongValueLabels(l) => Some(l),
            _ => None,
        })
        .expect("long value labels");
    assert_eq!(long_labels.len(), 1);
    assert_eq!(long_labels[0].variable_name(), "longstr");
    assert_eq!(long_labels[0].width(), 300);
    assert_eq!(long_labels[0].labels().len(), 2);
    assert_eq!(long_labels[0].labels()[0].label(), "First value");
    assert_eq!(long_labels[0].labels()[1].label(), "Second value");
    // The value bytes are the padded string key.
    assert!(long_labels[0].labels()[0].value().starts_with(b"alpha"));

    // Subtype 22 — long string missing values.
    let long_missing = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::LongMissingValues(l) => Some(l),
            _ => None,
        })
        .expect("long missing values");
    assert_eq!(long_missing.len(), 1);
    assert_eq!(long_missing[0].variable_name(), "longstr");
    assert_eq!(long_missing[0].values().len(), 1);
    assert!(long_missing[0].values()[0].starts_with(b"alpha"));

    // Subtype 17 — file attributes.
    let file_attributes = extensions
        .iter()
        .find_map(|e| match e {
            ExtensionRecord::FileAttributes(a) => Some(a),
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
            ExtensionRecord::VariableAttributes(a) => Some(a),
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
fn comprehensive_multiple_response_sets_are_still_unparsed() {
    // Subtypes 7 and 19 (multiple response sets) are not yet wired, so
    // they surface as Unknown. This test captures the ground-truth raw
    // payloads PSPP wrote; when 7/19 land, replace these assertions
    // with typed ones.
    let records = read_dictionary(COMPREHENSIVE);
    let extensions = extensions(&records);

    let unknown_payload = |subtype: u32| -> Vec<u8> {
        extensions
            .iter()
            .find_map(|e| match e {
                ExtensionRecord::Unknown(u) if u.subtype() == subtype => Some(u.payload().to_vec()),
                _ => None,
            })
            .unwrap_or_else(|| panic!("expected Unknown extension subtype {subtype}"))
    };

    // Subtype 7 — C (category) and D (dichotomy/VARLABELS) groups.
    let sets_7 = String::from_utf8(unknown_payload(7)).unwrap();
    assert_eq!(
        sets_7,
        "$dich=D1 1 13 Dichotomy set q1 q2 q3\n$cat=C 12 Category set q1 q2\n"
    );

    // Subtype 19 — E (dichotomy/COUNTEDVALUES) group with the 1/11
    // label-source prefix.
    let sets_19 = String::from_utf8(unknown_payload(19)).unwrap();
    assert_eq!(sets_19, "$counted=E 1 1 1 0  q2 q3\n");
}
