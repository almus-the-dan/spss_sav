# spss_sav

A pure Rust library for reading SPSS data files.

This crate covers the **SAV/ZSAV** binary format — the way [SPSS](https://www.ibm.com/products/spss-statistics) persists a dataset, and the format most survey research, public health, and government statistics work is distributed in. A `.sav` file carries far more than a table: variable and value labels, declared missing values, display metadata, multiple response sets, and free-text documentation all travel with the data, and losing them on import loses the meaning of the columns.

Both spellings are handled through the same API. `.zsav` is the same dictionary and the same rows inside a zlib container, so nothing above the row source needs to know which one it opened.

The reader is built around a typestate chain: each phase consumes the previous one and hands back the next, so a data row cannot be decoded before the dictionary that describes it has been parsed. There is no way to hold a reader in a state where the next call would be wrong.

> **Status.** The reader is complete and covered end to end — header, dictionary, and data records, under all three compression schemes. The writer is not implemented yet; see [Planned work](#planned-work).

## Reading values

The common case, and what most callers want: the column names, then every cell.

```rust
use spss_sav::spss::sav::sav_error::Result;
use spss_sav::spss::sav::sav_reader_builder::SavReaderBuilder;
use spss_sav::spss::sav::value::Value;

fn read_values(path: &str) -> Result<()> {
    // Straight to the rows. No dictionary record is retained, which for a
    // file carrying megabytes of value labels is the difference that
    // matters.
    let mut reader = SavReaderBuilder::new()
        .from_path(path)?
        .into_record_reader()?;

    // Capture the column names before the loop: `schema()` borrows the
    // reader and `read_record` takes it mutably, so the two cannot overlap.
    let names: Vec<String> = reader
        .schema()
        .variables()
        .iter()
        .map(|variable| variable.full_name().to_owned())
        .collect();

    while let Some(record) = reader.read_record()? {
        for (name, cell) in names.iter().zip(record.values()) {
            match cell {
                Value::Numeric(number) => match number.present() {
                    Some(value) => println!("{name}: {value}"),
                    None => println!("{name}: (missing)"),
                },
                Value::String(text) => match text.present() {
                    Some(value) => println!("{name}: {}", value.text()),
                    None => println!("{name}: (missing)"),
                },
            }
        }
    }

    Ok(())
}
```

`examples/read_values.rs` is this loop written the way a data frame library would want it — no allocation per cell, missing rendered as `NaN` and `""`. Run it against any file with:

```sh
cargo run --example read_values -- path/to/file.sav
```

Alongside the schema, the record reader reports the file header (`header()`), the declared row count (`case_count()`), the encoding it resolved and where that came from (`encoding_provenance()`), and any warnings the most recent read raised (`warnings()`).

## Reading the dictionary

When the metadata is the point — labels, value labels, documents, attributes — walk the dictionary first. Every record the file carried is handed out in order, and the transition to reading rows consumes whatever is left.

```rust
use spss_sav::spss::sav::dictionary_record::DictionaryRecord;
use spss_sav::spss::sav::sav_error::Result;
use spss_sav::spss::sav::sav_reader_builder::SavReaderBuilder;

fn read_dictionary(path: &str) -> Result<()> {
    let mut dictionary = SavReaderBuilder::new()
        .from_path(path)?
        .into_dictionary_reader()?;

    println!("written by {}", dictionary.header().product_name());

    while let Some(record) = dictionary.read_record()? {
        match record {
            DictionaryRecord::Variable(variable) => {
                println!("variable {}", variable.short_name());
            }
            DictionaryRecord::ValueLabelSet(set) => {
                println!("{} value labels", set.entries().len());
            }
            DictionaryRecord::Document(document) => {
                println!("{} document lines", document.lines().len());
            }
            DictionaryRecord::Extension(extension) => {
                println!("extension record: {extension:?}");
            }
            // `DictionaryRecord` is `#[non_exhaustive]`: a record kind
            // added later must not break your build.
            _ => {}
        }
    }

    // Rows read the same either way.
    let mut reader = dictionary.into_record_reader()?;
    while let Some(_record) = reader.read_record()? {
        // ...
    }

    Ok(())
}
```

`peek_kind()` classifies the next record for free, so you can decide whether it is worth decoding before paying for it; `skip_record()` passes over one you have decided against. Neither can leave the schema with a hole in it — anything the schema draws on is folded in regardless of what the caller pulls.

Choosing between the two entry points is the whole of how retention is expressed. There is no filtering option to configure: `into_record_reader` keeps nothing, `into_dictionary_reader` keeps everything, and no choice on either path can change how a row decodes. Row count, widths, encoding, missing tagging and variable names come out identical.

## Lazy records

`read_record()` decodes every cell in the row. When a file is wide and only a few columns matter, `read_lazy_record()` defers decoding until a cell is asked for — the rest are never touched.

```rust
while let Some(lazy) = reader.read_lazy_record()? {
    let id = lazy.value(0);
    let name = lazy.value(5);
    // Other columns are never decoded.
}
```

`skip_record()` goes further and decodes nothing at all. The bytes still have to be read — the reader chain has no `Seek` bound, and under either compressed scheme a row's length is not known until it has been decoded — but no cell is split out and nothing is allocated.

## Missing values

SPSS has two distinct notions of missing, and the crate keeps them apart rather than flattening both to one sentinel.

- **System-missing** is the absence of a value, written as a reserved `f64`.
- **User-declared missing** is a value the dictionary singles out — `MISSING VALUES age (99)`, a range like `LOWEST THRU 0`, or a string such as `'N/A'`. The number or the bytes are real data that the variable asks you to treat as absent.

Classification happens while the row is decoded, against the declared missing values for that variable, so a cell never claims to be present when the dictionary says otherwise. Both kinds keep their payload:

```rust
use spss_sav::spss::missing_value::MissingValue;
use spss_sav::spss::numeric::Numeric;
use spss_sav::spss::sav::value::Value;

fn describe(cell: &Value<'_>) -> String {
    match cell {
        Value::Numeric(Numeric::Present(value)) => format!("{value}"),
        Value::Numeric(Numeric::Missing(MissingValue::System)) => "system missing".to_owned(),
        Value::Numeric(Numeric::Missing(MissingValue::UserDefined(value))) => {
            // The declared value survives: 99 is still 99.
            format!("missing (declared {value})")
        }
        // There is no system-missing for strings, so a string cell always
        // has bytes — `value()` is total, `present()` is the one that
        // returns `None` for a declared-missing cell.
        Value::String(text) => format!("{:?}", text.value().text()),
    }
}
```

Callers that want the flattened view can ignore the distinction: `Numeric::present()` and `Text::present()` return `None` for anything missing, and `Value::is_missing()` answers for either arm.

## Value labels

Value labels are attached to the variables they describe, shared between the variables a single label set covers rather than copied per variable.

```rust
use spss_sav::spss::sav::value_label_value::ValueLabelValue;

if let Some(variable) = reader.schema().variable_by_name("q1")
    && let Some(label) = variable.label_for(&ValueLabelValue::Numeric(1.0))
{
    println!("1 means {label}");
}
```

Long-string labels (extension subtype 21) carry keys at the variable's full declared width rather than the eight bytes a type-3 record uses, and both kinds are reachable the same way. Keys stay as raw bytes on purpose: a key that is not valid in the file's declared encoding still has to compare equal to the cell holding it.

## Character encodings

[encoding_rs](https://docs.rs/encoding_rs) is a hard dependency, not a feature flag. A SAV file declares its encoding at the *end* of the dictionary — after every string that encoding governs — so the reader buffers the dictionary undecoded, resolves the encoding, and only then decodes any text, including the header's own product name and file label.

By default the file's declaration is honored, with `windows-1252` assumed when the file declares nothing and an unresolvable declaration failing the read. Both fallbacks are configurable, and an explicit encoding can override the declaration entirely:

```rust
use spss_sav::spss::sav::encoding_strategy::EncodingStrategy;
use spss_sav::spss::sav::sav_reader_builder::SavReaderBuilder;

// Trust this encoding regardless of what the file claims. A mismatch
// surfaces as a warning rather than an error.
let reader = SavReaderBuilder::new()
    .encoding_strategy(EncodingStrategy::Override(encoding_rs::SHIFT_JIS))
    .from_path("legacy.sav")?;
```

`encoding_provenance()` reports which encoding was applied and which record declared it, so a surprising decode can be traced to its source.

## What's supported

**Compression.** All three schemes a `.sav` or `.zsav` file can use: uncompressed, SAV bytecode compression (every command byte, including the row-cut and space-padding cases), and ZSAV zlib block compression across block boundaries.

**Floating point.** Four on-disk representations: IEEE 754, IBM hexadecimal floating point, and VAX D_float and G_float, each with its own canonical missing-value sentinel triple. Byte order is detected from the header and carried alongside the format, so an `f64` has exactly one conversion path.

**Extension records.** All sixteen recognized type-7 subtypes are parsed: machine integer and floating-point info, variable sets, multiple response sets (both the original and the extended, counted-value form), extra product info, display parameters, UUID, long variable names, very long strings, the extended case count, file and variable attributes, character encoding, long-string value labels, and long-string missing values. Unrecognized subtypes are preserved verbatim rather than discarded, and never fail the read.

**Very long strings.** Variables wider than 255 bytes are stored segmented on disk; the reader reassembles them and presents one variable at its declared width. The segmentation never leaks into the API.

**Diagnostics.** Recoverable problems accumulate as warnings rather than failing the read — a stale case count, a dictionary that disagrees with its own extension records, an unknown measurement level, a lossy decode. A file that any other reader opens will open here too.

## Correctness

The format documentation for SAV is reverse-engineered, and it is wrong in places, so this crate is checked against two independent references rather than against a specification alone:

- **Real files.** `tests/fixtures/*.sav` are written by [GNU PSPP](https://www.gnu.org/software/pspp/) from the `.sps` generators kept beside them, and `tests/real_files.rs` asserts the parsed dictionary and rows against them. Hand-built byte payloads only test the reader against its own assumptions; a file another implementation actually wrote does not.
- **PSPP's developer manual**, which documents the format at wire level. The whole System File Format chapter has been audited against the shipped reader — every subtype assignment, every extension payload shape, and the four core record types. That pass found real bugs in shapes no fixture we can generate would produce, because PSPP does not write them.

Where the format's documentation is ambiguous about strictness, the crate matches what a well-established reader produces for the data and raises a warning where those readers are silent.

## Planned work

Roughly in the order they are likely to land.

**The writer.** The format is understood well enough to write it, and the reader's typestate approach mirrors onto a writer directly. Not started.

**Async I/O.** A `tokio` feature is declared in `Cargo.toml` but does nothing yet; enabling it today changes no behavior. The intent is async terminals on the builder that mirror the sync chain with `.await` at each step, sharing the same pure parsing state so only the I/O loop differs.

**Date and time helpers.** A `chrono` feature is likewise declared and unimplemented, and `spss::temporal` is an empty placeholder. SPSS stores dates and times as plain numeric values whose meaning lives in the variable's display format, so the planned shape is a layered one: pure numeric conversions with no time-crate dependency, format classification, and typed adapters behind the feature flag.

**Other SPSS formats.** Two more are candidates, both read-only:

- **Portable files (`.por`)** — SPSS's transport format, designed for exchanging data between systems with incompatible numeric representations. It is a text format: 80-character lines, its own base-30 number encoding, and a translation table in the header. PSPP calls it "mostly obsolete", but files still circulate in long-running research archives, and a reader is worth having for exactly the case the format was built for. Note that despite being a text format it is a *transport* format, not a way to describe an external raw data file — SPSS does that with `DATA LIST` inside a `.sps` syntax file, which is a separate problem and not currently planned.
- **SPSS/PC+ system files (`.sys`)** — the legacy DOS format. Rarer still, and only worth doing if a real file turns up that needs it.

A future format would slot in alongside `spss::sav` under the same `spss` root, sharing the format-agnostic types — values, missing-value representations, encodings — that already live there for this reason.

## About

This is a passion project that I maintain on my own time. I care deeply about its quality and want it to be genuinely useful, but I also want to keep it fun and sustainable. To that end:

- **Bug reports** are always welcome. Please file issues for anything that isn't working correctly. A small `.sav` file that reproduces the problem is the single most useful thing you can attach.
- **Feature requests** are best expressed as pull requests. I'm much more likely to engage with a well-crafted PR than a request for new work.
- **Timelines** are my own. I'll get to things when I can, and I may close issues or PRs that don't align with the project's direction — nothing personal.

If you find this library valuable, the best way to support it is to contribute or share it with others.
