use crate::pattern::{Atom, AtomKind, CaseMatching, Normalization, Pattern};
use crate::{Matcher, Utf32String};

#[test]
fn canonically_equivalent_needles() {
    let nfc = Atom::new(
        "ä",
        CaseMatching::Respect,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );
    let nfd = Atom::new(
        "a\u{0308}",
        CaseMatching::Respect,
        Normalization::Smart,
        AtomKind::Fuzzy,
        false,
    );

    assert_eq!(nfc.needle, nfd.needle);
    assert_eq!(nfd.needle.to_string(), "ä");
    assert!(!nfd.normalize);

    let nfc_haystack = Utf32String::from("ä");
    let nfd_haystack = Utf32String::from("a\u{0308}");
    let plain_haystack = Utf32String::from("a");
    let acute_haystack = Utf32String::from("á");
    let mut matcher = Matcher::default();

    assert!(nfd.score(nfc_haystack.slice(..), &mut matcher).is_some());
    assert!(nfd.score(nfd_haystack.slice(..), &mut matcher).is_some());
    assert!(nfd.score(plain_haystack.slice(..), &mut matcher).is_none());
    assert!(nfd.score(acute_haystack.slice(..), &mut matcher).is_none());
}

#[test]
fn negative() {
    let pat = Atom::parse("!foo", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Substring);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("!^foo", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Prefix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("!foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Postfix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("!^foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.negative);
    assert_eq!(pat.kind, AtomKind::Exact);
    assert_eq!(pat.needle.to_string(), "foo");
}

#[test]
fn pattern_kinds() {
    let pat = Atom::parse("foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("'foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Substring);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("^foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Prefix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Postfix);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("^foo$", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.negative);
    assert_eq!(pat.kind, AtomKind::Exact);
    assert_eq!(pat.needle.to_string(), "foo");
}

#[test]
fn case_matching() {
    let pat = Atom::parse("foo", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("Foo", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "Foo");
    let pat = Atom::parse("Foo", CaseMatching::Ignore, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "foo");
    let pat = Atom::parse("Foo", CaseMatching::Respect, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "Foo");
    let pat = Atom::parse("Foo", CaseMatching::Respect, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "Foo");
    let pat = Atom::parse("Äxx", CaseMatching::Ignore, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "äxx");
    let pat = Atom::parse("Äxx", CaseMatching::Respect, Normalization::Smart);
    assert!(!pat.ignore_case);
    let pat = Atom::parse("Axx", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "Axx");
    let pat = Atom::parse("你xx", CaseMatching::Smart, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "你xx");
    let pat = Atom::parse("你xx", CaseMatching::Ignore, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "你xx");
    let pat = Atom::parse("Ⲽxx", CaseMatching::Smart, Normalization::Smart);
    assert!(!pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "Ⲽxx");
    let pat = Atom::parse("Ⲽxx", CaseMatching::Ignore, Normalization::Smart);
    assert!(pat.ignore_case);
    assert_eq!(pat.needle.to_string(), "ⲽxx");
}

#[test]
fn escape() {
    // escapes only impact whitespace
    let pat = Atom::parse("foo\\ bar", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "foo bar");
    let pat = Atom::parse("foo\\\tbar", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "foo\tbar");
    let pat = Atom::parse("\\", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "\\");
    let pat = Atom::parse("\\ ", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), " ");
    let pat = Atom::parse("\\\\", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "\\\\");

    // some unicode checks
    let pat = Atom::parse("foö\\ bar", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "foö bar");
    let pat = Atom::parse("ö\\ ", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "ö ");
    let pat = Atom::parse("foö\\\\ bar", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "foö\\ bar");
    let pat = Atom::parse("foo\\　bar", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "foo　bar"); // double-width IDEOGRAPHIC SPACE
    let pat = Atom::parse("ö\\b", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "ö\\b");
    let pat = Atom::parse("ö\\\\", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "ö\\\\");
    let pat = Atom::parse("\\!^foö\\$", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "!^foö$");
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    let pat = Atom::parse("!\\^foö\\$", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "^foö$");
    assert_eq!(pat.kind, AtomKind::Substring);

    let pat = Atom::parse("\\!foo", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "!foo");
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    let pat = Atom::parse("\\'foo", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "'foo");
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    let pat = Atom::parse("\\^foo", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "^foo");
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    let pat = Atom::parse("foo\\$", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "foo$");
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    let pat = Atom::parse("^foo\\$", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "foo$");
    assert_eq!(pat.kind, AtomKind::Prefix);
    let pat = Atom::parse("\\^foo\\$", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "^foo$");
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    let pat = Atom::parse("\\!^foo\\$", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "!^foo$");
    assert_eq!(pat.kind, AtomKind::Fuzzy);
    let pat = Atom::parse("!\\^foo\\$", CaseMatching::Smart, Normalization::Smart);
    assert_eq!(pat.needle.to_string(), "^foo$");
    assert_eq!(pat.kind, AtomKind::Substring);
}

#[test]
fn pattern_atoms() {
    assert_eq!(
        Pattern::parse("a b", CaseMatching::Ignore, Normalization::Smart).atoms,
        vec![
            Atom::parse("a", CaseMatching::Ignore, Normalization::Smart),
            Atom::parse("b", CaseMatching::Ignore, Normalization::Smart),
        ]
    );

    assert_eq!(
        Pattern::parse("a\n b", CaseMatching::Ignore, Normalization::Smart).atoms,
        vec![
            Atom::parse("a", CaseMatching::Ignore, Normalization::Smart),
            Atom::parse("b", CaseMatching::Ignore, Normalization::Smart),
        ]
    );

    assert_eq!(
        Pattern::parse("  a b\r\n", CaseMatching::Ignore, Normalization::Smart).atoms,
        vec![
            Atom::parse("a", CaseMatching::Ignore, Normalization::Smart),
            Atom::parse("b", CaseMatching::Ignore, Normalization::Smart),
        ]
    );

    assert_eq!(
        Pattern::parse("ほ　げ", CaseMatching::Smart, Normalization::Smart).atoms,
        vec![
            Atom::parse("ほ", CaseMatching::Smart, Normalization::Smart),
            Atom::parse("げ", CaseMatching::Smart, Normalization::Smart),
        ],
    )
}
