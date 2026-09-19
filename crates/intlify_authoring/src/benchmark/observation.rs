// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! This owner's observation codec, revision 0.
//!
//! These digests are not 017 artifact digests, Intent revisions, identity
//! values, or evidence-disclosure tokens. They exist so that two runs of the
//! same fixture can be compared for the same result, and the framing that
//! produced them travels beside every value.

use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// One complete 256-bit observation value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Digest([u8; 32]);

impl Digest {
    pub(super) const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

impl schemars::JsonSchema for Digest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "AuthoringObservationChecksum".into()
    }
    fn json_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({"type": "string", "pattern": "^[0-9a-f]{64}$"})
    }
}

impl Serialize for Digest {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&blake3::Hash::from_bytes(self.0).to_hex())
    }
}

impl<'de> Deserialize<'de> for Digest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor;
        impl de::Visitor<'_> for Visitor {
            type Value = Digest;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a lowercase 64-digit observation checksum")
            }
            fn visit_str<E: de::Error>(self, value: &str) -> Result<Digest, E> {
                fn digit(byte: u8) -> Option<u8> {
                    match byte {
                        b'0'..=b'9' => Some(byte - b'0'),
                        b'a'..=b'f' => Some(byte - b'a' + 10),
                        _ => None,
                    }
                }
                let bytes = value.as_bytes();
                if bytes.len() != 64 {
                    return Err(E::custom("invalid observation checksum"));
                }
                let mut result = [0; 32];
                for (index, [high, low]) in bytes.as_chunks::<2>().0.iter().enumerate() {
                    result[index] = digit(*high)
                        .zip(digit(*low))
                        .map(|(h, l)| h * 16 + l)
                        .ok_or_else(|| E::custom("invalid observation checksum"))?;
                }
                Ok(Digest(result))
            }
        }
        deserializer.deserialize_str(Visitor)
    }
}

/// One complete observation of a measured operation's result.
///
/// `entry` carries what the operation produced beyond its semantic value —
/// positions, mappings, and reported diagnostics — so that a run which agrees
/// semantically but disagrees about where things are is still a disagreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Observation {
    pub(super) semantic: Digest,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) positions: Option<Digest>,
}

impl Observation {
    /// Reduce the complete observation to one identity value.
    pub(super) fn identity(self) -> Digest {
        let mut frame = Frame::new("observation");
        frame.digest(self.semantic);
        frame.flag(self.positions.is_some());
        if let Some(positions) = self.positions {
            frame.digest(positions);
        }
        frame.finish()
    }
}

/// A length-prefixed framing, so that concatenation cannot be ambiguous.
pub(super) struct Frame(blake3::Hasher);

impl Frame {
    pub(super) fn new(domain: &str) -> Self {
        let mut frame = Self(blake3::Hasher::new());
        frame.bytes(b"intlify-authoring-minimum-observation/0");
        frame.text(domain);
        frame
    }
    pub(super) fn uint(&mut self, value: u64) {
        self.0.update(&value.to_le_bytes());
    }
    pub(super) fn flag(&mut self, value: bool) {
        self.0.update(&[u8::from(value)]);
    }
    pub(super) fn bytes(&mut self, bytes: &[u8]) {
        self.uint(u64::try_from(bytes.len()).expect("addressable fixture length"));
        self.0.update(bytes);
    }
    pub(super) fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }
    pub(super) fn digest(&mut self, value: Digest) {
        self.0.update(&value.0);
    }
    pub(super) fn finish(self) -> Digest {
        Digest(*self.0.finalize().as_bytes())
    }

    /// Frame one admitted JSON value produced by this crate's own models.
    pub(super) fn json(&mut self, value: &Value) {
        match value {
            Value::Null => self.uint(0),
            Value::Bool(value) => {
                self.uint(1);
                self.flag(*value);
            }
            Value::Number(value) => {
                self.uint(2);
                self.text(&value.to_string());
            }
            Value::String(value) => {
                self.uint(3);
                self.text(value);
            }
            Value::Array(values) => {
                self.uint(4);
                self.uint(u64::try_from(values.len()).expect("bounded fixture array"));
                for value in values {
                    self.json(value);
                }
            }
            Value::Object(values) => {
                self.uint(5);
                self.uint(u64::try_from(values.len()).expect("bounded fixture map"));
                // Member order is the model's own and is preserved, because a
                // projection's order is part of what it means.
                for (key, value) in values {
                    self.text(key);
                    self.json(value);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(domain: &str, value: &str) -> Digest {
        let mut frame = Frame::new(domain);
        frame.text(value);
        frame.finish()
    }

    #[test]
    fn framing_separates_domains_and_cannot_be_made_ambiguous_by_concatenation() {
        assert_ne!(digest("one", "ab"), digest("two", "ab"));
        // Length prefixes mean two adjacent runs never collide with one longer
        // run that happens to concatenate to the same bytes.
        let mut split = Frame::new("d");
        split.text("a");
        split.text("b");
        let mut joined = Frame::new("d");
        joined.text("ab");
        assert_ne!(split.finish(), joined.finish());
    }

    #[test]
    fn an_observation_identity_distinguishes_absent_positions_from_present_ones() {
        let semantic = digest("semantic", "value");
        let positions = digest("positions", "value");
        let without = Observation {
            semantic,
            positions: None,
        };
        let with = Observation {
            semantic,
            positions: Some(positions),
        };
        assert_ne!(without.identity(), with.identity());
        assert_eq!(without.identity(), without.identity());
    }

    #[test]
    fn the_wire_form_is_lowercase_hexadecimal_and_round_trips() {
        let observation = Observation {
            semantic: digest("semantic", "value"),
            positions: None,
        };
        let value = serde_json::to_value(observation).unwrap();
        assert!(value["semantic"]
            .as_str()
            .unwrap()
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        assert_eq!(value["positions"], serde_json::Value::Null);
        assert_eq!(
            serde_json::from_value::<Observation>(value).unwrap(),
            observation
        );
    }

    #[test]
    fn json_framing_keeps_the_models_own_member_order() {
        // A projection's member order is part of what it means, so two objects
        // that differ only in order are different observations.
        let mut first = Frame::new("d");
        first.json(&serde_json::json!({"a": "1", "b": "2"}));
        let mut second = Frame::new("d");
        second.json(&serde_json::json!({"b": "2", "a": "1"}));
        assert_ne!(first.finish(), second.finish());
    }
}
