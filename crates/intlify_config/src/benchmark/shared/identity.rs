// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed 017 token, instance, digest, and Case-ID representations. An instance
//! ID is neither a semantic checksum nor an assertion about a trusted runner.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de, Deserialize, Deserializer, Serialize};

use crate::model::{valid_identity, ID_PATTERN};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::benchmark) enum IdentityFailure {
    InvalidToken,
    EntropyUnavailable,
}

fn hex64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(hex64)
}
fn case(value: &str) -> bool {
    value.strip_prefix("mc0_").is_some_and(hex64)
}

macro_rules! string_type {
    ($name:ident, $check:ident, $pattern:expr, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub(in crate::benchmark) struct $name(String);

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                if $check(&value) { Ok(Self(value)) } else { Err(de::Error::custom($label)) }
            }
        }
        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> { stringify!($name).into() }
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                schemars::json_schema!({"type": "string", "pattern": $pattern})
            }
        }
    };
}

string_type!(
    Token,
    valid_identity,
    ID_PATTERN,
    "invalid exact identity token"
);
string_type!(Hex256, hex64, "^[0-9a-f]{64}$", "invalid 256-bit value");
string_type!(
    IntegrityDigest,
    digest,
    "^sha256:[0-9a-f]{64}$",
    "invalid shared SHA-256 digest"
);
string_type!(
    CaseIdentity,
    case,
    "^mc0_[0-9a-f]{64}$",
    "invalid Measurement Case identity"
);

impl Token {
    pub(in crate::benchmark) fn new(value: &str) -> Result<Self, IdentityFailure> {
        valid_identity(value)
            .then(|| Self(value.into()))
            .ok_or(IdentityFailure::InvalidToken)
    }
    pub(in crate::benchmark) fn literal(value: &'static str) -> Self {
        Self::new(value).expect("registered literal token")
    }
}

fn hex(bytes: [u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(64);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    text
}

impl IntegrityDigest {
    pub(in crate::benchmark) fn from_hash(bytes: [u8; 32]) -> Self {
        Self(format!("sha256:{}", hex(bytes)))
    }
}
impl CaseIdentity {
    pub(in crate::benchmark) fn from_hash(bytes: [u8; 32]) -> Self {
        Self(format!("mc0_{}", hex(bytes)))
    }
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
pub(in crate::benchmark) enum InstanceDomain {
    #[serde(rename = "intlify-verification-record-v0")]
    Record,
    #[serde(rename = "intlify-measurement-run-v0")]
    Run,
    // Native schema v1 explicitly gives one fresh immutable ID to the one-shot
    // owner result. The native content checksum remains independently retained.
    #[serde(rename = "intlify-config-owner-result-v1")]
    NativeOwnerResult,
    // Per-run local harness instance, not a machine identity or qualification.
    #[serde(rename = "intlify-config-local-runner-instance-v0")]
    LocalRunnerInstance,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::benchmark) struct RecordIdentity {
    domain: InstanceDomain,
    value: Hex256,
}

impl RecordIdentity {
    pub(in crate::benchmark) fn fresh(domain: InstanceDomain) -> Result<Self, IdentityFailure> {
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        {
            let mut bytes = [0; 32];
            getrandom::fill(&mut bytes).map_err(|_| IdentityFailure::EntropyUnavailable)?;
            Ok(Self {
                domain,
                value: Hex256(hex(bytes)),
            })
        }
        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            let _ = domain;
            Err(IdentityFailure::EntropyUnavailable)
        }
    }
    pub(in crate::benchmark) const fn domain(&self) -> InstanceDomain {
        self.domain
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(in crate::benchmark) struct VersionedIdentity {
    identity: Token,
    revision: Token,
}

impl VersionedIdentity {
    pub(in crate::benchmark) fn new(
        identity: &str,
        revision: &str,
    ) -> Result<Self, IdentityFailure> {
        Ok(Self {
            identity: Token::new(identity)?,
            revision: Token::new(revision)?,
        })
    }
    pub(in crate::benchmark) fn specification() -> Self {
        Self::new("intlify-design-026", "0").expect("registered specification")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn token_digest_and_case_encodings_are_closed_and_do_not_coerce_values() {
        for text in ["0", "intlify-design-026", "0.14.0-alpha.12", "a_b.c"] {
            assert!(serde_json::from_value::<Token>(json!(text)).is_ok());
        }
        for text in ["", "Upper", "a-", "-a", "a/b", "é", "a\n"] {
            assert!(serde_json::from_value::<Token>(json!(text)).is_err());
        }
        let digest = IntegrityDigest::from_hash([0xab; 32]);
        let case = CaseIdentity::from_hash([0xab; 32]);
        assert_eq!(
            serde_json::to_value(&digest).unwrap(),
            json!(format!("sha256:{}", "ab".repeat(32)))
        );
        assert_eq!(
            serde_json::to_value(&case).unwrap(),
            json!(format!("mc0_{}", "ab".repeat(32)))
        );
        for text in [
            "ab".repeat(32),
            format!("sha256:{}", "AB".repeat(32)),
            format!("sha256:{}", "a".repeat(63)),
            format!("sha256:{}", "g".repeat(64)),
        ] {
            assert!(serde_json::from_value::<IntegrityDigest>(json!(text)).is_err());
        }
        assert!(
            serde_json::from_value::<CaseIdentity>(serde_json::to_value(digest).unwrap()).is_err()
        );
        for value in [json!(0), json!(null), json!({}), json!([])] {
            assert!(serde_json::from_value::<Token>(value.clone()).is_err());
            assert!(serde_json::from_value::<IntegrityDigest>(value).is_err());
        }
    }

    #[test]
    fn identity_domains_and_complete_pairs_are_not_interchangeable() {
        let original = json!({"domain": "intlify-verification-record-v0", "value": "0".repeat(64)});
        let first: RecordIdentity = serde_json::from_value(original.clone()).unwrap();
        let mut changed = original.clone();
        changed["domain"] = json!("intlify-measurement-run-v0");
        assert_ne!(
            first,
            serde_json::from_value::<RecordIdentity>(changed).unwrap()
        );
        for action in 0..4 {
            let mut invalid = original.clone();
            match action {
                0 => invalid["domain"] = json!("unknown-domain"),
                1 => invalid["value"] = json!("0".repeat(63)),
                2 => {
                    invalid.as_object_mut().unwrap().remove("domain");
                }
                _ => invalid["extra"] = json!(true),
            }
            assert!(serde_json::from_value::<RecordIdentity>(invalid).is_err());
        }
    }
}
