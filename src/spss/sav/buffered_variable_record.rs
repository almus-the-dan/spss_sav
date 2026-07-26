//! A type-2 variable record held undecoded until the encoding is known.

use crate::spss::sav::dictionary_format::VARIABLE_SHORT_NAME_LEN;
use crate::spss::sav::raw_missing_values::RawMissingValues;
use crate::spss::sav::sav_format::SavFormat;
use crate::spss::sav::variable_type::VariableType;

/// A type-2 variable record, fully validated but not yet decoded.
///
/// Only two fields in a variable record need the encoding: the 8-byte
/// short name and the variable label. Everything else is numeric and is
/// parsed and validated while the record is buffered, so this type holds
/// those fields in their final form and keeps only the text raw.
/// `missing_values` stays raw permanently — see
/// [`RawMissingValues`].
///
/// Continuation records are collapsed during buffering and never reach
/// this type.
#[derive(Debug)]
pub(crate) struct BufferedVariableRecord {
    /// Raw short-name bytes, padding not yet trimmed.
    pub(crate) short_name: [u8; VARIABLE_SHORT_NAME_LEN],
    /// Raw variable-label bytes with the padding already removed,
    /// present when `has_var_label` was set.
    pub(crate) label: Option<Vec<u8>>,
    pub(crate) variable_type: VariableType,
    pub(crate) missing_values: RawMissingValues,
    pub(crate) print_format: SavFormat,
    pub(crate) write_format: SavFormat,
}
