//! Catch-all extension record for unrecognized subtypes.

/// An extension record carrying a subtype this library does not yet
/// recognize, preserved verbatim.
///
/// The reader emits a
/// [`SavWarning::UnknownExtensionSubtype`](crate::spss::sav::sav_warning::SavWarning::UnknownExtensionSubtype)
/// whenever one of these is produced, and the writer accepts it as
/// input so a round-trip preserves the original bytes bit-for-bit.
///
/// The on-disk encoding splits the payload into `element_size *
/// element_count` bytes; both dimensions are kept on the struct so
/// that a writer can re-emit the record verbatim without inferring
/// either factor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownExtension {
    subtype: u32,
    element_size: usize,
    element_count: usize,
    payload: Vec<u8>,
}

impl UnknownExtension {
    /// Returns a fresh [`UnknownExtensionBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> UnknownExtensionBuilder {
        UnknownExtensionBuilder::default()
    }

    /// The on-disk subtype number this record carried.
    #[must_use]
    #[inline]
    pub fn subtype(&self) -> u32 {
        self.subtype
    }

    /// Element size in bytes, as declared in the record envelope.
    /// The on-disk encoding is a `u32`; the reader exposes it as a
    /// [`usize`] for arithmetic ergonomics. The writer validates the
    /// `u32` fit at write time.
    #[must_use]
    #[inline]
    pub fn element_size(&self) -> usize {
        self.element_size
    }

    /// Element count, as declared in the record envelope. The
    /// on-disk encoding is a `u32`; this is the same convention as
    /// [`element_size`](Self::element_size).
    #[must_use]
    #[inline]
    pub fn element_count(&self) -> usize {
        self.element_count
    }

    /// Raw payload bytes — exactly
    /// `element_size() * element_count()` bytes long.
    #[must_use]
    #[inline]
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}

/// Builder for [`UnknownExtension`].
#[derive(Debug, Default, Clone)]
pub struct UnknownExtensionBuilder {
    subtype: Option<u32>,
    element_size: Option<usize>,
    element_count: Option<usize>,
    payload: Option<Vec<u8>>,
}

impl UnknownExtensionBuilder {
    /// Sets the on-disk subtype number.
    #[must_use]
    #[inline]
    pub fn subtype(mut self, subtype: u32) -> Self {
        self.subtype = Some(subtype);
        self
    }

    /// Sets the declared element size in bytes.
    #[must_use]
    #[inline]
    pub fn element_size(mut self, element_size: usize) -> Self {
        self.element_size = Some(element_size);
        self
    }

    /// Sets the declared element count.
    #[must_use]
    #[inline]
    pub fn element_count(mut self, element_count: usize) -> Self {
        self.element_count = Some(element_count);
        self
    }

    /// Sets the raw payload bytes.
    #[must_use]
    #[inline]
    pub fn payload(mut self, payload: Vec<u8>) -> Self {
        self.payload = Some(payload);
        self
    }

    /// Finalizes this builder into an [`UnknownExtension`].
    ///
    /// Unset fields default to zero or an empty payload.
    #[must_use]
    #[inline]
    pub fn build(self) -> UnknownExtension {
        let subtype = self.subtype.unwrap_or(0);
        let element_size = self.element_size.unwrap_or(0);
        let element_count = self.element_count.unwrap_or(0);
        let payload = self.payload.unwrap_or_default();
        UnknownExtension {
            subtype,
            element_size,
            element_count,
            payload,
        }
    }
}
