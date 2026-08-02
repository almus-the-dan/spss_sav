//! Command units read straight from the file.

use std::io::Read;

use crate::spss::sav::compression::data_unit_source::DataUnitSource;
use crate::spss::sav::reader_state::ReaderState;
use crate::spss::sav::sav_error::Result;
use crate::spss::sav::segment_layout::DATA_UNIT_LEN;

/// Feeds the bytecode decoder straight from the file.
///
/// The `$FL2` bytecode case, where the data section *is* the command
/// stream and there is no container to unwrap. Stateless: the reader's
/// own position is all the state there is.
#[derive(Debug, Default)]
pub(crate) struct FileUnits;

impl<R: Read> DataUnitSource<R> for FileUnits {
    fn next_unit(&mut self, _state: &mut ReaderState<R>) -> Result<Option<[u8; DATA_UNIT_LEN]>> {
        todo!("body lands with Phase 6(b)")
    }
}
