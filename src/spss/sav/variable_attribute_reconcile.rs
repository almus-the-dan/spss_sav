//! Collapsing indexed attribute names into array-valued attributes.

use crate::spss::sav::extensions::variable_attribute_entry::VariableAttributeEntry;
use crate::spss::sav::variable_attribute::VariableAttribute;

/// Splits `fred[2]` into `("fred", Some(2))`, leaving an unindexed name
/// as `(name, None)`.
///
/// Only a well-formed, non-empty, all-digit `[n]` suffix counts. A name
/// that merely ends in a bracket, or brackets something that is not a
/// number, is left alone — the wire layer keeps names verbatim, so
/// anything not recognizably an index really is part of the name.
fn split_index(name: &str) -> (&str, Option<u32>) {
    let Some(stripped) = name.strip_suffix(']') else {
        return (name, None);
    };
    let Some(open) = stripped.rfind('[') else {
        return (name, None);
    };
    let digits = &stripped[open + 1..];
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return (name, None);
    }
    match digits.parse::<u32>() {
        Ok(index) => (&stripped[..open], Some(index)),
        // A run of digits too long for u32 is not an index SPSS wrote.
        Err(_) => (name, None),
    }
}

/// Folds a wire-level attribute list into its user-facing form.
///
/// SPSS writes an array-valued attribute as a run of separately named
/// entries — `fred[1]`, `fred[2]` — which the wire layer deliberately
/// keeps verbatim. Here they become one `fred` carrying both values,
/// ordered by index rather than by declaration, since a writer is free
/// to emit them out of order.
///
/// A lone `fred[1]` collapses to a scalar `fred`, which is what PSPP
/// means by "an attribute array that has a single element (number 1) is
/// not distinguished from a non-array attribute".
///
/// Attributes keep the order in which their first entry appeared, under
/// the spelling that first entry used. Names are matched
/// case-insensitively: PSPP treats an attribute name as an identifier,
/// and identifiers are not case-sensitive, so `fred[1]` and `FRED[2]`
/// are one attribute rather than two.
///
/// An entry whose name carries no index contributes its values as-is;
/// if an indexed and an unindexed spelling of the same name both
/// appear, they merge, with the unindexed values last.
#[must_use]
pub(crate) fn collapse(entries: &[VariableAttributeEntry]) -> Vec<VariableAttribute> {
    let mut groups: Vec<Group> = Vec::new();

    for entry in entries {
        let (base, index) = split_index(entry.name());
        // Resolve to an index rather than a reference: appending on the
        // miss needs the vec mutably, which a borrow held across the
        // lookup would rule out.
        let position = if let Some(found) = groups
            .iter()
            .position(|group| group.name.eq_ignore_ascii_case(base))
        {
            found
        } else {
            groups.push(Group::new(base));
            groups.len() - 1
        };
        let slot = &mut groups[position];
        match index {
            Some(index) => slot
                .indexed
                .extend(entry.values().iter().map(|value| (index, value.clone()))),
            None => slot.unindexed.extend(entry.values().iter().cloned()),
        }
    }

    groups.into_iter().map(Group::into_attribute).collect()
}

/// One attribute name's values, gathered across however many entries
/// spelled it, before ordering.
struct Group {
    name: String,
    /// Values that arrived under an `[n]` spelling, paired with `n`.
    indexed: Vec<(u32, String)>,
    /// Values that arrived under the bare name.
    unindexed: Vec<String>,
}

impl Group {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_owned(),
            indexed: Vec::new(),
            unindexed: Vec::new(),
        }
    }

    fn into_attribute(mut self) -> VariableAttribute {
        self.indexed.sort_by_key(|(index, _)| *index);
        let mut values: Vec<String> = self.indexed.into_iter().map(|(_, value)| value).collect();
        values.extend(self.unindexed);
        VariableAttribute::builder()
            .name(self.name)
            .values(values)
            .build()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, values: &[&str]) -> VariableAttributeEntry {
        let mut builder = VariableAttributeEntry::builder().name(name);
        for value in values {
            builder = builder.value(*value);
        }
        builder.build()
    }

    #[test]
    fn unindexed_attributes_pass_through() {
        let collapsed = collapse(&[entry("MyAttr", &["hello world"])]);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].name(), "MyAttr");
        assert_eq!(collapsed[0].values(), ["hello world"]);
    }

    #[test]
    fn indexed_entries_collapse_into_one_attribute() {
        let collapsed = collapse(&[entry("fred[1]", &["first"]), entry("fred[2]", &["second"])]);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].name(), "fred");
        assert_eq!(collapsed[0].values(), ["first", "second"]);
    }

    /// A writer is free to emit indices out of order; the collapsed
    /// values are ordered by index, not by declaration.
    /// PSPP: "an attribute array that has a single element (number 1)
    /// is not distinguished from a non-array attribute". So a lone
    /// `fred[1]` has to come out identical to a scalar `fred`.
    #[test]
    fn a_lone_index_one_collapses_to_a_scalar() {
        let indexed = collapse(&[entry("fred[1]", &["only"])]);
        let scalar = collapse(&[entry("fred", &["only"])]);
        assert_eq!(indexed, scalar);
        assert_eq!(indexed[0].name(), "fred");
        assert_eq!(indexed[0].value(), Some("only"));
    }

    /// Attribute names are identifiers, and PSPP identifiers are not
    /// case-sensitive, so spellings that differ only in case name one
    /// attribute.
    #[test]
    fn indexed_entries_group_case_insensitively() {
        let collapsed = collapse(&[entry("fred[1]", &["first"]), entry("FRED[2]", &["second"])]);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].values(), ["first", "second"]);
    }

    /// The first spelling seen is the one kept — the same
    /// first-declaration-wins rule variable names resolve under.
    #[test]
    fn the_first_spelling_of_a_name_is_the_one_kept() {
        let collapsed = collapse(&[entry("FrEd[1]", &["a"]), entry("fred[2]", &["b"])]);
        assert_eq!(collapsed[0].name(), "FrEd");
    }

    #[test]
    fn indexed_entries_sort_by_index() {
        let collapsed = collapse(&[entry("fred[3]", &["third"]), entry("fred[1]", &["first"])]);
        assert_eq!(collapsed[0].values(), ["first", "third"]);
    }

    #[test]
    fn distinct_attributes_keep_first_seen_order() {
        let collapsed = collapse(&[
            entry("$@Role", &["0"]),
            entry("fred[1]", &["a"]),
            entry("MyAttr", &["b"]),
            entry("fred[2]", &["c"]),
        ]);
        let names: Vec<&str> = collapsed.iter().map(VariableAttribute::name).collect();
        assert_eq!(names, ["$@Role", "fred", "MyAttr"]);
        assert_eq!(collapsed[1].values(), ["a", "c"]);
    }

    /// A sigil-prefixed name is not an index and must survive intact —
    /// `$@Role` is what SPSS writes on every variable.
    #[test]
    fn names_without_an_index_are_untouched() {
        for name in ["$@Role", "plain", "trailing]", "a[]", "a[x]", "a[1x]"] {
            let collapsed = collapse(&[entry(name, &["v"])]);
            assert_eq!(collapsed[0].name(), name, "name {name}");
        }
    }

    #[test]
    fn one_entry_may_carry_several_values() {
        let collapsed = collapse(&[entry("multi", &["a", "b"])]);
        assert_eq!(collapsed[0].values(), ["a", "b"]);
    }

    #[test]
    fn indexed_and_unindexed_spellings_merge_with_indexed_first() {
        let collapsed = collapse(&[entry("fred", &["bare"]), entry("fred[1]", &["indexed"])]);
        assert_eq!(collapsed.len(), 1);
        assert_eq!(collapsed[0].values(), ["indexed", "bare"]);
    }

    #[test]
    fn empty_input_yields_no_attributes() {
        assert!(collapse(&[]).is_empty());
    }
}
