//! Reading every value out of a SAV file — the shape a data frame
//! library wants.
//!
//! Higher-level readers built on lower-level SAV libraries all want
//! the same narrow thing from a SAV file: the variable names, and
//! then every cell as an `f64` or a string. This example is that reader,
//! and nothing more.
//!
//! Three points it exists to make:
//!
//! 1. **None of the dictionary needs retaining.** Every category is
//!    skipped — see [`UNNEEDED`] — and the reader still names its
//!    columns and reads its rows correctly, because everything a correct
//!    read depends on is absorbed before any skip decision is made. See
//!    [`SkippableContent`](spss_sav::spss::sav::skippable_content::SkippableContent).
//! 2. **The row loop allocates nothing per cell.** String cells borrow
//!    the reader's row buffer, and a cell already valid in the file's
//!    encoding decodes to a borrow as well, so a UTF-8 file costs zero
//!    allocations per row beyond the record's own `Vec` of cells.
//! 3. **Missing values are already classified**, against the variable's
//!    declared missing values, by the time a cell is handed over. A
//!    consumer that renders missing as NaN and `""` needs one call per
//!    arm and no knowledge of what the dictionary declared.
//!
//! Nothing is printed per row, so what the loop costs is the read and
//! the conversion rather than formatting. The converted values go to
//! [`std::hint::black_box`], which stands in for whatever a real
//! consumer would do with them and keeps the compiler from discarding
//! the conversion as dead. Only a one-line summary and any warnings
//! reach stderr.
//!
//! Run it with:
//!
//! ```text
//! cargo run --example read_values -- tests/fixtures/compression_bytecode.sav
//! ```
//!
//! With no argument it reads a bundled fixture whose columns cover every
//! missing-value shape the format allows — system-missing, a discrete
//! numeric value, a numeric range, a short string and a very long
//! string.

use std::borrow::Cow;
use std::env;
use std::error::Error;
use std::hint::black_box;

use spss_sav::spss::sav::extensions::extension_subtype::ExtensionSubtype;
use spss_sav::spss::sav::sav_reader::SavReader;
use spss_sav::spss::sav::skippable_content::SkippableContent;
use spss_sav::spss::sav::string_value::StringValue;
use spss_sav::spss::sav::value::Value;

/// Read when no path is given, so the example runs from a bare
/// `cargo run --example read_values`.
const DEFAULT_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/compression_bytecode.sav"
);

/// Every category of dictionary content there is — a value reader needs
/// none of it retained. Stated outright because there is deliberately no
/// "skip everything" shorthand.
///
/// Skipping all of it still leaves the reader able to name its columns
/// and read its rows, which is worth being precise about. What a skip
/// costs is that record's *contribution to the schema*, since the schema
/// is accumulated from records as they are handed out and a skipped
/// record is never handed out. Three records lose something that way —
/// subtypes 11, 18 and 21, plus the value-label sets — and this consumer
/// reads none of it.
///
/// The two the schema would miss most, subtypes 13 (long variable names)
/// and 22 (a very long string's missing values), do not go through that
/// path at all. They are absorbed into the reader's layout skeleton
/// before any skip decision, and the schema takes them from there, so
/// `full_name()` still reports `household_income` rather than the
/// eight-byte `HOUSEHOL`.
///
/// The read itself is untouched no matter what: the layout-bearing
/// subtypes (3, 4, 13, 14, 16, 20 and 22) are absorbed regardless, so
/// row count, widths, encoding and missing tagging come out identical.
const UNNEEDED: [SkippableContent; 19] = [
    SkippableContent::ValueLabels,
    SkippableContent::Documents,
    SkippableContent::Extension(ExtensionSubtype::MachineIntegerInfo),
    SkippableContent::Extension(ExtensionSubtype::FloatInfo),
    SkippableContent::Extension(ExtensionSubtype::VariableSets),
    SkippableContent::Extension(ExtensionSubtype::MultipleResponseSets),
    SkippableContent::Extension(ExtensionSubtype::ExtraProductInfo),
    SkippableContent::Extension(ExtensionSubtype::DisplayParameters),
    SkippableContent::Extension(ExtensionSubtype::Uuid),
    SkippableContent::Extension(ExtensionSubtype::LongVariableNames),
    SkippableContent::Extension(ExtensionSubtype::VeryLongStrings),
    SkippableContent::Extension(ExtensionSubtype::ExtendedNumberOfCases),
    SkippableContent::Extension(ExtensionSubtype::FileAttributes),
    SkippableContent::Extension(ExtensionSubtype::VariableAttributes),
    SkippableContent::Extension(ExtensionSubtype::MultipleResponseSetsExtended),
    SkippableContent::Extension(ExtensionSubtype::CharacterEncoding),
    SkippableContent::Extension(ExtensionSubtype::LongValueLabels),
    SkippableContent::Extension(ExtensionSubtype::LongMissingValues),
    SkippableContent::Extension(ExtensionSubtype::Unrecognized),
];

fn main() -> Result<(), Box<dyn Error>> {
    let path = env::args()
        .nth(1)
        .unwrap_or_else(|| DEFAULT_PATH.to_owned());

    // Open, walk the dictionary, and land in the data section. Skipping
    // is a memory win rather than an I/O one — the bytes are still read,
    // they are just discarded instead of retained and decoded, which is
    // what keeps a dictionary carrying megabytes of value labels from
    // being held in memory whole.
    let mut reader = UNNEEDED
        .into_iter()
        .fold(SavReader::new(), SavReader::skip_dictionary_content)
        .from_path(&path)?
        .read_header()?
        .into_record_reader()?;

    // Snapshot what the loop needs before the first read: `schema()`
    // borrows the reader, `read_record` takes it mutably, so the two
    // cannot overlap. Names are all this consumer wants; a columnar one
    // would also take `variable_type()` here to pick its column
    // storage, and `reader.case_count()` to size it.
    let names: Vec<String> = reader
        .schema()
        .variables()
        .iter()
        .map(|variable| variable.full_name().to_owned())
        .collect();

    let mut rows: u64 = 0;
    while let Some(record) = reader.read_record()? {
        let cells = record.values();
        // One cell per variable the schema reports, always: a very long
        // string counts once here however many segments hold it on disk.
        assert_eq!(cells.len(), names.len(), "one cell per schema variable");

        // Every cell of the row is decoded and classified at this point.
        // This is where your record processing would go — building a
        // column vector, filling a data frame row, inserting into a
        // database.
        for cell in cells {
            match cell {
                // A missing number becomes NaN, whether it was
                // system-missing or a value the variable declared
                // missing. `present()` is the whole of the check.
                Value::Numeric(number) => {
                    black_box(number.present().unwrap_or(f64::NAN));
                }
                // A missing string becomes empty. There is no
                // system-missing for strings, so this only ever hides a
                // value the variable declared missing — reach it with
                // `text.value()` if you would rather keep it.
                //
                // Note what the convention costs: an all-blank cell is
                // present and renders identically. Call
                // `cell.is_missing()` where the difference matters.
                Value::String(text) => {
                    black_box(text.present().map_or(Cow::Borrowed(""), StringValue::text));
                }
            }
        }
        rows += 1;
    }

    eprintln!("{rows} rows x {} variables from {path}", names.len());
    Ok(())
}
