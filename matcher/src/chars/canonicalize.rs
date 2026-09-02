/// Canonicalize a grapheme to a single char.
///
/// For the most part, this is simply the first `char` of a grapheme. The two exceptions are:
///
/// - The windows-style newline `\r\n`, which is normalized to the char `'\n'`.
/// - A two-codepoint grapheme which has a canonical Unicode composition, which is normalized to
///   the composed character.
///
/// The input string is not checked to actually correspond to a grapheme. To split a string into
/// graphemes, you might want to use the [`unicode_segmentation`] crate.
pub fn canonicalize_latin(grapheme: &str) -> char {
    if grapheme == "\r\n" {
        return '\n';
    }

    let mut chars = grapheme.chars();
    let first = chars.next().expect("graphemes must be non-empty");
    let Some(second) = chars.next() else {
        return first;
    };

    if chars.next().is_some() {
        return first;
    }

    unicode_normalization::char::compose(first, second).unwrap_or(first)
}

#[cfg(test)]
mod tests {
    use super::canonicalize_latin;

    #[test]
    fn canonicalizes_composable_pairs() {
        assert_eq!(canonicalize_latin("a\u{0308}"), 'ä');
        assert_eq!(canonicalize_latin("は\u{3099}"), 'ば');
    }

    #[test]
    fn preserves_existing_fallbacks() {
        assert_eq!(canonicalize_latin("\r\n"), '\n');
        assert_eq!(canonicalize_latin("q\u{0308}"), 'q');
        assert_eq!(canonicalize_latin("a\u{0308}\u{0301}"), 'a');
        assert_eq!(canonicalize_latin("ä"), 'ä');
    }

    #[test]
    #[should_panic(expected = "graphemes must be non-empty")]
    fn rejects_empty_input() {
        canonicalize_latin("");
    }
}
