// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Duplicate-aware JSON decoding extracted from the CLI compatibility path.
//!
//! This utility does not perform schema admission or the 015 Portable JSON
//! Number checks. In particular, it preserves the existing serde-based numeric
//! behavior; it is not a project-profile materializer. JSONC normalization,
//! filesystem access, and diagnostic envelopes belong to the caller.

use std::cell::Cell;
use std::fmt;

use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::Value;

use crate::location::{duplicate_member_position, SourcePosition};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JsonDecodeErrorKind {
    Syntax,
    DuplicateObjectMember,
}

/// A complete decoding failure; never contains a partially decoded value.
#[derive(Debug)]
pub struct JsonDecodeError {
    kind: JsonDecodeErrorKind,
    position: SourcePosition,
    cause: serde_json::Error,
}

impl JsonDecodeError {
    pub const fn kind(&self) -> JsonDecodeErrorKind {
        self.kind
    }

    /// For a duplicate, identifies the second key's opening quote.
    /// For syntax errors, preserves the CLI's serde error coordinates.
    pub const fn position(&self) -> SourcePosition {
        self.position
    }
}

impl fmt::Display for JsonDecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.cause.fmt(formatter)
    }
}

impl std::error::Error for JsonDecodeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.cause)
    }
}

/// Decode exactly one JSON value, rejecting repeated decoded keys per object.
///
/// Keys are case-sensitive and are not Unicode-normalized. A duplicate is
/// rejected before decoding its value, preserving the first encountered parse
/// failure. The result owns its data and borrows neither input nor scratch.
pub fn decode_unique_json(source: &str) -> Result<Value, JsonDecodeError> {
    let duplicate_found = Cell::new(false);
    let mut deserializer = serde_json::Deserializer::from_str(source);
    UniqueJsonValueSeed {
        duplicate_found: &duplicate_found,
    }
    .deserialize(&mut deserializer)
    .and_then(|value| {
        deserializer.end()?;
        Ok(value)
    })
    .map_err(|cause| {
        let (kind, position) = if duplicate_found.get() {
            (
                JsonDecodeErrorKind::DuplicateObjectMember,
                duplicate_member_position(source, cause.line(), cause.column()),
            )
        } else {
            (
                JsonDecodeErrorKind::Syntax,
                SourcePosition {
                    line: cause.line(),
                    column: cause.column().saturating_sub(1),
                },
            )
        };
        JsonDecodeError {
            kind,
            position,
            cause,
        }
    })
}

#[derive(Clone, Copy)]
struct UniqueJsonValueSeed<'a> {
    duplicate_found: &'a Cell<bool>,
}

struct UniqueJsonValueVisitor<'a> {
    duplicate_found: &'a Cell<bool>,
}

impl<'de> DeserializeSeed<'de> for UniqueJsonValueSeed<'_> {
    type Value = Value;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_any(UniqueJsonValueVisitor {
            duplicate_found: self.duplicate_found,
        })
    }
}

impl<'de> Visitor<'de> for UniqueJsonValueVisitor<'_> {
    type Value = Value;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("any JSON value without duplicate object members")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::Number(value.into()))
    }

    fn visit_f64<E>(self, value: f64) -> Result<Self::Value, E> {
        Ok(serde_json::Number::from_f64(value).map_or(Value::Null, Value::Number))
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(UniqueJsonValueSeed {
            duplicate_found: self.duplicate_found,
        })? {
            values.push(value);
        }
        Ok(Value::Array(values))
    }

    fn visit_map<A>(self, mut object: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut values = serde_json::Map::new();
        while let Some(key) = object.next_key::<String>()? {
            if values.contains_key(&key) {
                self.duplicate_found.set(true);
                return Err(de::Error::custom("duplicate object member"));
            }
            let value = object.next_value_seed(UniqueJsonValueSeed {
                duplicate_found: self.duplicate_found,
            })?;
            values.insert(key, value);
        }
        Ok(Value::Object(values))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decodes_owned_values_without_merging_object_scopes() {
        let source = String::from(r#"{"items":[{"x":1},{"x":2}],"X":true,"x":null}"#);
        let value = decode_unique_json(&source).unwrap();
        drop(source);
        assert_eq!(value, json!({"items":[{"x":1},{"x":2}],"X":true,"x":null}));
        assert_eq!(decode_unique_json("[]").unwrap(), json!([]));
    }

    #[test]
    fn rejects_decoded_duplicates_at_second_key() {
        let cases = [
            (r#"{"x":0,"\u0078":1}"#, 1, 7),
            (r#"{"a\"b":0,"a\u0022b":1}"#, 1, 10),
            (r#"{"x":"日本🙂","x":0}"#, 1, 18),
            ("{\r\n \"x\":0,\r\n \"x\":1\r\n}", 3, 1),
            (r#"{"x":0,"x" INVALID}"#, 1, 7),
        ];
        for (source, line, column) in cases {
            let error = decode_unique_json(source).unwrap_err();
            assert_eq!(
                error.kind(),
                JsonDecodeErrorKind::DuplicateObjectMember,
                "{source}"
            );
            assert_eq!(
                error.position(),
                SourcePosition { line, column },
                "{source}"
            );
        }
    }

    #[test]
    fn unicode_normalization_does_not_merge_keys() {
        let value = decode_unique_json(r#"{"é":1,"e\u0301":2}"#).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 2);
    }

    #[test]
    fn syntax_and_trailing_tokens_never_return_a_partial_value() {
        for source in [
            "",
            "{",
            "{} {}",
            "[] null",
            r#"{"x":[,"x":0}"#,
            "[1,]",
            "/*x*/{}",
        ] {
            let error = decode_unique_json(source).unwrap_err();
            assert_eq!(error.kind(), JsonDecodeErrorKind::Syntax, "{source}");
            assert!(std::error::Error::source(&error).is_some());
        }
    }

    #[test]
    fn failures_do_not_leak_state_into_later_calls() {
        for _ in 0..3 {
            assert_eq!(
                decode_unique_json(r#"{"x":0,"x":1}"#).unwrap_err().kind(),
                JsonDecodeErrorKind::DuplicateObjectMember
            );
            assert_eq!(decode_unique_json(r#"{"x":1}"#).unwrap(), json!({"x":1}));
            assert_eq!(
                decode_unique_json("{").unwrap_err().kind(),
                JsonDecodeErrorKind::Syntax
            );
        }
    }
}
