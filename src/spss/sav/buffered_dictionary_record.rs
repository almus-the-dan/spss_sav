//! One buffered dictionary record, paired with the warnings it raised.

use crate::spss::sav::buffered_record_payload::BufferedRecordPayload;
use crate::spss::sav::sav_warning::SavWarning;

/// An undecoded dictionary record together with the warnings raised
/// while reading it.
///
/// The warnings travel with the record rather than accumulating in the
/// reader's shared vec, because buffering separates *reading* a record
/// from *yielding* it. Without this pairing, every warning from the
/// whole dictionary would land on the first
/// [`read_record`](crate::spss::sav::dictionary_reader::DictionaryReader::read_record)
/// call and none on the rest, breaking the documented contract that
/// `warnings()` reports what the most recent call produced. Holding
/// them here lets each record's warnings be replayed as that record is
/// handed to the caller.
///
/// Warnings raised by *decoding* — which happens at hand-out time — are
/// appended to the reader's vec directly and are not stored here.
#[derive(Debug)]
pub(crate) struct BufferedDictionaryRecord {
    /// The record's undecoded payload.
    pub(crate) payload: BufferedRecordPayload,
    /// Warnings raised while reading this record off the wire.
    pub(crate) warnings: Vec<SavWarning>,
}
