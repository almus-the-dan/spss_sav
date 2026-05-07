//! Display alignment of a SAV variable.

/// Display alignment hint for a SAV variable.
///
/// The alignment is a presentational attribute carried alongside
/// each variable's display parameters (extension subtype 11). It does
/// not affect storage or analysis — only how SPSS renders the column
/// in its data editor.
///
/// SAV's wire encoding uses `0=Left`, `1=Right`, `2=Center`. Any
/// other byte is preserved in [`Unknown`](Self::Unknown) for
/// round-trip fidelity. The "no display info at all" case is
/// represented by the absence of a display record on the variable
/// (an `Option<VariableDisplay>`), not by a sentinel variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Alignment {
    /// Left-aligned (canonical disk byte `0`).
    Left,
    /// Right-aligned (canonical disk byte `1`).
    Right,
    /// Center-aligned (canonical disk byte `2`).
    Center,
    /// An on-disk byte outside `0..=2`. The raw byte is preserved
    /// for round-trip fidelity.
    Unknown(u8),
}

impl Alignment {
    /// On-disk byte representation of this alignment.
    #[must_use]
    pub fn to_byte(self) -> u8 {
        match self {
            Self::Left => 0,
            Self::Right => 1,
            Self::Center => 2,
            Self::Unknown(byte) => byte,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_byte_canonical_alignments() {
        assert_eq!(Alignment::Left.to_byte(), 0);
        assert_eq!(Alignment::Right.to_byte(), 1);
        assert_eq!(Alignment::Center.to_byte(), 2);
    }

    #[test]
    fn to_byte_preserves_unknown() {
        assert_eq!(Alignment::Unknown(7).to_byte(), 7);
        assert_eq!(Alignment::Unknown(255).to_byte(), 255);
    }
}
