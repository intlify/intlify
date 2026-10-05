// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The native observation codec an owner run computes its checksums with.
//!
//! These digests are not 017 artifact digests, Intent revisions, identity
//! values, or evidence-disclosure tokens. They exist so that two runs of the
//! same fixture can be compared for the same result. The algorithm is fixed
//! here; the framing is the owner's, and every value is recorded beside it.

use std::fmt;

use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;

/// The framing label an owner registers for every checksum it computes.
///
/// It is written into the hash ahead of each domain, so two owners that frame
/// the same values never produce the same checksum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Framing(&'static str);

impl Framing {
    /// Register one framing label.
    #[must_use]
    pub const fn new(label: &'static str) -> Self {
        Self(label)
    }

    /// Return the label exactly as registered.
    #[must_use]
    pub const fn label(self) -> &'static str {
        self.0
    }

    /// Open one frame under this framing and the given domain.
    #[must_use]
    pub fn frame(self, domain: &str) -> Frame {
        let mut frame = Frame(blake3::Hasher::new());
        frame.bytes(self.0.as_bytes());
        frame.text(domain);
        frame
    }
}

/// One complete 256-bit observation value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Digest([u8; 32]);

impl Digest {
    /// Return the complete value.
    #[must_use]
    pub const fn bytes(self) -> [u8; 32] {
        self.0
    }
}

impl schemars::JsonSchema for Digest {
    fn schema_name() -> std::borrow::Cow<'static, str> {
        "OwnerObservationChecksum".into()
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
/// `positions` carries what the operation produced beyond its semantic value —
/// positions, mappings, and reported diagnostics — so that a run which agrees
/// semantically but disagrees about where things are is still a disagreement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Observation {
    pub semantic: Digest,
    #[serde(deserialize_with = "Option::deserialize")]
    pub positions: Option<Digest>,
}

impl Observation {
    /// Reduce the complete observation to one identity value.
    #[must_use]
    pub fn identity(self, framing: Framing) -> Digest {
        let mut frame = framing.frame("observation");
        frame.digest(self.semantic);
        frame.flag(self.positions.is_some());
        if let Some(positions) = self.positions {
            frame.digest(positions);
        }
        frame.finish()
    }
}

/// A length-prefixed framing, so that concatenation cannot be ambiguous.
pub struct Frame(blake3::Hasher);

impl Frame {
    /// Frame one unsigned integer.
    pub fn uint(&mut self, value: u64) {
        self.0.update(&value.to_le_bytes());
    }

    /// Frame one flag.
    pub fn flag(&mut self, value: bool) {
        self.0.update(&[u8::from(value)]);
    }

    /// Frame one run of bytes, prefixed with its length.
    pub fn bytes(&mut self, bytes: &[u8]) {
        self.uint(u64::try_from(bytes.len()).expect("addressable fixture length"));
        self.0.update(bytes);
    }

    /// Frame one string, as its bytes.
    pub fn text(&mut self, value: &str) {
        self.bytes(value.as_bytes());
    }

    /// Frame one digest, which is always exactly 32 bytes.
    pub fn digest(&mut self, value: Digest) {
        self.0.update(&value.0);
    }

    /// Close the frame.
    #[must_use]
    pub fn finish(self) -> Digest {
        Digest(*self.0.finalize().as_bytes())
    }

    /// Frame one admitted JSON value produced by an owner's own models.
    pub fn json(&mut self, value: &Value) {
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

    const FRAMING: Framing = Framing::new("intlify-measurement-test-observation/0");

    fn digest(domain: &str, value: &str) -> Digest {
        let mut frame = FRAMING.frame(domain);
        frame.text(value);
        frame.finish()
    }

    #[test]
    fn framing_separates_domains_and_cannot_be_made_ambiguous_by_concatenation() {
        assert_ne!(digest("one", "ab"), digest("two", "ab"));
        // Length prefixes mean two adjacent runs never collide with one longer
        // run that happens to concatenate to the same bytes.
        let mut split = FRAMING.frame("d");
        split.text("a");
        split.text("b");
        let mut joined = FRAMING.frame("d");
        joined.text("ab");
        assert_ne!(split.finish(), joined.finish());
    }

    #[test]
    fn two_owners_framing_the_same_values_do_not_agree() {
        // The framing label is hashed ahead of the domain, so an owner cannot
        // produce another owner's checksum by framing the same content.
        let other = Framing::new("intlify-measurement-other-observation/0");
        let mut mine = FRAMING.frame("d");
        mine.text("value");
        let mut theirs = other.frame("d");
        theirs.text("value");
        assert_ne!(mine.finish(), theirs.finish());
        assert_eq!(other.label(), "intlify-measurement-other-observation/0");
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
        assert_ne!(without.identity(FRAMING), with.identity(FRAMING));
        assert_eq!(without.identity(FRAMING), without.identity(FRAMING));
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
        // A missing member is not an absent one: `positions` must be present.
        assert!(serde_json::from_value::<Observation>(serde_json::json!({
            "semantic": "0".repeat(64)
        }))
        .is_err());
        // Uppercase or short values are not this codec's spelling.
        for invalid in ["A".repeat(64), "0".repeat(63)] {
            assert!(serde_json::from_value::<Digest>(serde_json::json!(invalid)).is_err());
        }
    }

    #[test]
    fn the_framing_is_exactly_its_registered_byte_layout() {
        // Records already written depend on these exact bytes, so the layout
        // is pinned against a separate computation rather than against itself.
        fn prefixed(hasher: &mut blake3::Hasher, bytes: &[u8]) {
            hasher.update(&(bytes.len() as u64).to_le_bytes());
            hasher.update(bytes);
        }
        let opened = |domain: &[u8]| {
            let mut hasher = blake3::Hasher::new();
            prefixed(&mut hasher, FRAMING.label().as_bytes());
            prefixed(&mut hasher, domain);
            hasher
        };
        let semantic = digest("semantic", "value");
        let positions = digest("positions", "value");
        for recorded in [None, Some(positions)] {
            let mut expected = opened(b"observation");
            expected.update(&semantic.bytes());
            expected.update(&[u8::from(recorded.is_some())]);
            if let Some(positions) = recorded {
                expected.update(&positions.bytes());
            }
            let observation = Observation {
                semantic,
                positions: recorded,
            };
            assert_eq!(
                observation.identity(FRAMING).bytes(),
                *expected.finalize().as_bytes()
            );
        }

        let mut frame = FRAMING.frame("d");
        frame.uint(7);
        frame.flag(true);
        frame.text("ab");
        frame.digest(semantic);
        frame.json(&serde_json::json!({"k": [null, false, "s", 1]}));
        let mut expected = opened(b"d");
        expected.update(&7_u64.to_le_bytes());
        expected.update(&[1]);
        prefixed(&mut expected, b"ab");
        expected.update(&semantic.bytes());
        // An object of one member, then an array of four values: each value
        // is its tag, then its own content.
        for tag in [5_u64, 1] {
            expected.update(&tag.to_le_bytes());
        }
        prefixed(&mut expected, b"k");
        for tag in [4_u64, 4, 0, 1] {
            expected.update(&tag.to_le_bytes());
        }
        expected.update(&[0]);
        expected.update(&3_u64.to_le_bytes());
        prefixed(&mut expected, b"s");
        expected.update(&2_u64.to_le_bytes());
        prefixed(&mut expected, b"1");
        assert_eq!(frame.finish().bytes(), *expected.finalize().as_bytes());
    }

    #[test]
    fn json_framing_keeps_the_models_own_member_order() {
        // A projection's member order is part of what it means, so two objects
        // that differ only in order are different observations.
        let mut first = FRAMING.frame("d");
        first.json(&serde_json::json!({"a": "1", "b": "2"}));
        let mut second = FRAMING.frame("d");
        second.json(&serde_json::json!({"b": "2", "a": "1"}));
        assert_ne!(first.finish(), second.finish());
    }
}
