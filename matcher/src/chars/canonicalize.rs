/// Canonicalize a grapheme to a single char.
///
/// For the most part, this is simply the first `char` of a grapheme. The main exceptions are:
///
/// - The windows-style newline `\r\n`, which is normalized to the char `'\n'`.
///
/// The input string is not checked to actually correspond to a grapheme. To split a string into
/// graphemes, you might want to use the [`unicode_segmentation`] crate.
pub fn canonicalize_latin(grapheme: &str) -> char {
    if grapheme == "\r\n" {
        '\n'
    } else {
        grapheme
            .chars()
            .next()
            .expect("graphemes must be non-empty")
    }
}
