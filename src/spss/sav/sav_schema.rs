//! Schema of variables in a SAV file.

use crate::spss::sav::sav_variable::SavVariable;

/// The set of variables in a SAV file, in declaration order.
///
/// `SavSchema` is a thin wrapper around an ordered `Vec<SavVariable>`
/// plus crate-internal bookkeeping (notably the per-row byte length
/// the record reader uses to size its row buffer). It has no public
/// constructor or builder; users only ever get a `SavSchema` from
/// `RecordReader::schema()` after the dictionary phase has finalized.
#[derive(Debug, Clone)]
pub struct SavSchema {
    variables: Vec<SavVariable>,
    #[allow(dead_code)] // exercised once the record reader phase lands.
    row_len: usize,
}

impl SavSchema {
    #[allow(dead_code)] // exercised once the record reader phase lands.
    pub(crate) fn new(variables: Vec<SavVariable>, row_len: usize) -> Self {
        Self { variables, row_len }
    }

    /// All variables in declaration order.
    #[must_use]
    #[inline]
    pub fn variables(&self) -> &[SavVariable] {
        &self.variables
    }

    /// Per-row on-disk byte length used by the record reader to size
    /// its row buffer.
    #[allow(dead_code)] // exercised once the record reader phase lands.
    #[inline]
    pub(crate) fn row_len(&self) -> usize {
        self.row_len
    }
}
