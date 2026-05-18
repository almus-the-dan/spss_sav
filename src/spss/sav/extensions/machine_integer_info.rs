//! Subtype 5 — integer-typed environment metadata.

use crate::spss::sav::byte_order::ByteOrder;
use crate::spss::sav::dictionary_format::{
    ENDIANNESS_BIG_ENDIAN, ENDIANNESS_LITTLE_ENDIAN, FLOATING_POINT_REPRESENTATION_IBM_HFP,
    FLOATING_POINT_REPRESENTATION_IEEE, FLOATING_POINT_REPRESENTATION_VAX,
};
use crate::spss::sav::float_format::FloatFormat;

/// Integer-typed environment metadata from extension record subtype
/// 5: version numbers, machine code, floating-point representation,
/// compression code, endianness, and character encoding code.
///
/// Eight `i32` fields, carried verbatim as read from disk. Several
/// of them (notably [`floating_point_representation`] and
/// [`endianness`]) duplicate information the dictionary reader
/// already derived from the file header; the reader exposes both
/// and emits a
/// [`SavWarning`](crate::spss::sav::sav_warning::SavWarning) when
/// the two disagree, leaving final reconciliation to consumers.
///
/// Convenience methods like [`floating_point_representation_kind`]
/// and [`endianness_kind`] map the well-known tagged codes onto the
/// crate's existing [`FloatFormat`] / [`ByteOrder`] enums; they
/// return `None` for codes not in the recognized set.
///
/// [`floating_point_representation`]: Self::floating_point_representation
/// [`endianness`]: Self::endianness
/// [`floating_point_representation_kind`]: Self::floating_point_representation_kind
/// [`endianness_kind`]: Self::endianness_kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineIntegerInfo {
    version_major: i32,
    version_minor: i32,
    version_revision: i32,
    machine_code: i32,
    floating_point_representation: i32,
    compression_code: i32,
    endianness: i32,
    character_code: i32,
}

impl MachineIntegerInfo {
    /// Returns a fresh [`MachineIntegerInfoBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> MachineIntegerInfoBuilder {
        MachineIntegerInfoBuilder::default()
    }

    /// Major version of the SPSS release that wrote the file.
    #[must_use]
    #[inline]
    pub fn version_major(&self) -> i32 {
        self.version_major
    }

    /// Minor version of the SPSS release that wrote the file.
    #[must_use]
    #[inline]
    pub fn version_minor(&self) -> i32 {
        self.version_minor
    }

    /// Revision number of the SPSS release that wrote the file.
    #[must_use]
    #[inline]
    pub fn version_revision(&self) -> i32 {
        self.version_revision
    }

    /// Opaque machine identifier as written by the producing
    /// platform. The value space is not documented to a useful
    /// degree; surface it verbatim.
    #[must_use]
    #[inline]
    pub fn machine_code(&self) -> i32 {
        self.machine_code
    }

    /// Tagged code identifying the file's floating-point
    /// representation (`1` = IEEE 754, `2` = IBM HFP, `3` = VAX).
    /// Use [`floating_point_representation_kind`] for the typed
    /// form.
    ///
    /// [`floating_point_representation_kind`]: Self::floating_point_representation_kind
    #[must_use]
    #[inline]
    pub fn floating_point_representation(&self) -> i32 {
        self.floating_point_representation
    }

    /// Maps [`floating_point_representation`] onto the typed
    /// [`FloatFormat`] enum. Returns `None` for codes outside the
    /// documented set; the raw value is still available via
    /// [`floating_point_representation`].
    ///
    /// [`floating_point_representation`]: Self::floating_point_representation
    #[must_use]
    pub fn floating_point_representation_kind(&self) -> Option<FloatFormat> {
        match self.floating_point_representation {
            FLOATING_POINT_REPRESENTATION_IEEE => Some(FloatFormat::Ieee754),
            FLOATING_POINT_REPRESENTATION_IBM_HFP => Some(FloatFormat::IbmHfp),
            FLOATING_POINT_REPRESENTATION_VAX => Some(FloatFormat::Vax),
            _ => None,
        }
    }

    /// Tagged compression code. SPSS writes `1` for bytecode
    /// compression; other values appear only in malformed or
    /// non-SPSS files.
    #[must_use]
    #[inline]
    pub fn compression_code(&self) -> i32 {
        self.compression_code
    }

    /// Tagged endianness code (`1` = big-endian, `2` = little-
    /// endian). Use [`endianness_kind`] for the typed form.
    ///
    /// [`endianness_kind`]: Self::endianness_kind
    #[must_use]
    #[inline]
    pub fn endianness(&self) -> i32 {
        self.endianness
    }

    /// Maps [`endianness`] onto the typed [`ByteOrder`] enum.
    /// Returns `None` for codes outside the documented set; the
    /// raw value is still available via [`endianness`].
    ///
    /// [`endianness`]: Self::endianness
    #[must_use]
    pub fn endianness_kind(&self) -> Option<ByteOrder> {
        match self.endianness {
            ENDIANNESS_BIG_ENDIAN => Some(ByteOrder::BigEndian),
            ENDIANNESS_LITTLE_ENDIAN => Some(ByteOrder::LittleEndian),
            _ => None,
        }
    }

    /// Opaque character-set code identifying the file's text
    /// encoding. SPSS uses several conventions here (legacy
    /// numeric codes, Windows code pages, locale numbers); the
    /// reader surfaces it verbatim and defers interpretation.
    #[must_use]
    #[inline]
    pub fn character_code(&self) -> i32 {
        self.character_code
    }
}

/// Builder for [`MachineIntegerInfo`].
#[derive(Debug, Default, Clone, Copy)]
pub struct MachineIntegerInfoBuilder {
    version_major: Option<i32>,
    version_minor: Option<i32>,
    version_revision: Option<i32>,
    machine_code: Option<i32>,
    floating_point_representation: Option<i32>,
    compression_code: Option<i32>,
    endianness: Option<i32>,
    character_code: Option<i32>,
}

impl MachineIntegerInfoBuilder {
    /// Sets the major version.
    #[must_use]
    #[inline]
    pub fn version_major(mut self, value: i32) -> Self {
        self.version_major = Some(value);
        self
    }

    /// Sets the minor version.
    #[must_use]
    #[inline]
    pub fn version_minor(mut self, value: i32) -> Self {
        self.version_minor = Some(value);
        self
    }

    /// Sets the revision number.
    #[must_use]
    #[inline]
    pub fn version_revision(mut self, value: i32) -> Self {
        self.version_revision = Some(value);
        self
    }

    /// Sets the machine code.
    #[must_use]
    #[inline]
    pub fn machine_code(mut self, value: i32) -> Self {
        self.machine_code = Some(value);
        self
    }

    /// Sets the floating-point representation code.
    #[must_use]
    #[inline]
    pub fn floating_point_representation(mut self, value: i32) -> Self {
        self.floating_point_representation = Some(value);
        self
    }

    /// Sets the compression code.
    #[must_use]
    #[inline]
    pub fn compression_code(mut self, value: i32) -> Self {
        self.compression_code = Some(value);
        self
    }

    /// Sets the endianness code.
    #[must_use]
    #[inline]
    pub fn endianness(mut self, value: i32) -> Self {
        self.endianness = Some(value);
        self
    }

    /// Sets the character-set code.
    #[must_use]
    #[inline]
    pub fn character_code(mut self, value: i32) -> Self {
        self.character_code = Some(value);
        self
    }

    /// Finalizes this builder into a [`MachineIntegerInfo`].
    ///
    /// Unset fields default to `0`.
    #[must_use]
    #[inline]
    pub fn build(self) -> MachineIntegerInfo {
        let version_major = self.version_major.unwrap_or(0);
        let version_minor = self.version_minor.unwrap_or(0);
        let version_revision = self.version_revision.unwrap_or(0);
        let machine_code = self.machine_code.unwrap_or(0);
        let floating_point_representation = self.floating_point_representation.unwrap_or(0);
        let compression_code = self.compression_code.unwrap_or(0);
        let endianness = self.endianness.unwrap_or(0);
        let character_code = self.character_code.unwrap_or(0);
        MachineIntegerInfo {
            version_major,
            version_minor,
            version_revision,
            machine_code,
            floating_point_representation,
            compression_code,
            endianness,
            character_code,
        }
    }
}
