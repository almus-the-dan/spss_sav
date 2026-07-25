//! The kind of multiple response set (category vs dichotomy).

use crate::spss::sav::extensions::category_label_source::CategoryLabelSource;

/// The kind of multiple response set, and its kind-specific data.
///
/// Corresponds to the wire type letter: `C` (multiple category), `D`
/// (multiple dichotomy, `CATEGORYLABELS=VARLABELS`), or `E` (multiple
/// dichotomy, `CATEGORYLABELS=COUNTEDVALUES`). `D` and `E` are both
/// dichotomies distinguished by their
/// [`CategoryLabelSource`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MultipleResponseSetKind {
    /// A multiple-category set (wire type `C`): each member variable
    /// is an answer slot holding a category code, sharing one set of
    /// value labels. Carries no counted value.
    MultipleCategory,
    /// A multiple-dichotomy set (wire type `D` or `E`): each member
    /// variable is a yes/no flag, and the `counted_value` is the value
    /// that means "selected".
    MultipleDichotomy {
        /// The value that counts as "selected" for the member
        /// variables, decoded through the file's active encoding.
        counted_value: String,
        /// Where the per-category labels come from.
        category_labels: CategoryLabelSource,
    },
}
