//! Which kind of record a dictionary record is, without its payload.

use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;

/// The kind of one dictionary record, carrying no payload.
///
/// Obtained from
/// [`DictionaryRecord::kind`](crate::spss::sav::dictionary_record::DictionaryRecord::kind)
/// for a record already in hand, or from
/// [`DictionaryReader::peek_kind`](crate::spss::sav::dictionary_reader::DictionaryReader::peek_kind)
/// for the record about to be handed out — which is what lets a caller
/// decide whether a record is worth decoding before paying for it.
///
/// Mirrors [`DictionaryRecord`](crate::spss::sav::dictionary_record::DictionaryRecord)
/// variant for variant, with the extension subtype carried along
/// because "is this an extension record" is rarely a useful question on
/// its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum DictionaryRecordKind {
    /// A type-2 variable record.
    Variable,
    /// A type-3 + type-4 paired value-label set.
    ValueLabelSet,
    /// A type-6 document record.
    Document,
    /// A type-7 extension record of the given subtype.
    Extension(ExtensionSubtype),
}
