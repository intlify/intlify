// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Canonical JSON text revision `"0"` from design 017: the file codec for JSON
//! artifacts written into output sets and destinations.
//!
//! The text is UTF-8 without a byte-order mark, with object members in
//! ascending unsigned UTF-8 key order, no whitespace outside strings, and no
//! line terminator at the end. A string escapes only `"`, `\`, and the scalars
//! U+0000 to U+001F, the last as lowercase `\uXXXX`; every other scalar is
//! written as itself. There are no JSON numbers, because every quantity is
//! already its `UInt64` string. Arrays keep the order given: an owner puts a
//! set in its canonical sequence before writing it, as for value framing.
//!
//! This is a file codec, not a digest preimage. A record's digest still frames
//! its value, and a file's own identity is the SHA-256 of these bytes.

use std::fmt::Write as _;

use serde_json::Value;

use crate::decode::{self, DecodeFailure};

// Private writer capacity, matching the value framing's.
const MAX_DEPTH: usize = 128;

/// Complete failure to write or read canonical JSON text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextFailure {
    /// A JSON number was present; quantities are decimal strings.
    Number,
    /// The value nests deeper than the writer's capacity.
    Depth,
    /// The bytes are not one bounded JSON value in the shared encoding.
    Unreadable(DecodeFailure),
    /// The bytes are a JSON value but not its canonical text.
    NotCanonical,
}

/// Write one value as canonical JSON text.
pub fn write(value: &Value) -> Result<String, TextFailure> {
    let mut output = String::new();
    write_value(value, 1, &mut output)?;
    Ok(output)
}

/// Read bytes that must be exactly the canonical JSON text of their value.
///
/// Whitespace, another member order, another escape spelling, or a trailing
/// line terminator is a different file with the same value, so it is refused
/// rather than accepted as an equivalent spelling.
pub fn read(bytes: &[u8]) -> Result<Value, TextFailure> {
    let value = decode::value(bytes, true).map_err(TextFailure::Unreadable)?;
    if write(&value)?.as_bytes() != bytes {
        return Err(TextFailure::NotCanonical);
    }
    Ok(value)
}

fn write_value(value: &Value, depth: usize, output: &mut String) -> Result<(), TextFailure> {
    if depth > MAX_DEPTH {
        return Err(TextFailure::Depth);
    }
    match value {
        Value::Null => output.push_str("null"),
        Value::Bool(true) => output.push_str("true"),
        Value::Bool(false) => output.push_str("false"),
        Value::Number(_) => return Err(TextFailure::Number),
        Value::String(text) => string(text, output),
        Value::Array(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                write_value(item, depth + 1, output)?;
            }
            output.push(']');
        }
        Value::Object(members) => {
            let mut ordered: Vec<_> = members.iter().collect();
            ordered.sort_unstable_by(|(a, _), (b, _)| a.as_bytes().cmp(b.as_bytes()));
            output.push('{');
            for (index, (key, item)) in ordered.into_iter().enumerate() {
                if index > 0 {
                    output.push(',');
                }
                string(key, output);
                output.push(':');
                write_value(item, depth + 1, output)?;
            }
            output.push('}');
        }
    }
    Ok(())
}

fn string(text: &str, output: &mut String) {
    output.push('"');
    for scalar in text.chars() {
        match scalar {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0}'..='\u{1f}' => {
                // Writing to a String cannot fail.
                let _ = write!(output, "\\u{:04x}", u32::from(scalar));
            }
            other => output.push(other),
        }
    }
    output.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scalars_and_containers_are_written_without_whitespace() {
        for (value, expected) in [
            (Value::Null, "null"),
            (json!(false), "false"),
            (json!(true), "true"),
            (json!(""), "\"\""),
            (json!("a"), "\"a\""),
            (json!([]), "[]"),
            (json!({}), "{}"),
            (json!(["a", null, [true]]), "[\"a\",null,[true]]"),
            (
                json!({"b": {"d": [], "c": "x"}, "a": false}),
                "{\"a\":false,\"b\":{\"c\":\"x\",\"d\":[]}}",
            ),
        ] {
            assert_eq!(write(&value).unwrap(), expected, "{value}");
        }
    }

    #[test]
    fn members_are_ordered_by_unsigned_utf8_bytes_and_arrays_keep_their_order() {
        // U+E000 is EE 80 80 and U+10000 is F0 90 80 80, so the private-use
        // character sorts first by bytes, though not by UTF-16 code units.
        let value = json!({"𐀀": "1", "\u{e000}": "2", "é": "3", "z": "4", "Z": "5", "": "6"});
        assert_eq!(
            write(&value).unwrap(),
            "{\"\":\"6\",\"Z\":\"5\",\"z\":\"4\",\"é\":\"3\",\"\u{e000}\":\"2\",\"𐀀\":\"1\"}"
        );
        assert_eq!(write(&json!(["b", "a"])).unwrap(), "[\"b\",\"a\"]");
    }

    #[test]
    fn only_quote_backslash_and_c0_controls_are_escaped() {
        for (text, expected) in [
            ("\"", "\"\\\"\""),
            ("\\", "\"\\\\\""),
            ("\u{0}", "\"\\u0000\""),
            ("\n", "\"\\u000a\""),
            ("\u{1f}", "\"\\u001f\""),
            ("\u{7f}", "\"\u{7f}\""),
            ("/", "\"/\""),
            ("\u{2028}", "\"\u{2028}\""),
            ("é日本𐀀", "\"é日本𐀀\""),
        ] {
            assert_eq!(write(&json!(text)).unwrap(), expected, "{text:?}");
        }
        assert_eq!(
            write(&json!({"\n": "a"})).unwrap(),
            "{\"\\u000a\":\"a\"}",
            "member names are strings too"
        );
    }

    #[test]
    fn numbers_and_excess_depth_are_not_written() {
        for value in [json!(0), json!(-1), json!(1.5), json!({"count": [1]})] {
            assert_eq!(write(&value), Err(TextFailure::Number), "{value}");
        }
        let mut value = Value::Null;
        for _ in 1..MAX_DEPTH {
            value = Value::Array(vec![value]);
        }
        assert!(write(&value).is_ok());
        assert_eq!(
            write(&Value::Array(vec![value.clone()])),
            Err(TextFailure::Depth)
        );
        assert_eq!(
            write(&json!({"nested": value})),
            Err(TextFailure::Depth),
            "an object adds a level as an array does"
        );
    }

    #[test]
    fn reading_returns_the_value_of_exactly_canonical_text() {
        let text = "{\"a\":[\"x\",null,true],\"b\":{\"\\u000a\":\"é\"}}";
        assert_eq!(
            read(text.as_bytes()),
            Ok(json!({"a": ["x", null, true], "b": {"\n": "é"}}))
        );
    }

    #[test]
    fn another_spelling_of_the_same_value_is_not_canonical() {
        for text in [
            "{\"a\":\"x\"}\n",
            " {\"a\":\"x\"}",
            "{\"a\": \"x\"}",
            "{\"b\":\"y\",\"a\":\"x\"}",
            "[\"\\u0041\"]",
            "[\"\\/\"]",
            "[\"\\n\"]",
            "[\"\\u001F\"]",
            "[\"\\u00e9\"]",
        ] {
            assert_eq!(
                read(text.as_bytes()),
                Err(TextFailure::NotCanonical),
                "{text:?}"
            );
        }
    }

    #[test]
    fn bytes_that_are_not_one_bounded_value_are_unreadable() {
        for (bytes, failure) in [
            (b"\xef\xbb\xbf{}".as_slice(), DecodeFailure::Syntax),
            (b"{\"a\":\"x\",\"a\":\"y\"}", DecodeFailure::Syntax),
            (b"\xff", DecodeFailure::Syntax),
            (b"{}{}", DecodeFailure::Syntax),
            (b"[1]", DecodeFailure::Number),
        ] {
            assert_eq!(
                read(bytes),
                Err(TextFailure::Unreadable(failure)),
                "{bytes:?}"
            );
        }
    }
}
