// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The 017 string primitives the locale canonicalization data uses and no
//! shared crate holds yet. They check spelling only.

use std::borrow::Cow;

use intlify_shared_json::encoding::digest_bytes;
use intlify_shared_json::token::{hex, valid_hex256};
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{Deserialize, Deserializer, Serialize};

// A JSON Schema pattern is an ECMA-262 regular expression, which has no inline
// dot-all flag, so the class spells out "any scalar, newline included".
intlify_shared_json::shared_string_type!(
    pub NonemptyText,
    nonempty,
    "^[\\s\\S]+$",
    "value must be a nonempty string"
);

intlify_shared_json::shared_string_type!(
    pub SemanticDigest,
    sha256_presentation,
    "^sha256:[0-9a-f]{64}$",
    "invalid semantic digest"
);

intlify_shared_json::shared_string_type!(
    pub FileDigest,
    sha256_presentation,
    "^sha256:[0-9a-f]{64}$",
    "invalid file digest"
);

/// One 017 `LogicalAddress`: segments of `[a-z0-9][a-z0-9._-]{0,254}` joined
/// by `/`, at most 64 segments and 4096 bytes, with no leading `/` and no empty
/// segment. A payload's address is relative to the manifest's location.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(transparent)]
pub struct LogicalAddress(String);

const ADDRESS_BYTES: usize = 4096;
const ADDRESS_SEGMENTS: usize = 64;
const SEGMENT_BYTES: usize = 255;

fn nonempty(value: &str) -> bool {
    !value.is_empty()
}

fn sha256_presentation(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(valid_hex256)
}

fn logical_address(value: &str) -> bool {
    value.len() <= ADDRESS_BYTES
        && value.split('/').count() <= ADDRESS_SEGMENTS
        && value.split('/').all(segment)
}

// A segment's first byte rules out `.` and `..` as well as an empty segment.
fn segment(value: &str) -> bool {
    let bytes = value.as_bytes();
    let alphanumeric = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    bytes.first().is_some_and(|byte| alphanumeric(*byte))
        && bytes.len() <= SEGMENT_BYTES
        && bytes
            .iter()
            .all(|byte| alphanumeric(*byte) || matches!(byte, b'.' | b'_' | b'-'))
}

impl SemanticDigest {
    /// Present a complete SHA-256 result in its `sha256:` spelling.
    pub fn from_hash(bytes: [u8; 32]) -> Self {
        Self(format!("sha256:{}", hex(bytes)))
    }
}

impl FileDigest {
    /// The SHA-256 of exactly these bytes, outside the value framing.
    pub fn of(bytes: &[u8]) -> Self {
        Self(format!("sha256:{}", hex(digest_bytes(bytes))))
    }
}

impl LogicalAddress {
    /// Validate and retain one address.
    pub fn new(value: &str) -> Option<Self> {
        logical_address(value).then(|| Self(value.to_owned()))
    }

    /// Borrow the exact spelling.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for LogicalAddress {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::new(&value).ok_or_else(|| serde::de::Error::custom("invalid logical address"))
    }
}

impl JsonSchema for LogicalAddress {
    fn schema_name() -> Cow<'static, str> {
        "LogicalAddress".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        schemars::json_schema!({
            "type": "string",
            "pattern": "^[a-z0-9][a-z0-9._-]{0,254}(?:/[a-z0-9][a-z0-9._-]{0,254}){0,63}$",
            "maxLength": ADDRESS_BYTES,
        })
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn nonempty_text_is_any_string_but_the_empty_one() {
        for text in ["a", " ", "\n", "en-US", "日本"] {
            let parsed: NonemptyText = serde_json::from_value(json!(text)).unwrap();
            assert_eq!(parsed.as_str(), text);
            assert_eq!(NonemptyText::from_validated(text).unwrap().as_str(), text);
        }
        assert!(serde_json::from_value::<NonemptyText>(json!("")).is_err());
        assert!(NonemptyText::from_validated("").is_err());
        assert!(serde_json::from_value::<NonemptyText>(json!(["a"])).is_err());
    }

    #[test]
    fn a_semantic_digest_is_exactly_sha256_and_64_lowercase_hex_digits() {
        let digest = SemanticDigest::from_hash([0xab; 32]);
        assert_eq!(digest.as_str(), format!("sha256:{}", "ab".repeat(32)));
        assert_eq!(
            serde_json::from_value::<SemanticDigest>(json!(digest.as_str())).unwrap(),
            digest
        );
        for text in [
            String::new(),
            "ab".repeat(32),
            format!("sha256:{}", "AB".repeat(32)),
            format!("sha256:{}", "ab".repeat(31)),
            format!("sha256:{}0", "ab".repeat(32)),
            format!("sha512:{}", "ab".repeat(32)),
            format!("sha256:{}\n", "ab".repeat(32)),
        ] {
            assert!(SemanticDigest::from_validated(&text).is_err(), "{text:?}");
        }
    }

    #[test]
    fn a_file_digest_is_the_plain_sha256_of_the_bytes() {
        // A published SHA-256 answer, not this crate's output.
        assert_eq!(
            FileDigest::of(b"abc").as_str(),
            "sha256:ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert!(FileDigest::from_validated(&format!("sha256:{}", "AB".repeat(32))).is_err());
        assert!(serde_json::from_value::<FileDigest>(json!("sha256:ab")).is_err());
    }

    #[test]
    fn a_logical_address_has_lowercase_segments_and_no_empty_or_dot_segment() {
        for address in [
            "a",
            "0",
            "conformance.json",
            "common/bcp47/calendar.xml",
            "a-b_c.d/e9",
        ] {
            assert_eq!(LogicalAddress::new(address).unwrap().as_str(), address);
            let parsed: LogicalAddress = serde_json::from_value(json!(address)).unwrap();
            assert_eq!(parsed.as_str(), address);
        }
        for address in [
            "", "/a", "a/", "a//b", ".", "..", "a/../b", ".hidden", "-a", "_a", "A", "a b", "a\\b",
            "日本",
        ] {
            assert!(LogicalAddress::new(address).is_none(), "{address:?}");
            assert!(serde_json::from_value::<LogicalAddress>(json!(address)).is_err());
        }
    }

    #[test]
    fn a_logical_address_is_bounded_by_segment_length_count_and_bytes() {
        let segment = "a".repeat(SEGMENT_BYTES);
        assert!(LogicalAddress::new(&segment).is_some());
        assert!(LogicalAddress::new(&format!("{segment}a")).is_none());

        let segments = vec!["a"; ADDRESS_SEGMENTS].join("/");
        assert!(LogicalAddress::new(&segments).is_some());
        assert!(LogicalAddress::new(&format!("{segments}/a")).is_none());

        // 16 segments of 255 bytes and their 15 separators are 4095 bytes.
        let long = vec![segment.as_str(); 16].join("/");
        assert_eq!(long.len(), ADDRESS_BYTES - 1);
        assert!(LogicalAddress::new(&format!("{long}a")).is_none());
        let exact = format!(
            "{}/{}",
            vec![segment.as_str(); 15].join("/"),
            "a".repeat(SEGMENT_BYTES - 1)
        );
        assert_eq!(exact.len(), ADDRESS_BYTES - 2);
        let at_bound = format!("{exact}/a");
        assert_eq!(at_bound.len(), ADDRESS_BYTES);
        assert!(LogicalAddress::new(&at_bound).is_some());
        assert!(LogicalAddress::new(&format!("{at_bound}a")).is_none());
    }

    #[test]
    fn the_address_schema_states_the_same_grammar() {
        let schema = crate::schema::draft7_schema::<LogicalAddress>().unwrap();
        let validator = jsonschema::draft7::new(&schema).unwrap();
        let segment = "a".repeat(SEGMENT_BYTES);
        for (address, admitted) in [
            ("common/bcp47/calendar.xml", true),
            ("/a", false),
            ("a//b", false),
            ("..", false),
            ("A", false),
            (segment.as_str(), true),
        ] {
            assert_eq!(validator.is_valid(&json!(address)), admitted, "{address}");
        }
        let segments = vec!["a"; ADDRESS_SEGMENTS + 1].join("/");
        assert!(!validator.is_valid(&json!(segments)));
        let bytes = format!("{}/{}", vec![segment.as_str(); 16].join("/"), "a");
        assert_eq!(bytes.len(), ADDRESS_BYTES + 1);
        assert!(!validator.is_valid(&json!(bytes)));
    }
}
