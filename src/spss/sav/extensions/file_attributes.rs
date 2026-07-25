//! Subtype 17 — file-level custom attributes (collection wrapper).

use crate::spss::sav::extensions::file_attribute::FileAttribute;

/// The file-level custom attributes from one extension subtype-17
/// record.
///
/// A newtype over the parsed [`FileAttribute`]s, in on-disk order, so
/// the extension record's payload shape can gain fields without
/// changing the enum variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileAttributes {
    attributes: Vec<FileAttribute>,
}

impl FileAttributes {
    /// Returns a fresh [`FileAttributesBuilder`].
    #[must_use]
    #[inline]
    pub fn builder() -> FileAttributesBuilder {
        FileAttributesBuilder::default()
    }

    /// The file attributes, in on-disk order.
    #[must_use]
    #[inline]
    pub fn attributes(&self) -> &[FileAttribute] {
        &self.attributes
    }
}

/// Builder for [`FileAttributes`].
#[derive(Debug, Default, Clone)]
pub struct FileAttributesBuilder {
    attributes: Vec<FileAttribute>,
}

impl FileAttributesBuilder {
    /// Appends one file attribute.
    #[must_use]
    #[inline]
    pub fn attribute(mut self, value: FileAttribute) -> Self {
        self.attributes.push(value);
        self
    }

    /// Replaces the collection with `attributes`.
    #[must_use]
    #[inline]
    pub fn attributes(mut self, attributes: Vec<FileAttribute>) -> Self {
        self.attributes = attributes;
        self
    }

    /// Finalizes this builder into a [`FileAttributes`].
    ///
    /// Unset attributes default to an empty list.
    #[must_use]
    #[inline]
    pub fn build(self) -> FileAttributes {
        FileAttributes {
            attributes: self.attributes,
        }
    }
}
