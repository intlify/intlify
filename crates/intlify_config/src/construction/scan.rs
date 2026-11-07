// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Strict UTF-8 JSON, scanned without recursion and counted in the
//! construction-root bounds' units.
//!
//! Construction computes its bounds' totals over the complete input before
//! any other check, and an artifact contributes its JSON counts only when it
//! is strict UTF-8 JSON (D15:955), read as design 017 has every decoder read
//! it: malformed UTF-8, a byte-order mark, trailing data, a duplicate object
//! member, and an unpaired surrogate escape are refused, and a number is any
//! number JSON's grammar admits. The decoder construction reads values with
//! recurses, and stops at a recursion limit deeper than the depth bound. The
//! scan keeps its own stack instead, so it decides strictness at any depth:
//! a strict artifact too deep for the decoder still counts its depth, which
//! the depth bound refuses before any later row reads the value.

use std::collections::HashSet;

/// What a strict UTF-8 JSON artifact counts in the construction-root bounds'
/// units.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct JsonCounts {
    /// The deepest nesting level, with the root at one (`json-level`).
    pub(crate) depth: u64,
    /// The values, object keys excluded (`json-node`).
    pub(crate) nodes: u64,
    /// The object members and array elements (`json-collection-entry`).
    pub(crate) entries: u64,
    /// The UTF-8 bytes of every string and object key (`string-byte`).
    pub(crate) string_bytes: u64,
    /// The UTF-8 bytes of the longest string or object key.
    pub(crate) longest_string: u64,
}

/// Scan bytes as strict UTF-8 JSON and count them, or return `None` when they
/// are not strict UTF-8 JSON.
pub(crate) fn scan(bytes: &[u8]) -> Option<JsonCounts> {
    let text = std::str::from_utf8(bytes).ok()?;
    Scanner {
        text,
        at: 0,
        open: Vec::new(),
        objects: Vec::new(),
        opened: 0,
        keys: HashSet::new(),
        counts: JsonCounts::default(),
    }
    .document()
}

struct Scanner<'a> {
    text: &'a str,
    at: usize,
    /// The collections the scan is inside, innermost last: whether each is
    /// an object. One byte a level, so a deep input costs little.
    open: Vec<bool>,
    /// The ordinal of each object the scan is inside, innermost last.
    objects: Vec<usize>,
    /// How many objects the scan has entered, which gives the next ordinal.
    opened: usize,
    /// Every key read so far, decoded, with the ordinal of its object.
    keys: HashSet<(usize, String)>,
    counts: JsonCounts,
}

/// A count of a slice's length or a stack's size, which fits in 64 bits.
fn count(length: usize) -> u64 {
    u64::try_from(length).expect("an addressable length fits in u64")
}

impl Scanner<'_> {
    /// One value, and then nothing but whitespace.
    ///
    /// Each turn reads a value where one is expected, or, after a complete
    /// value, the delimiter or end of the collection it is in. Entering a
    /// collection pushes it, so no turn recurses.
    fn document(mut self) -> Option<JsonCounts> {
        let mut expecting = true;
        loop {
            self.whitespace();
            if expecting {
                expecting = self.value()?;
                continue;
            }
            match self.open.last().copied() {
                None => return (self.at == self.text.len()).then_some(self.counts),
                Some(false) => match self.next()? {
                    b',' => {
                        self.counts.entries += 1;
                        expecting = true;
                    }
                    b']' => {
                        self.open.pop();
                    }
                    _ => return None,
                },
                Some(true) => match self.next()? {
                    b',' => {
                        self.whitespace();
                        self.key()?;
                        expecting = true;
                    }
                    b'}' => {
                        self.open.pop();
                        self.objects.pop();
                    }
                    _ => return None,
                },
            }
        }
    }

    /// Read the start of a value, counting it. Returns whether a value is
    /// expected next, which is so only on entering a non-empty collection.
    fn value(&mut self) -> Option<bool> {
        self.counts.nodes += 1;
        self.counts.depth = self.counts.depth.max(count(self.open.len()) + 1);
        match self.next()? {
            b'[' => {
                self.whitespace();
                if self.peek() == Some(b']') {
                    self.at += 1;
                    return Some(false);
                }
                self.open.push(false);
                self.counts.entries += 1;
                Some(true)
            }
            b'{' => {
                self.whitespace();
                if self.peek() == Some(b'}') {
                    self.at += 1;
                    return Some(false);
                }
                self.open.push(true);
                self.objects.push(self.opened);
                self.opened += 1;
                self.key()?;
                Some(true)
            }
            b'"' => self.string(None).map(|()| false),
            b't' | b'f' | b'n' => {
                self.at -= 1;
                self.literal().map(|()| false)
            }
            b'-' | b'0'..=b'9' => {
                self.at -= 1;
                self.number().map(|()| false)
            }
            _ => None,
        }
    }

    /// Read a member's key and its colon, counting the member: its key must
    /// not repeat one of the same object's.
    fn key(&mut self) -> Option<()> {
        if self.next()? != b'"' {
            return None;
        }
        let mut key = String::new();
        self.string(Some(&mut key))?;
        let object = *self
            .objects
            .last()
            .expect("a key is read only inside an object");
        if !self.keys.insert((object, key)) {
            return None;
        }
        self.counts.entries += 1;
        self.whitespace();
        (self.next()? == b':').then_some(())
    }

    /// Read a string after its opening quote, counting its decoded UTF-8
    /// bytes, and keep them in `key` when it is a key.
    fn string(&mut self, mut key: Option<&mut String>) -> Option<()> {
        let mut length = 0;
        loop {
            // A run of characters that stand for themselves. It ends at an
            // ASCII byte, so it is whole UTF-8.
            let run = self.text[self.at..]
                .bytes()
                .take_while(|&byte| byte != b'"' && byte != b'\\' && byte >= 0x20)
                .count();
            if let Some(key) = key.as_deref_mut() {
                key.push_str(&self.text[self.at..self.at + run]);
            }
            length += count(run);
            self.at += run;
            match self.next()? {
                b'"' => break,
                b'\\' => {
                    let character = self.escape()?;
                    if let Some(key) = key.as_deref_mut() {
                        key.push(character);
                    }
                    length += count(character.len_utf8());
                }
                // A control character must be escaped.
                _ => return None,
            }
        }
        self.counts.string_bytes += length;
        self.counts.longest_string = self.counts.longest_string.max(length);
        Some(())
    }

    /// Read an escape after its backslash.
    fn escape(&mut self) -> Option<char> {
        Some(match self.next()? {
            b'"' => '"',
            b'\\' => '\\',
            b'/' => '/',
            b'b' => '\u{8}',
            b'f' => '\u{c}',
            b'n' => '\n',
            b'r' => '\r',
            b't' => '\t',
            b'u' => return self.unicode(),
            _ => return None,
        })
    }

    /// Read a `\u` escape after its `u`: a scalar value, or a leading
    /// surrogate escaped together with its trailing one.
    fn unicode(&mut self) -> Option<char> {
        let unit = self.hex()?;
        let scalar = match unit {
            0xD800..=0xDBFF => {
                if !self.text.as_bytes()[self.at..].starts_with(b"\\u") {
                    return None;
                }
                self.at += 2;
                let trailing = self.hex()?;
                if !(0xDC00..=0xDFFF).contains(&trailing) {
                    return None;
                }
                0x1_0000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(trailing) - 0xDC00)
            }
            0xDC00..=0xDFFF => return None,
            unit => u32::from(unit),
        };
        char::from_u32(scalar)
    }

    /// Read four hexadecimal digits.
    fn hex(&mut self) -> Option<u16> {
        let digits = self.text.get(self.at..self.at + 4)?;
        if !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        self.at += 4;
        u16::from_str_radix(digits, 16).ok()
    }

    /// Read a number: a sign, an integer part without a leading zero, and an
    /// optional fraction and exponent, each with at least one digit.
    fn number(&mut self) -> Option<()> {
        if self.peek() == Some(b'-') {
            self.at += 1;
        }
        match self.next()? {
            b'0' => {}
            b'1'..=b'9' => self.digits(),
            _ => return None,
        }
        if self.peek() == Some(b'.') {
            self.at += 1;
            self.required_digits()?;
        }
        if matches!(self.peek(), Some(b'e' | b'E')) {
            self.at += 1;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.at += 1;
            }
            self.required_digits()?;
        }
        Some(())
    }

    fn digits(&mut self) {
        while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
            self.at += 1;
        }
    }

    fn required_digits(&mut self) -> Option<()> {
        let start = self.at;
        self.digits();
        (self.at > start).then_some(())
    }

    /// Read `true`, `false`, or `null`.
    fn literal(&mut self) -> Option<()> {
        let rest = &self.text.as_bytes()[self.at..];
        let literal = [&b"true"[..], b"false", b"null"]
            .into_iter()
            .find(|literal| rest.starts_with(literal))?;
        self.at += literal.len();
        Some(())
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.text.as_bytes().get(self.at).copied()
    }

    fn next(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.at += 1;
        Some(byte)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use intlify_shared_json::json::decode_unique_json;

    use super::*;
    use crate::construction::flow::testing::{
        AUTHORITY_SET, CAPABILITY, CONFIGURATION_SCHEMA, PACKAGE, PROFILE_SPECIFICATION,
        PROFILE_SPECIFICATION_SCHEMA, REGISTRY_MEMBERS,
    };

    /// Whether the shared decoder reads the bytes as strict UTF-8 JSON.
    fn decodes(bytes: &[u8]) -> bool {
        std::str::from_utf8(bytes).is_ok_and(|text| decode_unique_json(text).is_ok())
    }

    /// The counts of a decoded value, walked as the units define them.
    fn counted(value: &Value) -> JsonCounts {
        let mut counts = JsonCounts::default();
        let mut stack = vec![(value, 1)];
        let string = |counts: &mut JsonCounts, text: &str| {
            let length = count(text.len());
            counts.string_bytes += length;
            counts.longest_string = counts.longest_string.max(length);
        };
        while let Some((value, depth)) = stack.pop() {
            counts.nodes += 1;
            counts.depth = counts.depth.max(depth);
            match value {
                Value::Array(items) => {
                    counts.entries += count(items.len());
                    stack.extend(items.iter().map(|item| (item, depth + 1)));
                }
                Value::Object(members) => {
                    counts.entries += count(members.len());
                    for (key, member) in members {
                        string(&mut counts, key);
                        stack.push((member, depth + 1));
                    }
                }
                Value::String(text) => string(&mut counts, text),
                Value::Null | Value::Bool(_) | Value::Number(_) => {}
            }
        }
        counts
    }

    fn nested(levels: usize) -> String {
        format!("{}{}", "[".repeat(levels), "]".repeat(levels))
    }

    #[test]
    fn the_counts_are_the_units_of_the_construction_bounds() {
        assert_eq!(
            scan(b" 0 "),
            Some(JsonCounts {
                depth: 1,
                nodes: 1,
                ..JsonCounts::default()
            })
        );
        assert_eq!(
            scan(br#"[[], {}, "", true]"#),
            Some(JsonCounts {
                depth: 2,
                nodes: 5,
                entries: 4,
                ..JsonCounts::default()
            })
        );
        // Keys are not nodes, but their bytes count; an escape counts the
        // UTF-8 bytes of the character it stands for.
        assert_eq!(
            scan("{\"é\": [\"a\\u00e9\", {\"\\ud83d\\ude00\": null}]}".as_bytes()),
            Some(JsonCounts {
                depth: 4,
                nodes: 5,
                entries: 4,
                string_bytes: 2 + 3 + 4,
                longest_string: 4,
            })
        );
        // As deep as the depth bound allows: 64 levels, the root at one.
        let deep = format!("{}0{}", "[".repeat(63), "]".repeat(63));
        assert_eq!(scan(deep.as_bytes()).map(|counts| counts.depth), Some(64));
    }

    /// Every committed authority and member counts as a walk of its decoded
    /// value does.
    #[test]
    fn the_committed_artifacts_count_as_their_values_do() {
        let artifacts = [
            PROFILE_SPECIFICATION_SCHEMA,
            PROFILE_SPECIFICATION,
            AUTHORITY_SET,
            CONFIGURATION_SCHEMA,
            CAPABILITY,
            PACKAGE,
        ]
        .into_iter()
        .chain(REGISTRY_MEMBERS);
        for bytes in artifacts {
            let value = decode_unique_json(std::str::from_utf8(bytes).unwrap()).unwrap();
            assert_eq!(scan(bytes), Some(counted(&value)));
        }
    }

    /// The scan refuses what the shared decoder refuses, and reads what it
    /// reads, at every depth the decoder reads.
    #[test]
    fn the_scan_is_as_strict_as_the_shared_decoder() {
        let cases: &[&[u8]] = &[
            b"",
            b" \t\r\n",
            b"{}",
            b"[]",
            b"{",
            b"[1,]",
            b"[1 2]",
            b"[,1]",
            br#"{"a":1,}"#,
            br#"{"a" 1}"#,
            br#"{"a":1 "b":2}"#,
            br#"{,"a":1}"#,
            br#"{"a"}"#,
            br"{1:2}",
            b"0",
            b"-0",
            b"01",
            b"-",
            b"+1",
            b"1.",
            b".5",
            b"1.5.3",
            b"1e",
            b"1e+",
            b"1E-9",
            b"0.0e0",
            b"1e400",
            b"-1.5e-400",
            b"123456789012345678901234567890",
            b"0x10",
            b"NaN",
            b"Infinity",
            b"true",
            b"truth",
            b"nul",
            b"null x",
            b"nulls",
            b"\xEF\xBB\xBF{}",
            b"\x0c{}",
            b"{} \t\r\n",
            b"{}{}",
            b"// x\n{}",
            b"'a'",
            b"\"\x01\"",
            b"\"\x7f\"",
            b"\"\\u0000\"",
            b"\"\\x\"",
            b"\"\\u12\"",
            b"\"\\u12G4\"",
            b"\"\\u+041\"",
            b"\"\\ud800\"",
            b"\"\\udc00\"",
            b"\"\\ud800\\udc00\"",
            b"\"\\ud800\\u0041\"",
            b"\"\\ud800\\n\"",
            b"\"\\uD83D\\uDE00\"",
            b"\"\\/\"",
            b"\"a",
            b"\"\xC3\"",
            b"\"\xE2\x80\xA8\"",
            br#"{"a":1,"a":2}"#,
            br#"{"a":1,"\u0061":2}"#,
            br#"{"a":{"a":1}}"#,
            br#"[{"a":1},{"a":2}]"#,
            br#"{"A":1,"a":2}"#,
            br#"{"$serde_json::private::Number":"1"}"#,
        ];
        for case in cases {
            assert_eq!(
                scan(case).is_some(),
                decodes(case),
                "{}",
                String::from_utf8_lossy(case)
            );
        }
        for levels in [1, 64, 65, 126, 127] {
            let case = nested(levels);
            assert!(decodes(case.as_bytes()), "{levels}");
            assert_eq!(
                scan(case.as_bytes()).map(|counts| counts.depth),
                Some(count(levels))
            );
        }
    }

    /// Replacing, removing, or inserting one byte anywhere leaves the scan and
    /// the shared decoder agreeing on whether the bytes are strict: in a
    /// committed artifact, and in a document with every kind of token.
    #[test]
    fn the_scan_agrees_with_the_shared_decoder_on_changed_bytes() {
        let tokens: &[u8] =
            br#"{"a": [0, -1.5e+10, 2E-3, true, false, null, "x\n\u00e9\ud83d\ude00\/"], "b": {"c": {}}, "d": []}"#;
        assert!(decodes(tokens));
        let bytes = [
            b'"', b'\\', b'{', b'}', b'[', b']', b',', b':', b' ', b'0', b'1', b'-', b'+', b'.',
            b'e', b'u', b'd', b'a', 0x01, 0x7F, 0xC3,
        ];
        let agree = |changed: &[u8], what: &str| {
            assert_eq!(scan(changed).is_some(), decodes(changed), "{what}");
        };
        for source in [AUTHORITY_SET, tokens] {
            for at in 0..=source.len() {
                for &byte in &bytes {
                    let mut inserted = source.to_vec();
                    inserted.insert(at, byte);
                    agree(&inserted, &format!("{byte} inserted at {at}"));
                    if at < source.len() {
                        let mut replaced = source.to_vec();
                        replaced[at] = byte;
                        agree(&replaced, &format!("{byte} at {at}"));
                    }
                }
                if at < source.len() {
                    let mut removed = source.to_vec();
                    removed.remove(at);
                    agree(&removed, &format!("removed at {at}"));
                }
            }
        }
    }

    /// The decoder stops at its recursion limit; the scan reads any depth
    /// with its own stack, and still refuses what is not strict.
    #[test]
    fn the_scan_decides_depths_the_shared_decoder_cannot_read() {
        let limit = nested(128);
        assert!(!decodes(limit.as_bytes()));
        assert_eq!(scan(limit.as_bytes()).map(|counts| counts.depth), Some(128));
        let deep = format!("{{\"a\":{}}}", nested(1_000_000));
        assert_eq!(
            scan(deep.as_bytes()).map(|counts| counts.depth),
            Some(1_000_001)
        );
        let unclosed = "[".repeat(1_000_000);
        assert_eq!(scan(unclosed.as_bytes()), None);
        let mismatched = format!("{}}}", "[".repeat(200));
        assert_eq!(scan(mismatched.as_bytes()), None);
    }
}
