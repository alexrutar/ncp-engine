use crate::chars::{AsciiChar, Char};
#[cfg(feature = "unicode-normalization")]
use crate::pattern::{CaseMatching, Normalization, Pattern};
use crate::score::{
    BONUS_BOUNDARY, BONUS_CAMEL123, BONUS_CONSECUTIVE, BONUS_FIRST_CHAR_MULTIPLIER, BONUS_NON_WORD,
    MAX_PREFIX_BONUS, PENALTY_GAP_EXTENSION, PENALTY_GAP_START, SCORE_MATCH,
};
use crate::utf32_str::Utf32Str;
use crate::{Config, Matcher};

use Algorithm::*;

#[derive(Debug)]
enum Algorithm {
    FuzzyOptimal,
    FuzzyGreedy,
    Substring,
    Prefix,
    Postfix,
    Exact,
}

fn assert_matches(
    algorithm: &[Algorithm],
    normalize: bool,
    case_sensitive: bool,
    path: bool,
    prefer_prefix: bool,
    cases: &[(&str, &str, &[u32], u16)],
) {
    let mut config = Config {
        normalize,
        ignore_case: !case_sensitive,
        prefer_prefix,
        ..Config::DEFAULT
    };
    if path {
        config.set_match_paths();
    }
    let mut matcher = Matcher::new(config);
    let mut matched_indices = Vec::new();
    let mut needle_buf = Vec::new();
    let mut haystack_buf = Vec::new();
    for &(haystack, needle, indices, mut score) in cases {
        let needle = if !case_sensitive {
            needle.to_lowercase()
        } else {
            needle.to_owned()
        };
        let needle = Utf32Str::new(&needle, &mut needle_buf);
        let haystack = Utf32Str::new(haystack, &mut haystack_buf);
        score += needle.len() as u16 * SCORE_MATCH;
        for algo in algorithm {
            println!("xx {matched_indices:?} {algo:?}");
            matched_indices.clear();
            let res = match algo {
                FuzzyOptimal => matcher.fuzzy_indices(haystack, needle, &mut matched_indices),
                FuzzyGreedy => matcher.fuzzy_indices_greedy(haystack, needle, &mut matched_indices),
                Substring => matcher.substring_indices(haystack, needle, &mut matched_indices),
                Prefix => matcher.prefix_indices(haystack, needle, &mut matched_indices),
                Postfix => matcher.postfix_indices(haystack, needle, &mut matched_indices),
                Exact => matcher.exact_indices(haystack, needle, &mut matched_indices),
            };
            println!("{matched_indices:?}");
            let match_chars: Vec<_> = matched_indices
                .iter()
                .map(|&i| match haystack {
                    Utf32Str::Ascii(bytes) => {
                        AsciiChar(bytes[i as usize]).normalize(&matcher.config).0 as char
                    }
                    Utf32Str::Unicode(chars) => chars[i as usize].normalize(&matcher.config),
                })
                .collect();
            let needle_chars: Vec<_> = needle.chars().collect();

            assert_eq!(
                res,
                Some(score),
                "{needle:?} did  not match {haystack:?}: matched {match_chars:?} {matched_indices:?} {algo:?}"
            );
            assert_eq!(
                matched_indices, indices,
                "{needle:?} match {haystack:?} {algo:?}"
            );
            assert_eq!(
                match_chars, needle_chars,
                "{needle:?} match {haystack:?} indices are incorrect {matched_indices:?} {algo:?}"
            );
        }
    }
}

fn assert_not_matches_with(
    normalize: bool,
    case_sensitive: bool,
    algorithm: &[Algorithm],
    cases: &[(&str, &str)],
) {
    let config = Config {
        normalize,
        ignore_case: !case_sensitive,
        ..Config::DEFAULT
    };
    let mut matcher = Matcher::new(config);
    let mut needle_buf = Vec::new();
    let mut haystack_buf = Vec::new();
    for &(haystack, needle) in cases {
        let needle = if !case_sensitive {
            needle.to_lowercase()
        } else {
            needle.to_owned()
        };
        let needle = Utf32Str::new(&needle, &mut needle_buf);
        let haystack = Utf32Str::new(haystack, &mut haystack_buf);

        for algo in algorithm {
            let res = match algo {
                FuzzyOptimal => matcher.fuzzy_match(haystack, needle),
                FuzzyGreedy => matcher.fuzzy_match_greedy(haystack, needle),
                Substring => matcher.substring_match(haystack, needle),
                Prefix => matcher.prefix_match(haystack, needle),
                Postfix => matcher.postfix_match(haystack, needle),
                Exact => matcher.exact_match(haystack, needle),
            };
            assert_eq!(
                res, None,
                "{needle:?} should not match {haystack:?} {algo:?}"
            );
        }
    }
}

pub fn assert_not_matches(normalize: bool, case_sensitive: bool, cases: &[(&str, &str)]) {
    assert_not_matches_with(
        normalize,
        case_sensitive,
        &[FuzzyOptimal, FuzzyGreedy, Substring, Prefix, Postfix, Exact],
        cases,
    )
}

const BONUS_BOUNDARY_WHITE: u16 = Config::DEFAULT.bonus_boundary_white;
const BONUS_BOUNDARY_DELIMITER: u16 = Config::DEFAULT.bonus_boundary_delimiter;

#[test]
fn test_fuzzy() {
    assert_matches(
        &[FuzzyGreedy, FuzzyOptimal],
        false,
        false,
        false,
        false,
        &[
            (
                "fooBarbaz1",
                "obr",
                &[2, 3, 5],
                BONUS_CAMEL123 - PENALTY_GAP_START,
            ),
            (
                "/usr/share/doc/at/ChangeLog",
                "changelog",
                &[18, 19, 20, 21, 22, 23, 24, 25, 26],
                (BONUS_FIRST_CHAR_MULTIPLIER + 8) * BONUS_BOUNDARY_DELIMITER,
            ),
            (
                "fooBarbaz1",
                "br",
                &[3, 5],
                BONUS_CAMEL123 * BONUS_FIRST_CHAR_MULTIPLIER - PENALTY_GAP_START,
            ),
            (
                "foo bar baz",
                "fbb",
                &[0, 4, 8],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER + BONUS_BOUNDARY_WHITE * 2
                    - 2 * PENALTY_GAP_START
                    - 4 * PENALTY_GAP_EXTENSION,
            ),
            (
                "/AutomatorDocument.icns",
                "rdoc",
                &[9, 10, 11, 12],
                BONUS_CAMEL123 + 2 * BONUS_CONSECUTIVE,
            ),
            (
                "/man1/zshcompctl.1",
                "zshc",
                &[6, 7, 8, 9],
                BONUS_BOUNDARY_DELIMITER * (BONUS_FIRST_CHAR_MULTIPLIER + 3),
            ),
            (
                "/.oh-my-zsh/cache",
                "zshc",
                &[8, 9, 10, 12],
                BONUS_BOUNDARY * (BONUS_FIRST_CHAR_MULTIPLIER + 2) - PENALTY_GAP_START
                    + BONUS_BOUNDARY_DELIMITER,
            ),
            (
                "ab0123 456",
                "12356",
                &[3, 4, 5, 8, 9],
                BONUS_CONSECUTIVE * 3 - PENALTY_GAP_START - PENALTY_GAP_EXTENSION,
            ),
            (
                "abc123 456",
                "12356",
                &[3, 4, 5, 8, 9],
                BONUS_CAMEL123 * (BONUS_FIRST_CHAR_MULTIPLIER + 2)
                    - PENALTY_GAP_START
                    - PENALTY_GAP_EXTENSION
                    + BONUS_CONSECUTIVE,
            ),
            (
                "foo/bar/baz",
                "fbb",
                &[0, 4, 8],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER + BONUS_BOUNDARY_DELIMITER * 2
                    - 2 * PENALTY_GAP_START
                    - 4 * PENALTY_GAP_EXTENSION,
            ),
            (
                "fooBarBaz",
                "fbb",
                &[0, 3, 6],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER + BONUS_CAMEL123 * 2
                    - 2 * PENALTY_GAP_START
                    - 2 * PENALTY_GAP_EXTENSION,
            ),
            (
                "foo barbaz",
                "fbb",
                &[0, 4, 7],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER + BONUS_BOUNDARY_WHITE
                    - PENALTY_GAP_START * 2
                    - PENALTY_GAP_EXTENSION * 3,
            ),
            (
                "fooBar Baz",
                "foob",
                &[0, 1, 2, 3],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 3),
            ),
            (
                "xFoo-Bar Baz",
                "foo-b",
                &[1, 2, 3, 4, 5],
                BONUS_CAMEL123 * (BONUS_FIRST_CHAR_MULTIPLIER + 2) + 2 * BONUS_NON_WORD,
            ),
        ],
    );
}

#[test]
fn empty_needle() {
    assert_matches(
        &[Substring, Prefix, Postfix, FuzzyGreedy, FuzzyOptimal, Exact],
        false,
        false,
        false,
        false,
        &[("foo bar baz", "", &[], 0)],
    );
}

#[test]
fn test_substring() {
    assert_matches(
        &[Substring],
        false,
        false,
        false,
        false,
        &[(
            "x12A",
            "12a",
            &[1, 2, 3],
            BONUS_CAMEL123 * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
        )],
    );
    assert_matches(
        &[Substring, Prefix],
        false,
        false,
        false,
        false,
        &[
            (
                "foo bar baz",
                "foo",
                &[0, 1, 2],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                " foo bar baz",
                "FOO",
                &[1, 2, 3],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                " foo bar baz",
                " FOO",
                &[0, 1, 2, 3],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 3),
            ),
        ],
    );
    assert_matches(
        &[Substring, Postfix],
        false,
        false,
        false,
        false,
        &[
            (
                "foo bar baz",
                "baz",
                &[8, 9, 10],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                "foo bar baz ",
                "baz",
                &[8, 9, 10],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                "foo bar baz ",
                "baz ",
                &[8, 9, 10, 11],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 3),
            ),
        ],
    );
    assert_matches(
        &[Substring, Prefix, Postfix, Exact, FuzzyGreedy, FuzzyOptimal],
        false,
        false,
        false,
        false,
        &[
            (
                "foo",
                "foo",
                &[0, 1, 2],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                " foo",
                "foo",
                &[1, 2, 3],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                " foo",
                " foo",
                &[0, 1, 2, 3],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 3),
            ),
        ],
    );
    assert_matches(
        &[Substring],
        false,
        false,
        false,
        false,
        &[
            (
                "fooBarbaz1",
                "oba",
                &[2, 3, 4],
                BONUS_CAMEL123 + BONUS_CONSECUTIVE,
            ),
            (
                "/AutomatorDocument.icns",
                "rdoc",
                &[9, 10, 11, 12],
                BONUS_CAMEL123 + 2 * BONUS_CONSECUTIVE,
            ),
            (
                "/man1/zshcompctl.1",
                "zshc",
                &[6, 7, 8, 9],
                BONUS_BOUNDARY_DELIMITER * (BONUS_FIRST_CHAR_MULTIPLIER + 3),
            ),
            (
                "/.oh-my-zsh/cache",
                "zsh/c",
                &[8, 9, 10, 11, 12],
                BONUS_BOUNDARY * (BONUS_FIRST_CHAR_MULTIPLIER + 2)
                    + BONUS_NON_WORD
                    + BONUS_BOUNDARY_DELIMITER,
            ),
        ],
    );
    assert_matches(
        &[Substring],
        false,
        false,
        false,
        false,
        &[
            (
                "x你x 你",
                "你",
                &[4],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER,
            ),
            (
                "x你好x 你好",
                "你好",
                &[5, 6],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1),
            ),
        ],
    );
    assert_not_matches_with(
        true,
        false,
        &[Prefix, Substring, Postfix, Exact],
        &[(
            "At the Road’s End - Seeming - SOL: A Self-Banishment Ritual",
            "adi",
        )],
    )
}

#[test]
fn test_substring_case_sensitive() {
    assert_matches(
        &[Substring, Prefix],
        false,
        true,
        false,
        false,
        &[
            (
                "Foo bar baz",
                "Foo",
                &[0, 1, 2],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                "Fȫô bar baz",
                "Fȫô",
                &[0, 1, 2],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
            (
                "Foo ฿ar baz",
                "Foo",
                &[0, 1, 2],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2),
            ),
        ],
    );
    assert_not_matches_with(false, true, &[Substring, Prefix], &[("foo bar baz", "Foo")]);
}

#[test]
fn test_fuzzy_case_sensitive() {
    assert_matches(
        &[FuzzyGreedy, FuzzyOptimal],
        false,
        true,
        false,
        false,
        &[
            (
                "fooBarbaz1",
                "oBr",
                &[2, 3, 5],
                BONUS_CAMEL123 - PENALTY_GAP_START,
            ),
            (
                "Foo/Bar/Baz",
                "FBB",
                &[0, 4, 8],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER + BONUS_BOUNDARY_DELIMITER * 2
                    - 2 * PENALTY_GAP_START
                    - 4 * PENALTY_GAP_EXTENSION,
            ),
            (
                "FooBarBaz",
                "FBB",
                &[0, 3, 6],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER + BONUS_CAMEL123 * 2
                    - 2 * PENALTY_GAP_START
                    - 2 * PENALTY_GAP_EXTENSION,
            ),
            (
                "FooBar Baz",
                "FooB",
                &[0, 1, 2, 3],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 3),
            ),
            ("foo-bar", "o-ba", &[2, 3, 4, 5], BONUS_NON_WORD * 3),
        ],
    );
}

#[test]
#[cfg(all(feature = "unicode-normalization", feature = "unicode-casefold"))]
fn test_normalize() {
    assert_matches(
        &[FuzzyGreedy, FuzzyOptimal],
        true,
        false,
        false,
        false,
        &[
            (
                "Só Danço Samba",
                "So",
                &[0, 1],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1),
            ),
            (
                "Só Danço Samba",
                "sodc",
                &[0, 1, 3, 6],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1) - PENALTY_GAP_START
                    + BONUS_BOUNDARY_WHITE
                    - PENALTY_GAP_START
                    - PENALTY_GAP_EXTENSION,
            ),
            (
                "Danço",
                "danco",
                &[0, 1, 2, 3, 4],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 4),
            ),
            (
                "DanÇo",
                "danco",
                &[0, 1, 2, 3, 4],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 4),
            ),
            (
                "xÇando",
                "cando",
                &[1, 2, 3, 4, 5],
                BONUS_CAMEL123 * (BONUS_FIRST_CHAR_MULTIPLIER + 4),
            ),
            ("ۂ(GCGɴCG", "n", &[5], 0),
        ],
    )
}

#[test]
fn test_unicode() {
    assert_matches(
        &[FuzzyGreedy, FuzzyOptimal, Substring],
        true,
        false,
        false,
        false,
        &[
            (
                "你好世界",
                "你好",
                &[0, 1],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1),
            ),
            (
                " 你好世界",
                "你好",
                &[1, 2],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1),
            ),
        ],
    );
    assert_matches(
        &[FuzzyGreedy, FuzzyOptimal],
        true,
        false,
        false,
        false,
        &[(
            "你好世界",
            "你世",
            &[0, 2],
            BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER - PENALTY_GAP_START,
        )],
    );
    assert_matches(
        &[Substring],
        false,
        false,
        false,
        false,
        &[("世你好", "你好", &[1, 2], BONUS_CONSECUTIVE)],
    );
    assert_not_matches(
        false,
        false,
        &[("Flibbertigibbet / イタズラっ子たち", "lying")],
    );
}

#[test]
fn test_long_str() {
    assert_matches(
        &[FuzzyGreedy, FuzzyOptimal],
        false,
        false,
        false,
        false,
        &[(
            &"x".repeat(u16::MAX as usize + 1),
            "xx",
            &[0, 1],
            BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1),
        )],
    );
}

#[test]
fn test_casing() {
    assert_matches(
        &[FuzzyGreedy, FuzzyOptimal],
        false,
        false,
        false,
        false,
        &[
            // these two have the same score
            (
                "fooBar",
                "foobar",
                &[0, 1, 2, 3, 4, 5],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 5),
            ),
            (
                "foobar",
                "foobar",
                &[0, 1, 2, 3, 4, 5],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 5),
            ),
            // these two have the same score (slightly lower than the other two: 60 instead of 70)
            (
                "foo-bar",
                "foobar",
                &[0, 1, 2, 4, 5, 6],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2) - PENALTY_GAP_START
                    + BONUS_BOUNDARY * 3,
            ),
            (
                "foo_bar",
                "foobar",
                &[0, 1, 2, 4, 5, 6],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2) - PENALTY_GAP_START
                    + BONUS_BOUNDARY * 3,
            ),
        ],
    )
}

#[test]
fn test_optimal() {
    assert_matches(
        &[FuzzyOptimal],
        false,
        false,
        false,
        false,
        &[
            (
                "axxx xx ",
                "xx",
                &[5, 6],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1),
            ),
            (
                "SS!H",
                "S!",
                &[0, 2],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER - PENALTY_GAP_START
                    + BONUS_NON_WORD,
            ),
            // this case is a cool example of why our algorithm is more than fzf
            // we handle this corretly detect that it's better to match
            // the second f instead of the third yielding a higher score
            // (despite using the same scoring function!)
            (
                "xf.foo",
                "xfoo",
                &[0, 3, 4, 5],
                BONUS_BOUNDARY_WHITE * BONUS_FIRST_CHAR_MULTIPLIER
                    - PENALTY_GAP_START
                    - PENALTY_GAP_EXTENSION
                    + BONUS_BOUNDARY * 3,
            ),
            (
                "xf fo",
                "xfo",
                &[0, 3, 4],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 2)
                    - PENALTY_GAP_START
                    - PENALTY_GAP_EXTENSION,
            ),
        ],
    );
}

#[test]
fn test_reject() {
    assert_not_matches(
        true,
        false,
        &[
            ("你好界", "abc"),
            ("你好界", "a"),
            ("你好世界", "富"),
            ("Só Danço Samba", "sox"),
            ("fooBarbaz", "fooBarbazz"),
            ("fooBarbaz", "c"),
        ],
    );
    assert_not_matches(
        true,
        true,
        &[
            ("你好界", "abc"),
            ("abc", "你"),
            ("abc", "A"),
            ("abc", "d"),
            ("你好世界", "富"),
            ("Só Danço Samba", "sox"),
            ("fooBarbaz", "oBZ"),
            ("Foo Bar Baz", "fbb"),
            ("fooBarbaz", "fooBarbazz"),
        ],
    );
    assert_not_matches(
        false,
        true,
        &[
            ("Só Danço Samba", "sod"),
            ("Só Danço Samba", "soc"),
            ("Só Danç", "So"),
        ],
    );
    assert_not_matches(false, false, &[("ۂۂfoۂۂ", "foo")]);
}

#[test]
fn test_prefer_prefix() {
    assert_matches(
        &[FuzzyOptimal, FuzzyGreedy],
        false,
        false,
        false,
        true,
        &[
            (
                "Moby Dick",
                "md",
                &[0, 5],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1) + MAX_PREFIX_BONUS
                    - PENALTY_GAP_START
                    - 3 * PENALTY_GAP_EXTENSION,
            ),
            (
                "Though I cannot tell why it was exactly that those stage managers, the Fates, put me down for this shabby part of a whaling voyage",
                "md",
                &[82, 85],
                BONUS_BOUNDARY_WHITE * (BONUS_FIRST_CHAR_MULTIPLIER + 1)
                    - PENALTY_GAP_START
                    - PENALTY_GAP_EXTENSION,
            ),
        ],
    );
}

#[test]
fn prefix_score_uses_selected_start() {
    use crate::pattern::{CaseMatching, Normalization, Pattern};

    let mut matcher = Matcher::new(Config::DEFAULT);
    for prefer_prefix in [false, true] {
        matcher.config.prefer_prefix = prefer_prefix;
        for query in ["lens", "'lens"] {
            let pattern = Pattern::parse(query, CaseMatching::default(), Normalization::Never);
            // The unused initial 'l' must not affect the suffix's bonus.
            for (candidate, start, bonus) in [
                ("code_lens", 5, 4),
                ("lsp_code_lens", 9, 2),
                ("xsp_code_lens", 9, 2),
                ("xxxxxxxxxxxxx_lens", 14, 0),
            ] {
                let haystack = Utf32Str::Ascii(candidate.as_bytes());
                let expected = Some(104 + if prefer_prefix { bonus } else { 0 });
                let mut indices = Vec::new();
                assert_eq!(pattern.score(haystack, &mut matcher), expected);
                assert_eq!(
                    pattern.indices(haystack, &mut matcher, &mut indices),
                    expected
                );
                assert_eq!(indices, [start, start + 1, start + 2, start + 3]);
            }
        }
    }
}

#[test]
fn prefix_bonus_survives_saturating_gaps() {
    let mut matcher = Matcher::new(Config::DEFAULT);
    let needle = Utf32Str::Ascii(b"ab");
    for candidate in [
        format!("a_a{}b", "x".repeat(50)),
        format!("a{}b", "x".repeat(5000)),
        format!("a{}b", "x".repeat(65536)), // Greedy fallback.
    ] {
        let haystack = Utf32Str::Ascii(candidate.as_bytes());
        matcher.config.prefer_prefix = false;
        assert_eq!(matcher.fuzzy_match(haystack, needle), Some(16));

        // Both starts lose their structural score in the gap; the earlier start
        // must still win with its full prefix bonus, including during traceback.
        matcher.config.prefer_prefix = true;
        let mut indices = Vec::new();
        assert_eq!(matcher.fuzzy_match(haystack, needle), Some(24));
        assert_eq!(
            matcher.fuzzy_indices(haystack, needle, &mut indices),
            Some(24)
        );
        assert_eq!(indices, [0, candidate.len() as u32 - 1]);
    }
}

#[test]
fn protected_floor_matches_exhaustive_alignments() {
    fn alignment_score(config: &Config, haystack: &[char], indices: &[u32]) -> u16 {
        let bonus = |pos: usize| {
            let prev = pos.checked_sub(1).map_or(config.initial_char_class, |i| {
                haystack[i].char_class(config)
            });
            config.bonus_for(prev, haystack[pos].char_class(config))
        };
        let start = indices[0] as usize;
        let mut carry = bonus(start);
        let mut score = SCORE_MATCH + 2 * carry;
        for pair in indices.windows(2) {
            let gap = pair[1] - pair[0] - 1;
            let mut b = bonus(pair[1] as usize);
            if gap == 0 {
                if b >= BONUS_BOUNDARY {
                    carry = carry.max(b);
                }
                b = b.max(carry).max(BONUS_CONSECUTIVE);
            } else {
                score = score.saturating_sub(PENALTY_GAP_START + (gap as u16 - 1));
                carry = b;
            }
            score += SCORE_MATCH + b;
        }
        score
            + [8, 6, 6, 5, 5, 4, 4, 3, 3, 2, 2, 1, 1]
                .get(start)
                .copied()
                .unwrap_or(0)
    }

    fn enumerate(
        config: &Config,
        haystack: &[char],
        needle: &[char],
        indices: &mut Vec<u32>,
        best: &mut Option<u16>,
    ) {
        if indices.len() == needle.len() {
            let score = alignment_score(config, haystack, indices);
            *best = Some(best.unwrap_or(0).max(score));
            return;
        }
        let start = indices.last().map_or(0, |&i| i as usize + 1);
        for i in start..haystack.len() {
            if haystack[i].normalize(config) == needle[indices.len()] {
                indices.push(i as u32);
                enumerate(config, haystack, needle, indices, best);
                indices.pop();
            }
        }
    }

    let mut cases = vec![
        ("aB _abBBAaA_bbbba".to_owned(), "abbbaabbba".to_owned()),
        (
            "_ba /a bbaAbb/ / /ab_aaba".to_owned(),
            "baabbabba".to_owned(),
        ),
        (
            format!("a_a{}b_b{}c", "x".repeat(50), "x".repeat(50)),
            "abc".to_owned(),
        ),
        (format!("_a___a{}b", "x".repeat(50)), "ab".to_owned()),
        ("é_你_你好".to_owned(), "你好".to_owned()),
        ("你_好_世".to_owned(), "你界世".to_owned()),
    ];
    // Enumerate alignments independently of the DP, including repeated starts,
    // competing continuation bonuses, and haystacks that reuse the same slab.
    let mut seed = 92u64;
    for _ in 0..2048 {
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as usize
        };
        let haystack = (0..4 + next() % 8)
            .map(|_| b"abAB_/ "[next() % 7] as char)
            .collect();
        let needle = (0..2 + next() % 3)
            .map(|_| b"ab"[next() % 2] as char)
            .collect();
        cases.push((haystack, needle));
    }
    for config in [Config::DEFAULT, Config::DEFAULT.match_paths()] {
        let mut matcher = Matcher::new(config);
        matcher.config.prefer_prefix = true;
        // Without case folding, exercise the same recurrence case-sensitively.
        matcher.config.ignore_case = cfg!(feature = "unicode-casefold");
        for (candidate, query) in &cases {
            let chars: Vec<_> = candidate.chars().collect();
            let needle_chars: Vec<_> = query.chars().collect();
            let needle = Utf32Str::Unicode(&needle_chars);
            let mut expected = None;
            enumerate(
                &matcher.config,
                &chars,
                &needle_chars,
                &mut Vec::new(),
                &mut expected,
            );
            let mut forms = vec![Utf32Str::Unicode(&chars)];
            let ascii_needle = query.is_ascii().then(|| Utf32Str::Ascii(query.as_bytes()));
            if candidate.is_ascii() {
                forms.push(Utf32Str::Ascii(candidate.as_bytes()));
            }
            for haystack in forms {
                let needle = ascii_needle.unwrap_or(needle);
                matcher.config.prefer_prefix = false;
                matcher.fuzzy_match(haystack, needle);
                matcher.config.prefer_prefix = true;
                let mut indices = vec![u32::MAX];
                let score = matcher.fuzzy_match(haystack, needle);
                assert_eq!(score, expected, "{candidate:?} / {query:?}");
                assert_eq!(matcher.fuzzy_indices(haystack, needle, &mut indices), score);
                assert_eq!(indices.remove(0), u32::MAX);
                if let Some(score) = score {
                    assert_eq!(indices.len(), needle.len());
                    assert!(indices.windows(2).all(|pair| pair[0] < pair[1]));
                    for (&index, &c) in indices.iter().zip(&needle_chars) {
                        assert_eq!(chars[index as usize].normalize(&matcher.config), c);
                    }
                    assert_eq!(alignment_score(&matcher.config, &chars, &indices), score);
                } else {
                    assert!(indices.is_empty());
                }
            }
        }
    }
}

#[test]
fn test_single_char_needle() {
    assert_matches(
        &[FuzzyOptimal],
        false,
        false,
        false,
        false,
        &[(
            "foO",
            "o",
            &[2],
            BONUS_FIRST_CHAR_MULTIPLIER * BONUS_CAMEL123,
        )],
    );
    #[cfg(feature = "unicode-casefold")]
    assert_matches(
        &[FuzzyOptimal],
        false,
        false,
        false,
        false,
        &[(
            "föÖ",
            "ö",
            &[2],
            BONUS_FIRST_CHAR_MULTIPLIER * BONUS_CAMEL123,
        )],
    );
}

#[test]
#[cfg(feature = "unicode-normalization")]
fn umlaut() {
    let paths = ["be", "bë"];
    let mut matcher = Matcher::new(Config::DEFAULT);
    let matches = Pattern::parse("ë", CaseMatching::default(), Normalization::Smart)
        .match_list(paths, &mut matcher);
    assert_eq!(matches.len(), 1);
    let matches = Pattern::parse("e", CaseMatching::default(), Normalization::Never)
        .match_list(paths, &mut matcher);
    assert_eq!(matches.len(), 1);
    let matches = Pattern::parse("e", CaseMatching::default(), Normalization::Smart)
        .match_list(paths, &mut matcher);
    assert_eq!(matches.len(), 2);
}
