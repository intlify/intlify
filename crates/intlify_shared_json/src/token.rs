// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed 017 string primitives: exact identity/revision tokens, 256-bit
//! values, shared SHA-256 digests, and versioned identity pairs.
//!
//! These types validate spelling only. A well-formed token is not proof of a
//! registered identity, a supported revision, an admitted artifact, or an
//! authorized publisher.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de, Deserialize, Deserializer, Serialize};

/// Complete ASCII grammar for `IdentityToken` and `RevisionToken`.
///
/// The pattern matches the entire string. No normalization, trimming, case
/// folding, or trailing-line-terminator tolerance is applied anywhere.
pub const ID_PATTERN: &str = "^[a-z0-9](?:[a-z0-9._-]*[a-z0-9])?$";

/// Return whether `value` matches [`ID_PATTERN`] in full.
#[must_use]
pub fn valid_identity(value: &str) -> bool {
    let bytes = value.as_bytes();
    let endpoint = |byte: u8| byte.is_ascii_lowercase() || byte.is_ascii_digit();
    bytes.first().is_some_and(|byte| endpoint(*byte))
        && bytes.last().is_some_and(|byte| endpoint(*byte))
        && bytes
            .iter()
            .all(|byte| endpoint(*byte) || matches!(byte, b'.' | b'_' | b'-'))
}

/// Complete failure of a token or identity construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityFailure {
    /// The supplied spelling is outside the exact grammar for its domain.
    InvalidToken,
    /// Operating-system cryptographic randomness could not be acquired.
    ///
    /// Retained here so owners that mint instance identities report one
    /// explicit failure instead of substituting a timestamp or counter.
    EntropyUnavailable,
}

/// Return whether `value` is exactly 64 lowercase hexadecimal digits.
#[must_use]
pub fn valid_hex256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

fn valid_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(valid_hex256)
}

/// Render 32 bytes as exactly 64 lowercase hexadecimal digits.
#[must_use]
pub fn hex(bytes: [u8; 32]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(64);
    for byte in bytes {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 15)]));
    }
    text
}

/// Define a closed, transparently encoded string newtype with an exact grammar.
///
/// Owners use this for the string domains they register, such as a Measurement
/// Case identity or an opaque authoring ID. The generated type rejects every
/// non-string JSON value and never coerces or repairs a spelling.
///
/// `$check` is a `fn(&str) -> bool` path, `$pattern` the complete JSON Schema
/// pattern, and `$label` the decoder's failure text. The text must not
/// interpolate the rejected value.
#[macro_export]
macro_rules! shared_string_type {
    ($visibility:vis $name:ident, $check:path, $pattern:expr, $label:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, ::serde::Serialize)]
        #[serde(transparent)]
        $visibility struct $name(String);

        impl<'de> ::serde::Deserialize<'de> for $name {
            fn deserialize<D: ::serde::Deserializer<'de>>(
                deserializer: D,
            ) -> ::core::result::Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                if $check(&value) {
                    ::core::result::Result::Ok(Self(value))
                } else {
                    ::core::result::Result::Err(::serde::de::Error::custom($label))
                }
            }
        }

        impl ::schemars::JsonSchema for $name {
            fn schema_name() -> ::std::borrow::Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(_: &mut ::schemars::SchemaGenerator) -> ::schemars::Schema {
                ::schemars::json_schema!({"type": "string", "pattern": $pattern})
            }
        }

        impl $name {
            /// Retain one already validated spelling.
            #[allow(dead_code)]
            $visibility fn from_validated(
                value: &str,
            ) -> ::core::result::Result<Self, $crate::token::IdentityFailure> {
                if $check(value) {
                    ::core::result::Result::Ok(Self(value.to_owned()))
                } else {
                    ::core::result::Result::Err($crate::token::IdentityFailure::InvalidToken)
                }
            }

            /// Borrow the exact retained spelling.
            #[allow(dead_code)]
            $visibility fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

macro_rules! string_type {
    ($name:ident, $check:path, $pattern:expr, $label:literal, $doc:literal) => {
        #[doc = $doc]
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                if $check(&value) {
                    Ok(Self(value))
                } else {
                    Err(de::Error::custom($label))
                }
            }
        }

        impl JsonSchema for $name {
            fn schema_name() -> Cow<'static, str> {
                stringify!($name).into()
            }
            fn json_schema(_: &mut SchemaGenerator) -> Schema {
                schemars::json_schema!({"type": "string", "pattern": $pattern})
            }
        }

        impl $name {
            /// Borrow the exact retained spelling.
            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

string_type!(
    Token,
    valid_identity,
    ID_PATTERN,
    "invalid exact identity token",
    "One exact `IdentityToken` or `RevisionToken`.\n\nA revision is a literal pin. It is never a range, branch, mutable tag,\ntimestamp selection, or `latest` lookup."
);
string_type!(
    Hex256,
    valid_hex256,
    "^[0-9a-f]{64}$",
    "invalid 256-bit value",
    "Exactly 64 lowercase hexadecimal digits retaining all 256 bits."
);
string_type!(
    IntegrityDigest,
    valid_digest,
    "^sha256:[0-9a-f]{64}$",
    "invalid shared SHA-256 digest",
    "One complete `sha256:` digest presentation.\n\nEquality of digests is not proof of equal content; owners with both\nvalues available compare canonical content when resolving conflicts."
);

impl Token {
    /// Validate and retain one exact token.
    pub fn new(value: &str) -> Result<Self, IdentityFailure> {
        valid_identity(value)
            .then(|| Self(value.into()))
            .ok_or(IdentityFailure::InvalidToken)
    }

    /// Retain one token spelling that the caller registers as a literal.
    ///
    /// # Panics
    ///
    /// Panics when the literal is outside [`ID_PATTERN`]. Registered literals
    /// are implementation constants, so an invalid one is a defect rather than
    /// an input failure.
    #[must_use]
    pub fn literal(value: &'static str) -> Self {
        Self::new(value).expect("registered literal token")
    }
}

impl Hex256 {
    /// Present 32 bytes as one 256-bit value.
    #[must_use]
    pub fn from_hash(bytes: [u8; 32]) -> Self {
        Self(hex(bytes))
    }
}

impl IntegrityDigest {
    /// Present a complete SHA-256 result in its shared `sha256:` spelling.
    #[must_use]
    pub fn from_hash(bytes: [u8; 32]) -> Self {
        Self(format!("sha256:{}", hex(bytes)))
    }
}

// The closed `{ identity, revision }` pair; both members are required. Both
// parts are compared completely, so equal identities under different revisions
// denote different pinned versions.
//
// Schemars derives a container description from a doc comment, and adopters
// pin the generated schema bytes. This explanation therefore stays a plain
// comment so the generated artifact is unchanged by documenting the type.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VersionedIdentity {
    identity: Token,
    revision: Token,
}

impl VersionedIdentity {
    /// Validate and retain one versioned identity.
    pub fn new(identity: &str, revision: &str) -> Result<Self, IdentityFailure> {
        Ok(Self {
            identity: Token::new(identity)?,
            revision: Token::new(revision)?,
        })
    }

    /// Retain one pair that the caller registers as a literal.
    ///
    /// # Panics
    ///
    /// Panics when either literal is outside [`ID_PATTERN`].
    #[must_use]
    pub fn literal(identity: &'static str, revision: &'static str) -> Self {
        Self::new(identity, revision).expect("registered literal identity")
    }

    /// Borrow the identity part.
    #[must_use]
    pub const fn identity(&self) -> &Token {
        &self.identity
    }

    /// Borrow the revision part.
    #[must_use]
    pub const fn revision(&self) -> &Token {
        &self.revision
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn token_digest_and_hex_encodings_are_closed_and_do_not_coerce_values() {
        for text in ["0", "intlify-design-026", "0.14.0-alpha.12", "a_b.c"] {
            assert!(serde_json::from_value::<Token>(json!(text)).is_ok());
        }
        for text in ["", "Upper", "a-", "-a", "a/b", "é", "a\n", " a"] {
            assert!(serde_json::from_value::<Token>(json!(text)).is_err());
        }
        let digest = IntegrityDigest::from_hash([0xab; 32]);
        assert_eq!(
            serde_json::to_value(&digest).unwrap(),
            json!(format!("sha256:{}", "ab".repeat(32)))
        );
        assert_eq!(
            serde_json::to_value(Hex256::from_hash([0xab; 32])).unwrap(),
            json!("ab".repeat(32))
        );
        for text in [
            "ab".repeat(32),
            format!("sha256:{}", "AB".repeat(32)),
            format!("sha256:{}", "a".repeat(63)),
            format!("sha256:{}", "g".repeat(64)),
        ] {
            assert!(serde_json::from_value::<IntegrityDigest>(json!(text)).is_err());
        }
        assert!(serde_json::from_value::<Hex256>(serde_json::to_value(digest).unwrap()).is_err());
        for value in [json!(0), json!(null), json!({}), json!([])] {
            assert!(serde_json::from_value::<Token>(value.clone()).is_err());
            assert!(serde_json::from_value::<IntegrityDigest>(value).is_err());
        }
    }

    #[test]
    fn hex_rendering_is_lowercase_and_fixed_width() {
        assert_eq!(hex([0; 32]), "0".repeat(64));
        assert_eq!(hex([0xff; 32]), "f".repeat(64));
        let mut bytes = [0; 32];
        bytes[0] = 0x0a;
        bytes[31] = 0xb0;
        let text = hex(bytes);
        assert_eq!(text.len(), 64);
        assert!(text.starts_with("0a") && text.ends_with("b0"));
        assert!(valid_hex256(&text));
    }

    #[test]
    fn versioned_identity_compares_both_parts_and_rejects_unknown_members() {
        let first = VersionedIdentity::literal("intlify-design-026", "0");
        assert_eq!(first.identity().as_str(), "intlify-design-026");
        assert_eq!(first.revision().as_str(), "0");
        assert_ne!(first, VersionedIdentity::literal("intlify-design-026", "1"));
        assert_ne!(first, VersionedIdentity::literal("intlify-design-017", "0"));
        assert_eq!(
            serde_json::to_value(&first).unwrap(),
            json!({"identity": "intlify-design-026", "revision": "0"})
        );
        for invalid in [
            json!({"identity": "a"}),
            json!({"identity": "a", "revision": "0", "extra": true}),
            json!({"identity": "A", "revision": "0"}),
            json!({"identity": "a", "revision": null}),
        ] {
            assert!(serde_json::from_value::<VersionedIdentity>(invalid).is_err());
        }
        assert_eq!(
            VersionedIdentity::new("a", "-"),
            Err(IdentityFailure::InvalidToken)
        );
    }

    #[test]
    fn malformed_spellings_do_not_enter_decoder_error_text() {
        let error = serde_json::from_str::<Token>("\"SECRET-not-a-token\"").unwrap_err();
        assert!(!error.to_string().contains("SECRET-not-a-token"));
    }
}
