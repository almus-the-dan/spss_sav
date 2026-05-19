//! Subtype 11 — per-variable display parameters.

use crate::spss::sav::alignment::Alignment;
use crate::spss::sav::measurement_level::MeasurementLevel;

/// Display parameters for a single variable: measurement level,
/// display width (optional), and alignment.
///
/// `VariableDisplay` is the *finalized* form produced during schema
/// finalization. The streaming layer yields a
/// [`RawDisplayParameters`](crate::spss::sav::extensions::raw_display_parameters::RawDisplayParameters)
/// holding the verbatim `u32` values from the subtype-11 record; the
/// finalizer then slices those values across the dictionary's
/// variables and decodes each triple into a `VariableDisplay`.
///
/// `display_width` is [`None`] when the subtype-11 record was written
/// in the 2-tuple form (measure and alignment only). The 3-tuple form
/// supplies a width.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VariableDisplay {
    measurement_level: MeasurementLevel,
    display_width: Option<u32>,
    alignment: Alignment,
}

impl VariableDisplay {
    /// Returns a fresh [`VariableDisplayBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> VariableDisplayBuilder {
        VariableDisplayBuilder::default()
    }

    /// Measurement level (nominal / ordinal / scale, or unspecified).
    #[must_use]
    #[inline]
    pub fn measurement_level(&self) -> MeasurementLevel {
        self.measurement_level
    }

    /// The display width, if the subtype-11 record carried one.
    #[must_use]
    #[inline]
    pub fn display_width(&self) -> Option<u32> {
        self.display_width
    }

    /// Display alignment hint.
    #[must_use]
    #[inline]
    pub fn alignment(&self) -> Alignment {
        self.alignment
    }
}

/// Builder for [`VariableDisplay`].
#[derive(Debug, Default, Clone, Copy)]
pub struct VariableDisplayBuilder {
    measurement_level: Option<MeasurementLevel>,
    display_width: Option<u32>,
    alignment: Option<Alignment>,
}

impl VariableDisplayBuilder {
    /// Sets the measurement level.
    #[must_use]
    #[inline]
    pub fn measurement_level(mut self, value: MeasurementLevel) -> Self {
        self.measurement_level = Some(value);
        self
    }

    /// Sets the display column width.
    #[must_use]
    #[inline]
    pub fn display_width(mut self, value: u32) -> Self {
        self.display_width = Some(value);
        self
    }

    /// Clears the display column width (records the 2-tuple form of
    /// the subtype-11 record).
    #[must_use]
    #[inline]
    pub fn clear_display_width(mut self) -> Self {
        self.display_width = None;
        self
    }

    /// Sets the display alignment.
    #[must_use]
    #[inline]
    pub fn alignment(mut self, value: Alignment) -> Self {
        self.alignment = Some(value);
        self
    }

    /// Finalizes this builder into a [`VariableDisplay`].
    ///
    /// Unset measurement level defaults to
    /// [`MeasurementLevel::Unspecified`]; unset alignment defaults to
    /// [`Alignment::Left`]. `display_width` is `None` when unset.
    #[must_use]
    #[inline]
    pub fn build(self) -> VariableDisplay {
        let measurement_level = self
            .measurement_level
            .unwrap_or(MeasurementLevel::Unspecified);
        let alignment = self.alignment.unwrap_or(Alignment::Left);
        VariableDisplay {
            measurement_level,
            display_width: self.display_width,
            alignment,
        }
    }
}
