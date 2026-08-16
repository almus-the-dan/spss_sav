//! How much of the dictionary the buffering pass keeps.

/// How much of the dictionary section
/// [`DictionaryBuffer::read`](crate::spss::sav::dictionary_buffer::DictionaryBuffer::read)
/// retains.
///
/// Not a caller-facing option. Which variant applies is decided by the
/// terminal method the caller reaches for on
/// [`SavReader`](crate::spss::sav::sav_reader::SavReader):
/// [`into_dictionary_reader`](crate::spss::sav::sav_reader::SavReader::into_dictionary_reader)
/// hands out dictionary records and so needs [`All`](Self::All), while
/// [`into_record_reader`](crate::spss::sav::sav_reader::SavReader::into_record_reader)
/// goes straight to the rows and so needs [`Minimal`](Self::Minimal).
/// Expressing it that way rather than as a flag is deliberate: a caller
/// cannot ask for records they have already declared they do not want.
///
/// It has to be known *before* the pass runs, which is why it is not
/// something the dictionary phase can be told later. A SAV file declares
/// its text encoding in extension records at the end of the dictionary,
/// so the whole section is walked by
/// [`into_dictionary_reader`](crate::spss::sav::sav_reader::SavReader::into_dictionary_reader)
/// before any text can be decoded. By the time that returns, whatever
/// was going to be retained is already resident.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DictionaryRetention {
    /// Keep every record, so the dictionary phase can hand them out.
    All,
    /// Keep only the schema-related records, and discard every other
    /// payload as it is scanned.
    ///
    /// Variable records stay because nothing else can supply what the
    /// schema builds each variable from — its label and its print and
    /// write formats. Their trailing blocks have to be parsed to find
    /// the next record at all. What a caller gives up is the content of
    /// the payloads that were never allocated: value labels, documents,
    /// display parameters, attributes, and long-string value labels do
    /// not reach the schema, and warnings those records would have raised
    /// are suppressed with them.
    ///
    /// What survives is everything a correct record reader depends on.
    /// The layout-bearing extension records are absorbed into the buffer's
    /// skeleton regardless — including the two the schema draws from
    /// there, subtypes 13 and 22. Rows, widths, encoding, missing
    /// tagging, the declared case count and the variable names all come
    /// out identical to a full read.
    Minimal,
}

impl DictionaryRetention {
    /// Whether a record a caller could ask for is worth keeping.
    pub fn keeps_records(self) -> bool {
        matches!(self, Self::All)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_all_keeps_records() {
        assert!(DictionaryRetention::All.keeps_records());
        assert!(!DictionaryRetention::Minimal.keeps_records());
    }
}
