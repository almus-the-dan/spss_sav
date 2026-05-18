//! Streaming-yielded record from the SAV dictionary section.

use crate::spss::sav::document_record::DocumentRecord;
use crate::spss::sav::extensions::extension_record::ExtensionRecord;
use crate::spss::sav::raw_value_label_set::RawValueLabelSet;
use crate::spss::sav::sav_variable_header::SavVariableHeader;

/// One typed record yielded by the streaming dictionary reader.
///
/// SAV's dictionary section is a stream of typed records (variables,
/// value-label sets, documents, extensions) freely interleaved
/// between the file header and the `999` end-of-dictionary marker.
/// `DictionaryRecord` is the union of those record kinds, owned (no
/// lifetime parameter) so that callers can store / forward records
/// without holding the reader.
///
/// The reader yields a [`Variable`](Self::Variable) carrying the
/// wire-level [`SavVariableHeader`]; the fully reconciled
/// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable) (with
/// long names, display, attributes, etc., patched in from extension
/// records) is materialized only when the dictionary phase finalizes
/// into the record reader.
///
/// Type-3 + type-4 records appear paired as a single
/// [`ValueLabelSet`](Self::ValueLabelSet) entry carrying the
/// wire-level [`RawValueLabelSet`] — the user never sees the
/// unpaired wire-level records. The fully reconciled, typed form
/// ([`ValueLabelSet`](crate::spss::sav::value_label_set::ValueLabelSet))
/// is materialized when the dictionary phase finalizes.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum DictionaryRecord {
    /// A single variable's wire-level header.
    Variable(SavVariableHeader),
    /// A type-3 + type-4 paired value-label set, wire-level form.
    ValueLabelSet(RawValueLabelSet),
    /// A document record (free-text annotation lines).
    Document(DocumentRecord),
    /// An extension record (any subtype, including unrecognized).
    Extension(ExtensionRecord),
}
