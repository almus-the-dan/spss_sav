//! A single data record decoded on demand, cell by cell.

use crate::spss::sav::data_layout::DataLayout;
use crate::spss::sav::record_parse::parse_cell;
use crate::spss::sav::value::Value;

/// A single data record decoded on demand, cell by cell.
///
/// Backed by the reader's internal row buffer. Each call to
/// [`value`](Self::value) decodes only the requested cell; cells that
/// are never asked for are never decoded. The borrow of the row buffer
/// is invalidated the next time a record is read from the parent
/// reader.
///
/// Use [`SavRecord`](crate::spss::sav::sav_record::SavRecord) when you
/// want the whole row decoded eagerly.
#[derive(Debug)]
pub struct LazySavRecord<'a> {
    row: &'a [u8],
    layout: &'a DataLayout,
}

impl<'a> LazySavRecord<'a> {
    pub(crate) fn new(row: &'a [u8], layout: &'a DataLayout) -> Self {
        Self { row, layout }
    }

    /// The number of cells in this record.
    ///
    /// Counts logical variables, so a very long string counts once
    /// however many segments hold it — the same count
    /// [`SavSchema::variables`](crate::spss::sav::sav_schema::SavSchema::variables)
    /// reports.
    #[must_use]
    #[inline]
    pub fn len(&self) -> usize {
        self.layout.variables().len()
    }

    /// Whether the record has no cells.
    #[must_use]
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.layout.variables().is_empty()
    }

    /// Decodes and returns the cell at `index`, or `None` when `index`
    /// is out of range.
    ///
    /// Returns a value rather than a `Result` because decoding a cell
    /// cannot fail: bytes that are not valid in the file's declared
    /// encoding become U+FFFD, matching how the dictionary reader
    /// treats malformed text, and the raw bytes stay reachable through
    /// [`StringValue::raw`](crate::spss::sav::string_value::StringValue::raw)
    /// either way.
    #[must_use]
    pub fn value(&self, index: usize) -> Option<Value<'a>> {
        let variable = self.layout.variables().get(index)?;
        parse_cell(self.row, variable, self.layout)
    }
}
