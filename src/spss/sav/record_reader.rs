//! Reader for the data-record section of a SAV file.
//!
//! Phase 4 placeholder — the real definition (with `header()`,
//! `schema()`, `read_record()`, `read_lazy_record()`, etc.) lands
//! alongside the bytecode and ZLIB decoders in Phase 6. Defined as
//! a unit shell now so
//! [`DictionaryReader::into_record_reader`](crate::spss::sav::dictionary_reader::DictionaryReader::into_record_reader)
//! has a return type to name.

/// Reader for the data-record section of a SAV file.
///
/// This is currently a placeholder; the real surface lands in Phase 6.
#[derive(Debug)]
#[allow(dead_code)] // exercised once the record reader phase lands.
pub struct RecordReader<R> {
    _placeholder: core::marker::PhantomData<R>,
}
