// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The pattern vectors an independent implementation checks.
//!
//! They hold patterns revision `"0"` compiles, each with strings it matches
//! and strings it does not; the decisions of more patterns on every string of
//! up to three code points over a small alphabet; and patterns it refuses,
//! each with its failure. The Node checker compiles every pattern with
//! JavaScript's own `RegExp` and the `u` flag, the ECMA-262 implementation the
//! subset is drawn from, so a disagreement says whether this crate or the
//! subset's boundary is wrong.

use serde_json::{json, Value};

use crate::construction::validator::pattern::{Construct, Pattern, PatternFailure, MAX_NESTING};

/// The code points every exhaustive string is made of: two letters, two
/// punctuation marks, a space, a line terminator, a letter past ASCII, and an
/// astral code point.
pub const ALPHABET: [char; 8] = ['a', 'b', '_', '-', ' ', '\n', '\u{E9}', '\u{1F600}'];

/// The most code points an exhaustive string holds.
pub const LONGEST: usize = 3;

/// Patterns with the strings chosen to try them.
const CHOSEN: &[(&str, &str, &[&str])] = &[
    ("unanchored", "es", &["expression", "es", "e s", "", "ES"]),
    ("anchors", "^ab$", &["ab", "abc", "cab", "\nab", "ab\n", ""]),
    (
        "anchors-in-alternatives",
        "(?:^a|b$)",
        &["ac", "cb", "ca", "bc"],
    ),
    (
        "dot",
        "^.$",
        &[
            "a",
            "\n",
            "\r",
            "\u{2028}",
            "\u{2029}",
            "\u{85}",
            "\u{1F600}",
            "",
            "ab",
        ],
    ),
    (
        "digit",
        r"^\d+$",
        &["0123456789", "\u{663}", "\u{FF11}", "a1", ""],
    ),
    (
        "word",
        r"^\w+$",
        &["az_AZ09", "\u{E9}", "\u{17F}", "\u{212A}", "a-b"],
    ),
    (
        "space",
        r"^\s$",
        &[
            "\t", "\n", "\u{B}", "\u{C}", "\r", " ", "\u{A0}", "\u{1680}", "\u{2000}", "\u{200A}",
            "\u{2028}", "\u{2029}", "\u{202F}", "\u{205F}", "\u{3000}", "\u{FEFF}", "\u{85}",
            "\u{180E}", "\u{200B}", "a",
        ],
    ),
    ("not-space", r"^\S$", &["a", " ", "\u{FEFF}", "\u{85}"]),
    (
        "word-boundary",
        r"\bfoo\b",
        &["foo", "a foo b", "foobar", "\u{E9}foo\u{E9}", "_foo"],
    ),
    (
        "not-word-boundary",
        r"\Boo\B",
        &["foo", "fooa", "oo", " oo "],
    ),
    (
        "astral-code-point",
        r"^\u{1F600}+$",
        &["\u{1F600}", "\u{1F600}\u{1F600}", "\u{1F601}", ""],
    ),
    (
        "surrogate-pair-escape",
        r"^\uD83D\uDE00$",
        &["\u{1F600}", "a"],
    ),
    ("lone-surrogate-escape", r"\uD800", &["a", "\u{1F600}", ""]),
    // Between the two surrogates of a pair is no position: `\B` does not
    // hold beside the astral code point, and holds between two of them.
    (
        "not-word-boundary-beside-astral",
        r"\B",
        &["a\u{1F600}a", "\u{1F600}\u{1F600}", "ab", "a b"],
    ),
    ("class-ranges", "^[a-cx-z]+$", &["abc", "xyz", "d", "w", ""]),
    ("negated-class", "^[^a-c]$", &["d", "a", "\n", "\u{1F600}"]),
    ("dash-after", "^[a-]$", &["-", "a", "b"]),
    ("dash-before", "^[-a]$", &["-", "a", "b"]),
    ("dash-after-escape", r"^[\w-]$", &["-", "_", "\u{E9}"]),
    ("empty-class", "[]", &["", "a"]),
    ("any-class", "^[^]$", &["\n", "a", "\u{1F600}", "", "ab"]),
    ("class-escapes", r"^[\b\-\]]$", &["\u{8}", "-", "]", "b"]),
    ("class-literals", r"^[$^[(]+$", &["$^[(", "]", ")"]),
    (
        "control-escapes",
        r"^\t\n\v\f\r$",
        &["\t\n\u{B}\u{C}\r", "tnvfr"],
    ),
    ("control-letters", r"^\cJ\cj$", &["\n\n", "cJcj"]),
    (
        "hexadecimal-escapes",
        r"^\x41\u0042\u{43}$",
        &["ABC", "abc"],
    ),
    ("nul", r"^\0$", &["\0", "0"]),
    (
        "identity-escapes",
        r"^\^\$\\\.\*\+\?\(\)\[\]\{\}\|\/$",
        &[r"^$\.*+?()[]{}|/", "x"],
    ),
    ("counted", "^a{2,3}$", &["a", "aa", "aaa", "aaaa"]),
    ("counted-exactly", "^a{2}$", &["a", "aa", "aaa"]),
    ("counted-unbounded", "^a{2,}$", &["a", "aa", "aaaaa"]),
    ("lazy", "^a+?b*?c??$", &["a", "abbc", "ac", "bc"]),
    (
        "nested-quantifiers",
        "^(?:a|b{2})*$",
        &["", "abba", "ab", "bb", "bbb"],
    ),
    ("empty-loop", "^(?:a?)*$", &["", "aaa", "b"]),
    (
        "empty-alternative",
        "^(?:|a|bc)$",
        &["", "a", "bc", "b", "abc"],
    ),
    ("groups", "^(a)(?:b)(c)$", &["abc", "ab"]),
    (
        "nested-loops",
        "^(a+)+$",
        &["aaaaaaaaaaaaaaaa", "aaaaaaaaaaaaaaab"],
    ),
    (
        "configuration-token",
        "^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$",
        &["a", "a.b", "a-", "-a", "A", "a_b-c.d", ""],
    ),
    (
        "decimal-bound",
        "^[1-9][0-9]{0,19}$",
        &[
            "1",
            "0",
            "12345678901234567890",
            "123456789012345678901",
            "01",
        ],
    ),
];

/// Patterns decided on every exhaustive string.
const EXHAUSTIVE: &[&str] = &[
    "",
    "a",
    "^a",
    "a$",
    "^a$",
    "^$",
    "ab|b_",
    "^(?:a|b)*$",
    ".",
    "^.$",
    "^..$",
    "^...$",
    "^[^a]*$",
    r"\w",
    r"\W",
    r"^\w*$",
    r"\s",
    r"^\S+$",
    r"\b",
    r"\B",
    r"a\b",
    r"\ba",
    r"^\b",
    r"\B$",
    "^[a-b_]{2}$",
    "^a{0,2}$",
    "^(?:a|)b?$",
    "^(?:a*b)+$",
    r"^[\s-]+$",
    r"[\u{1F600}\u{E9}]",
    r"^[^\n]{3}$",
    r"^(?:a|b|_|-)?(?:\n|$)",
    r"^(?:.|\n){2,}$",
    r"[^\w\s]",
    "^(?:a?b?)*_$",
    r"\u{1F600}$",
    r"^\u{E9}",
    "a{2,}",
    "(?:^|_)a",
    r"^(?:\B.)+$",
];

/// Patterns refused, with their failure.
fn refused() -> Vec<(String, PatternFailure)> {
    let syntax = [
        "(",
        ")",
        "a)",
        "[",
        "[a",
        "[a-",
        "a{",
        "a{1",
        "a{1,",
        "a{1,2",
        "a{,1}",
        "a{a}",
        "{",
        "}",
        "]",
        "*",
        "+",
        "?",
        "a**",
        "a{1}{2}",
        "a???",
        "a{2,1}",
        r"\",
        r"\a",
        r"\-",
        r"\c",
        r"\c1",
        r"\x4",
        r"\x4g",
        r"\u12",
        r"\u{}",
        r"\u{110000}",
        r"\u{41",
        r"\01",
        "[b-a]",
        r"[\w-a]",
        r"[a-\d]",
        r"[\B]",
        r"[\1]",
        r"[\k<a>]",
        "^*",
        "$+",
        r"\b+",
        r"\B*",
        "(?",
        "(?x)",
        "(?-:a)",
        "(?ii:a)",
        "(?i-i:a)",
        "(?<1a>x)",
        "(?<>x)",
        r"\k<a>",
        r"(?<b>x)\k<a>",
        r"\2(a)",
        r"\p",
        r"\p{}",
        "(?=a)*",
        "(?<=a)+",
    ];
    let unsupported = [
        ("(?=a)", Construct::Lookaround),
        ("(?!a)", Construct::Lookaround),
        ("(?<=a)", Construct::Lookaround),
        ("(?<!a)", Construct::Lookaround),
        (r"(a)\1", Construct::Backreference),
        (r"\1(a)", Construct::Backreference),
        (r"\k<n>(?<n>a)", Construct::Backreference),
        ("(?<n>a)", Construct::NamedGroup),
        (r"(?<n>a)\k<n>", Construct::NamedGroup),
        ("(?i:a)", Construct::Modifiers),
        ("(?-i:a)", Construct::Modifiers),
        ("(?ims-:a)", Construct::Modifiers),
        (r"\p{L}", Construct::PropertyEscape),
        (r"\P{Lu}", Construct::PropertyEscape),
        (r"[\p{Script=Greek}]", Construct::PropertyEscape),
    ];
    let nested = |depth: usize| format!("{}a{}", "(".repeat(depth), ")".repeat(depth));
    let mut refused: Vec<(String, PatternFailure)> = Vec::new();
    refused.push((nested(MAX_NESTING + 1), PatternFailure::Nesting));
    refused.push((
        format!("(?:{})", nested(MAX_NESTING)),
        PatternFailure::Nesting,
    ));
    refused.extend(
        syntax
            .iter()
            .map(|source| ((*source).to_owned(), PatternFailure::Syntax)),
    );
    refused.extend(unsupported.iter().map(|(source, construct)| {
        (
            (*source).to_owned(),
            PatternFailure::Unsupported(*construct),
        )
    }));
    refused.push(("a{65536}".to_owned(), PatternFailure::Size));
    refused.push(("(?:a{256}){256}".to_owned(), PatternFailure::Size));
    refused.push(("(?:a|b){0,32768}".to_owned(), PatternFailure::Size));
    refused
}

/// Every string of up to [`LONGEST`] code points of [`ALPHABET`]: the shorter
/// first, and strings of one length in the alphabet's order, the first code
/// point varying slowest.
pub fn strings() -> Vec<String> {
    let mut every = vec![String::new()];
    let mut previous = vec![String::new()];
    for _ in 0..LONGEST {
        let next: Vec<String> = previous
            .iter()
            .flat_map(|prefix| {
                ALPHABET.iter().map(move |code_point| {
                    let mut string = prefix.clone();
                    string.push(*code_point);
                    string
                })
            })
            .collect();
        every.extend(next.iter().cloned());
        previous = next;
    }
    every
}

fn construct_name(construct: Construct) -> &'static str {
    match construct {
        Construct::Backreference => "backreference",
        Construct::Lookaround => "lookaround",
        Construct::NamedGroup => "namedGroup",
        Construct::Modifiers => "modifiers",
        Construct::PropertyEscape => "propertyEscape",
    }
}

fn failure_value(failure: PatternFailure) -> Value {
    match failure {
        PatternFailure::Nesting => json!({ "failure": "nesting" }),
        PatternFailure::Syntax => json!({ "failure": "syntax" }),
        PatternFailure::Unsupported(construct) => {
            json!({ "failure": "unsupported", "construct": construct_name(construct) })
        }
        PatternFailure::Size => json!({ "failure": "size" }),
    }
}

fn compiled(source: &str) -> Result<Pattern, String> {
    Pattern::compile(source).map_err(|failure| format!("{source:?} fails with {failure:?}"))
}

/// The vector of a pattern refused with `expected`.
fn refused_vector(source: &str, expected: PatternFailure) -> Result<Value, String> {
    match Pattern::compile(source) {
        Err(failure) if failure == expected => {
            let mut vector = failure_value(failure);
            vector["pattern"] = json!(source);
            Ok(vector)
        }
        outcome => Err(format!("{source:?} gives {outcome:?}, not {expected:?}")),
    }
}

/// The vectors document.
///
/// # Errors
///
/// A chosen or exhaustive pattern that does not compile, or a refused pattern
/// that compiles or fails otherwise than listed.
pub fn document() -> Result<Value, String> {
    let mut chosen = Vec::with_capacity(CHOSEN.len());
    for (id, source, haystacks) in CHOSEN {
        let pattern = compiled(source)?;
        let (matching, not_matching): (Vec<&str>, Vec<&str>) = haystacks
            .iter()
            .partition(|haystack| pattern.matches(haystack));
        chosen.push(json!({
            "id": id,
            "pattern": source,
            "matching": matching,
            "notMatching": not_matching,
        }));
    }
    let every = strings();
    let mut exhaustive = Vec::with_capacity(EXHAUSTIVE.len());
    for source in EXHAUSTIVE {
        let pattern = compiled(source)?;
        let decisions: String = every
            .iter()
            .map(|haystack| if pattern.matches(haystack) { '1' } else { '0' })
            .collect();
        exhaustive.push(json!({ "pattern": source, "decisions": decisions }));
    }
    let refused_vectors = refused()
        .iter()
        .map(|(source, expected)| refused_vector(source, *expected))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(json!({
        "alphabet": ALPHABET.iter().map(char::to_string).collect::<Vec<_>>(),
        "longest": LONGEST,
        "chosen": chosen,
        "exhaustive": exhaustive,
        "refused": refused_vectors,
    }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/pattern/vectors.json");

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        let committed: Value = serde_json::from_str(COMMITTED).expect("the vectors are JSON");
        assert_eq!(committed, document().expect("the vectors build"));
    }

    #[test]
    fn the_exhaustive_strings_are_every_string_up_to_the_longest() {
        let every = strings();
        let expected: usize = (0..=LONGEST)
            .map(|length| ALPHABET.len().pow(length as u32))
            .sum();
        assert_eq!(every.len(), expected);
        assert_eq!(every.iter().collect::<BTreeSet<_>>().len(), expected);
        assert_eq!(every[0], "");
        assert_eq!(every[1], "a");
        assert_eq!(every[ALPHABET.len() + 1], "aa");
        assert_eq!(
            every.last().map(String::as_str),
            Some("\u{1F600}\u{1F600}\u{1F600}")
        );
        assert!(every
            .windows(2)
            .all(|pair| pair[0].chars().count() <= pair[1].chars().count()));
    }

    #[test]
    fn every_chosen_pattern_has_strings_on_both_sides_or_says_why_not() {
        // A pattern that can match nothing a string holds has none to match.
        let never = ["empty-class", "lone-surrogate-escape"];
        let document = document().expect("the vectors build");
        for vector in document["chosen"].as_array().expect("chosen vectors") {
            let id = vector["id"].as_str().expect("an id");
            let matching = vector["matching"].as_array().expect("matching").len();
            let not_matching = vector["notMatching"]
                .as_array()
                .expect("not matching")
                .len();
            assert!(not_matching > 0, "{id}");
            assert_eq!(matching == 0, never.contains(&id), "{id}");
        }
    }

    #[test]
    fn every_failure_appears_among_the_refused_vectors() {
        let failures: BTreeSet<String> = refused()
            .iter()
            .map(|(_, failure)| format!("{failure:?}"))
            .collect();
        for failure in [
            PatternFailure::Nesting,
            PatternFailure::Syntax,
            PatternFailure::Unsupported(Construct::Backreference),
            PatternFailure::Unsupported(Construct::Lookaround),
            PatternFailure::Unsupported(Construct::NamedGroup),
            PatternFailure::Unsupported(Construct::Modifiers),
            PatternFailure::Unsupported(Construct::PropertyEscape),
            PatternFailure::Size,
        ] {
            assert!(failures.contains(&format!("{failure:?}")), "{failure:?}");
        }
    }

    #[test]
    fn a_refused_vector_fails_as_listed_or_not_at_all() {
        assert!(refused_vector("(", PatternFailure::Syntax).is_ok());
        assert!(refused_vector("(", PatternFailure::Size).is_err());
        assert!(refused_vector("a", PatternFailure::Syntax).is_err());
        assert_eq!(
            refused_vector("(?=a)", PatternFailure::Unsupported(Construct::Lookaround)),
            Ok(json!({ "failure": "unsupported", "construct": "lookaround", "pattern": "(?=a)" }))
        );
    }

    #[test]
    fn every_exhaustive_pattern_tells_some_strings_apart() {
        let every = strings();
        for source in EXHAUSTIVE.iter().filter(|source| !source.is_empty()) {
            let pattern = Pattern::compile(source).expect("the pattern compiles");
            let matched = every
                .iter()
                .filter(|haystack| pattern.matches(haystack))
                .count();
            assert!(matched > 0 && matched < every.len(), "{source:?}");
        }
    }
}
