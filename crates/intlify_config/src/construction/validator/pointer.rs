// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! JSON Pointers, as the fragments of Draft 7's `$ref` name subschemas.
//!
//! A fragment that is empty or starts with `/` is a JSON Pointer (RFC 6901
//! section 6); any other fragment is a plain name. A pointer's escapes are
//! decoded first, and must spell UTF-8. The decoded text is then split at
//! each `/` into reference tokens, in which `~1` stands for `/` and `~0` for
//! `~`; a `~` followed by anything else is malformed. A token names an
//! object's member by its name, or an array's element by an index written in
//! decimal without a leading zero.

use serde_json::Value;

/// A JSON Pointer's reference tokens, decoded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pointer {
    tokens: Vec<String>,
}

impl Pointer {
    /// Read the pointer a URI fragment spells, as written in the reference.
    ///
    /// Returns `None` when the fragment is a plain name, has an escape that
    /// does not decode to UTF-8, or decodes to a malformed pointer.
    pub(crate) fn from_fragment(fragment: &str) -> Option<Self> {
        if !fragment.is_empty() && !fragment.starts_with('/') {
            return None;
        }
        Self::parse(&decode(fragment)?)
    }

    /// Read a pointer's text: empty for the whole value, or a `/` before each
    /// reference token.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        if text.is_empty() {
            return Some(Self { tokens: Vec::new() });
        }
        let tokens = text
            .strip_prefix('/')?
            .split('/')
            .map(unescape)
            .collect::<Option<Vec<_>>>()?;
        Some(Self { tokens })
    }

    pub(crate) fn tokens(&self) -> &[String] {
        &self.tokens
    }

    /// The value the pointer names within `root`, or `None` when no value is
    /// there.
    pub(crate) fn evaluate<'a>(&self, root: &'a Value) -> Option<&'a Value> {
        self.tokens
            .iter()
            .try_fold(root, |value, token| match value {
                Value::Object(members) => members.get(token),
                Value::Array(items) => index(token).and_then(|index| items.get(index)),
                _ => None,
            })
    }
}

/// Decode every escape of a fragment, which the URI grammar has already
/// checked, and read the bytes as UTF-8.
fn decode(fragment: &str) -> Option<String> {
    let bytes = fragment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits = std::str::from_utf8(bytes.get(index + 1..index + 3)?).ok()?;
            if !digits.bytes().all(|digit| digit.is_ascii_hexdigit()) {
                return None;
            }
            decoded.push(u8::from_str_radix(digits, 16).ok()?);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// A reference token with `~1` read as `/` and `~0` as `~`.
fn unescape(token: &str) -> Option<String> {
    let mut unescaped = String::with_capacity(token.len());
    let mut characters = token.chars();
    while let Some(character) = characters.next() {
        if character == '~' {
            match characters.next() {
                Some('0') => unescaped.push('~'),
                Some('1') => unescaped.push('/'),
                _ => return None,
            }
        } else {
            unescaped.push(character);
        }
    }
    Some(unescaped)
}

/// An array index: `0`, or digits that do not start with `0`. Parsing
/// refuses every byte but a digit after the first, which is a digit too, so
/// no sign gets through.
fn index(token: &str) -> Option<usize> {
    let first = *token.as_bytes().first()?;
    if !first.is_ascii_digit() || (first == b'0' && token.len() > 1) {
        return None;
    }
    token.parse().ok()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    /// RFC 6901 section 5's document.
    fn document() -> Value {
        json!({
            "foo": ["bar", "baz"],
            "": 0,
            "a/b": 1,
            "c%d": 2,
            "e^f": 3,
            "g|h": 4,
            "i\\j": 5,
            "k\"l": 6,
            " ": 7,
            "m~n": 8
        })
    }

    #[test]
    fn rfc_6901s_pointers_name_its_values() {
        let document = document();
        let examples = [
            ("", document.clone()),
            ("/foo", json!(["bar", "baz"])),
            ("/foo/0", json!("bar")),
            ("/", json!(0)),
            ("/a~1b", json!(1)),
            ("/c%d", json!(2)),
            ("/e^f", json!(3)),
            ("/g|h", json!(4)),
            ("/i\\j", json!(5)),
            ("/k\"l", json!(6)),
            ("/ ", json!(7)),
            ("/m~0n", json!(8)),
        ];
        for (text, expected) in &examples {
            let pointer = Pointer::parse(text).unwrap_or_else(|| panic!("{text:?}"));
            assert_eq!(pointer.evaluate(&document), Some(expected), "{text:?}");
        }
        // Section 6 writes the same pointers as fragments.
        let fragments = [
            "", "/foo", "/foo/0", "/", "/a~1b", "/c%25d", "/e%5Ef", "/g%7Ch", "/i%5Cj", "/k%22l",
            "/%20", "/m~0n",
        ];
        for (fragment, (_, expected)) in fragments.iter().zip(&examples) {
            let pointer =
                Pointer::from_fragment(fragment).unwrap_or_else(|| panic!("{fragment:?}"));
            assert_eq!(pointer.evaluate(&document), Some(expected), "{fragment:?}");
        }
    }

    #[test]
    fn a_token_reads_tilde_one_as_a_slash_and_tilde_zero_as_a_tilde() {
        for (text, tokens) in [
            ("", &[][..]),
            ("/", &[""][..]),
            ("//", &["", ""][..]),
            ("/a~1b/~0/", &["a/b", "~", ""][..]),
            ("/~01", &["~1"][..]),
            ("/~10", &["/0"][..]),
            ("/~0~1~1~0", &["~//~"][..]),
            ("/a/b/c", &["a", "b", "c"][..]),
        ] {
            let pointer = Pointer::parse(text).unwrap_or_else(|| panic!("{text:?}"));
            assert_eq!(pointer.tokens(), tokens, "{text:?}");
        }
        for text in [
            "a", "#/a", " /a", "/~", "/~2", "/a~", "/~a", "/a/~/b", "/~-1",
        ] {
            assert_eq!(Pointer::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn a_fragment_decodes_its_escapes_before_it_is_read() {
        for (fragment, tokens) in [
            ("/%7E0", &["~"][..]),
            ("/a%2Fb", &["a", "b"][..]),
            ("/%C3%A9", &["\u{e9}"][..]),
            ("/%c3%a9", &["\u{e9}"][..]),
            ("/%25", &["%"][..]),
            ("/%2525", &["%25"][..]),
        ] {
            let pointer =
                Pointer::from_fragment(fragment).unwrap_or_else(|| panic!("{fragment:?}"));
            assert_eq!(pointer.tokens(), tokens, "{fragment:?}");
        }
        for fragment in [
            // Plain names.
            "a", "%2Fa", "foo/bar", // Not UTF-8.
            "/%FF", "/%C3", "/%C3%28", // Escapes the URI grammar would refuse.
            "/%", "/%4", "/%4g", "/%+1", // A malformed token once decoded.
            "/%7E2",
        ] {
            assert_eq!(Pointer::from_fragment(fragment), None, "{fragment:?}");
        }
    }

    #[test]
    fn an_array_index_is_decimal_without_a_leading_zero() {
        let document = document();
        for (text, expected) in [
            ("/foo/0", Some(json!("bar"))),
            ("/foo/1", Some(json!("baz"))),
            ("/foo/2", None),
            ("/foo/01", None),
            ("/foo/00", None),
            ("/foo/-", None),
            ("/foo/+1", None),
            ("/foo/-1", None),
            ("/foo/ 1", None),
            ("/foo/1 ", None),
            ("/foo/1e0", None),
            ("/foo/", None),
            ("/foo/18446744073709551616", None),
            ("/foo/0/x", None),
            ("/foo/0/0", None),
            ("/x", None),
            ("/a~1b/c", None),
        ] {
            let pointer = Pointer::parse(text).unwrap_or_else(|| panic!("{text:?}"));
            assert_eq!(pointer.evaluate(&document).cloned(), expected, "{text:?}");
        }
        // An object's member is named by its name, even when it looks like an
        // index.
        let numbered = json!({ "0": "zero", "01": "one", "-": "dash", "": { "": "empty" } });
        for (text, expected) in [
            ("/0", json!("zero")),
            ("/01", json!("one")),
            ("/-", json!("dash")),
            ("//", json!("empty")),
        ] {
            let pointer = Pointer::parse(text).unwrap_or_else(|| panic!("{text:?}"));
            assert_eq!(pointer.evaluate(&numbered), Some(&expected), "{text:?}");
        }
        let long = Value::Array(vec![Value::Null; 11]);
        assert_eq!(
            Pointer::parse("/10").unwrap().evaluate(&long),
            Some(&Value::Null)
        );
    }
}
