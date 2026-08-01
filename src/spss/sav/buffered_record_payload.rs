//! The undecoded payload of one buffered dictionary record.

use crate::spss::sav::buffered_document_record::BufferedDocumentRecord;
use crate::spss::sav::buffered_value_label_set::BufferedValueLabelSet;
use crate::spss::sav::buffered_variable_record::BufferedVariableRecord;
use crate::spss::sav::dictionary_record_kind::DictionaryRecordKind;
use crate::spss::sav::extension_envelope::ExtensionEnvelope;
use crate::spss::sav::extensions::extension_subtype::ExtensionSubtype;

/// One dictionary record as read off the wire, before its text has been
/// decoded.
///
/// Mirrors the four variants of
/// [`DictionaryRecord`](crate::spss::sav::dictionary_record::DictionaryRecord),
/// which is what each of these becomes once the encoding is resolved.
/// Type-7 records need no dedicated variant: [`ExtensionEnvelope`] is
/// already the undecoded form for all 15 subtypes, holding its payload
/// verbatim, and every per-subtype `read` helper is a pure function of
/// it plus an encoding.
#[derive(Debug)]
pub(crate) enum BufferedRecordPayload {
    /// A type-2 variable record, post-continuation-collapse.
    Variable(BufferedVariableRecord),
    /// A type-3 record with its paired type-4 record.
    ValueLabelSet(BufferedValueLabelSet),
    /// A type-6 document record.
    Document(BufferedDocumentRecord),
    /// A type-7 extension record of any subtype.
    Extension(ExtensionEnvelope),
}

impl BufferedRecordPayload {
    /// This record's kind, available without decoding it.
    ///
    /// Classifying an extension costs only an
    /// [`ExtensionSubtype::from_code`] on the already-read envelope, so
    /// [`DictionaryReader::peek_kind`](crate::spss::sav::dictionary_reader::DictionaryReader::peek_kind)
    /// can answer without touching the payload.
    pub fn kind(&self) -> DictionaryRecordKind {
        match self {
            Self::Variable(_) => DictionaryRecordKind::Variable,
            Self::ValueLabelSet(_) => DictionaryRecordKind::ValueLabelSet,
            Self::Document(_) => DictionaryRecordKind::Document,
            Self::Extension(envelope) => {
                let subtype = ExtensionSubtype::from_code(envelope.subtype);
                DictionaryRecordKind::Extension(subtype)
            }
        }
    }
}
