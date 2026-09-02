use unicode_normalization::{UnicodeNormalization, char::compose};

/// Canonicalize a grapheme to a single char.
///
/// This implementation takes the first `char` of the grapheme's NFC-normalized form, with one
/// exception: the windows-style newline `\r\n`, which is normalized to the char `'\n'`. This
/// isn't the most robust implementation, but it works correctly for Latin script (hence the name)
/// and also reasonably well for languaes like Vietnamese, Hangul Jamo, and voiced kana marks.
///
/// Note that the input string is not checked to actually correspond to a grapheme. In particular,
/// the implementation will panic if `grapheme` is empty. To split a string into graphemes,
/// you might want to use the [`unicode_segmentation`] crate.
pub fn canonicalize_latin(grapheme: &str) -> char {
    if grapheme == "\r\n" {
        return '\n';
    }

    let mut chars = grapheme.chars();
    let first = chars.next().expect("graphemes must be non-empty");
    let Some(second) = chars.next() else {
        return first;
    };

    if chars.next().is_none() {
        return compose(first, second).unwrap_or(first);
    }

    grapheme.nfc().next().expect("graphemes must be non-empty")
}

#[cfg(test)]
mod tests {
    use super::canonicalize_latin;

    #[test]
    fn canonicalizes_composable_graphemes() {
        assert_eq!(canonicalize_latin("a\u{0308}"), 'ä');
        assert_eq!(canonicalize_latin("は\u{3099}"), 'ば');
        assert_eq!(canonicalize_latin("a\u{0302}\u{0301}"), 'ấ');
        assert_eq!(canonicalize_latin("a\u{0315}\u{0300}"), 'à');
        assert_eq!(canonicalize_latin("\u{1112}\u{1161}\u{11ab}"), '한');
    }

    #[test]
    fn preserves_existing_fallbacks() {
        assert_eq!(canonicalize_latin("\r\n"), '\n');
        assert_eq!(canonicalize_latin("q\u{0308}"), 'q');
        assert_eq!(canonicalize_latin("q\u{0308}\u{0301}"), 'q');
        assert_eq!(canonicalize_latin("👩\u{200d}💻"), '👩');
        assert_eq!(canonicalize_latin("ä"), 'ä');
    }

    #[test]
    #[should_panic(expected = "graphemes must be non-empty")]
    fn rejects_empty_input() {
        canonicalize_latin("");
    }
}
