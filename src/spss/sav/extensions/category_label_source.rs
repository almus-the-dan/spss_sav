//! Where a multiple-dichotomy set's category labels come from.

/// The source of category labels for a multiple-dichotomy response
/// set (extension subtypes 7 and 19).
///
/// A multiple-dichotomy set draws one category per member variable.
/// This records where the label for each category comes from,
/// corresponding to SPSS's `CATEGORYLABELS` keyword.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryLabelSource {
    /// `CATEGORYLABELS=VARLABELS` (wire type `D`): each category is
    /// labeled by its member variable's variable label.
    VariableLabels,
    /// `CATEGORYLABELS=COUNTEDVALUES` (wire type `E`): categories are
    /// labeled from the counted value's value labels.
    ///
    /// `label_source` is the on-disk indicator PSPP writes: `11` when
    /// `LABELSOURCE=VARLABEL` was specified, otherwise `1`. It is kept
    /// as the raw number to preserve any value.
    CountedValues {
        /// The raw label-source indicator (`1` or `11`).
        label_source: u32,
    },
}
