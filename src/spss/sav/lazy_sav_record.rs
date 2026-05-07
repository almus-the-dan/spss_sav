//! A single data record decoded on demand, cell by cell.

use core::marker::PhantomData;

use crate::spss::sav::sav_error::Result;
use crate::spss::sav::value::Value;

/// A single data record decoded on demand, cell by cell.
///
/// Backed by the reader's internal row buffer. Each call to
/// [`value`](Self::value) decodes only the requested cell;
/// non-accessed cells are never decoded. The borrow of the row buffer
/// is invalidated the next time `read_record` / `read_lazy_record` is
/// called on the parent reader.
///
/// Use [`SavRecord`](crate::spss::sav::sav_record::SavRecord) when you
/// want the entire row decoded eagerly.
#[derive(Debug)]
pub struct LazySavRecord<'a> {
    _phantom: PhantomData<&'a ()>,
}

impl<'a> LazySavRecord<'a> {
    /// The number of cells in this record.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        todo!("body lands with the record reader")
    }

    /// Whether the record is empty.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        todo!("body lands with the record reader")
    }

    /// Decode and return the cell at `index`.
    ///
    /// # Errors
    ///
    /// Returns a [`SavError`](crate::spss::sav::sav_error::SavError)
    /// if the cell's bytes are not valid in the file's declared
    /// encoding, or if `index` is out of range.
    pub fn value(&self, _index: usize) -> Result<Value<'a>> {
        todo!("body lands with the record reader")
    }
}
