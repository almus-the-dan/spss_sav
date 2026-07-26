//! A single decoded data record from the records section.

use crate::spss::sav::value::Value;

/// A single decoded data record from a SAV file.
///
/// Cells appear in declaration order, one per
/// [`SavVariable`](crate::spss::sav::sav_variable::SavVariable). String
/// cells borrow from the reader's internal row buffer when the bytes
/// are already valid in the file's encoding; the borrow is invalidated
/// the next time `read_record` is called.
///
/// Use [`LazySavRecord`](crate::spss::sav::lazy_sav_record::LazySavRecord)
/// when you only need a subset of cells per row — it skips decoding
/// for the cells you don't access.
#[derive(Debug, Clone)]
pub struct SavRecord<'a> {
    values: Vec<Value<'a>>,
}

impl<'a> SavRecord<'a> {
    #[allow(dead_code)] // exercised once the record reader phase lands.
    pub(crate) fn new(values: Vec<Value<'a>>) -> Self {
        Self { values }
    }

    /// Decoded cells of this record, in declaration order.
    #[must_use]
    #[inline]
    pub fn values(&self) -> &[Value<'a>] {
        &self.values
    }
}
