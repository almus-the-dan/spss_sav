//! Wire-level fields from a single type-2 variable record.

/// The wire-level fields of a single SAV type-2 variable record,
/// after any continuation records have been collapsed.
///
/// `SavVariableHeader` is yielded transiently by the dictionary
/// reader (via
/// [`DictionaryRecord::Variable`](crate::spss::sav::dictionary_record::DictionaryRecord::Variable))
/// and carries only the fields present on the variable record itself.
/// The fully reconciled, extension-record-patched form is
/// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable), which
/// is what users obtain from `RecordReader::schema()` once the
/// dictionary phase has finalized.
///
/// This split intentionally avoids a soft "this looks complete but
/// isn't" contract: the wire-level type and the reconciled type are
/// distinct.
///
/// Fields land in Phase 5.
#[derive(Debug, Clone)]
pub struct SavVariableHeader {}
