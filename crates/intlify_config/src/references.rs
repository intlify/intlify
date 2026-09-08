// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed 017 reference values for the 015 configuration schema, revision 0.
//! These types prove structure only. No acquisition, body digest computation,
//! artifact-version support, role validation, or trust is implied by decoding.

use std::borrow::Cow;

use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de, Deserialize, Deserializer, Serialize};

use crate::model::{valid_identity, ID_PATTERN};

pub(crate) const SEMANTIC_DIGEST_PATTERN: &str = "^sha256:[0-9a-f]{64}$";

pub(crate) fn valid_semantic_digest(value: &str) -> bool {
    value.strip_prefix("sha256:").is_some_and(|digits| {
        digits.len() == 64
            && digits
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    })
}

macro_rules! token_type {
    ($name:ident, $check:ident, $pattern:ident, $expectation:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
        #[serde(transparent)]
        struct $name(String);

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let value = String::deserialize(deserializer)?;
                if $check(&value) {
                    Ok(Self(value))
                } else {
                    // Invalid input contents are not echoed in diagnostics.
                    Err(de::Error::custom($expectation))
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
    };
}

token_type!(
    ReferenceIdentity,
    valid_identity,
    ID_PATTERN,
    "invalid artifact identity syntax"
);
token_type!(
    ReferenceRevision,
    valid_identity,
    ID_PATTERN,
    "invalid exact revision syntax"
);
token_type!(
    SemanticDigest,
    valid_semantic_digest,
    SEMANTIC_DIGEST_PATTERN,
    "invalid full SHA-256 semantic digest"
);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum PolicyKind {
    ResourceLimitPolicy,
    TrustPolicy,
    SourceAdmissionPolicy,
    ApprovalPolicy,
    SelectionPolicy,
    ProviderRoutingPolicy,
    GlossarySet,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum TargetKind {
    #[serde(rename = "target-profile")]
    TargetProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PolicyReference {
    kind: PolicyKind,
    identity: ReferenceIdentity,
    revision: ReferenceRevision,
    specification_revision: ReferenceRevision,
    semantic_digest: SemanticDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TargetProfileReference {
    kind: TargetKind,
    identity: ReferenceIdentity,
    revision: ReferenceRevision,
    specification_revision: ReferenceRevision,
    semantic_digest: SemanticDigest,
}
