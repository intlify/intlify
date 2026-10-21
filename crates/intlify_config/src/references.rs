// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed 017 reference values for the 015 configuration schema, revision 0.
//! These types prove structure only. No acquisition, body digest computation,
//! artifact-version support, role validation, or trust is implied by decoding.
//! A reference names its exact tuple and has 017's canonical bytes; matching it
//! to a submitted artifact is 015's.

use std::borrow::Cow;

use intlify_shared_json::encoding::encode;
use intlify_shared_json::token::Token;
use schemars::{JsonSchema, Schema, SchemaGenerator};
use serde::{de, Deserialize, Deserializer, Serialize};

use crate::artifacts::envelope::{ArtifactKind, ExactTuple};
use crate::artifacts::{count, SemanticDigest as DigestValue};
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

impl PolicyKind {
    const fn artifact_kind(self) -> ArtifactKind {
        match self {
            Self::ResourceLimitPolicy => ArtifactKind::ResourceLimitPolicy,
            Self::TrustPolicy => ArtifactKind::TrustPolicy,
            Self::SourceAdmissionPolicy => ArtifactKind::SourceAdmissionPolicy,
            Self::ApprovalPolicy => ArtifactKind::ApprovalPolicy,
            Self::SelectionPolicy => ArtifactKind::SelectionPolicy,
            Self::ProviderRoutingPolicy => ArtifactKind::ProviderRoutingPolicy,
            Self::GlossarySet => ArtifactKind::GlossarySet,
        }
    }
}

/// The exact tuple a reference names, from parts its decoding validated.
fn tuple(
    kind: ArtifactKind,
    identity: &str,
    revision: &str,
    specification_revision: &str,
    semantic_digest: &str,
) -> ExactTuple {
    let token = |text| Token::new(text).expect("a decoded reference token is a token");
    ExactTuple {
        kind,
        identity: token(identity),
        revision: token(revision),
        specification_revision: token(specification_revision),
        semantic_digest: DigestValue::from_validated(semantic_digest)
            .expect("a decoded reference digest is a digest"),
    }
}

/// The length of the reference's canonical value framing, whatever its
/// transport spelling.
fn canonical_bytes(reference: &impl Serialize) -> u64 {
    let value = serde_json::to_value(reference).expect("a reference serializes");
    count(
        encode(&value)
            .expect("a five-member reference frames")
            .len(),
    )
}

impl PolicyReference {
    /// The exact tuple this reference names: its kind, identity, revision,
    /// specification revision, and semantic digest.
    pub(crate) fn tuple(&self) -> ExactTuple {
        tuple(
            self.kind.artifact_kind(),
            &self.identity.0,
            &self.revision.0,
            &self.specification_revision.0,
            &self.semantic_digest.0,
        )
    }

    /// The canonical bytes 015's `artifactAdmission` reference bounds count.
    pub(crate) fn canonical_bytes(&self) -> u64 {
        canonical_bytes(self)
    }
}

impl TargetProfileReference {
    /// The exact tuple this reference names: its kind, identity, revision,
    /// specification revision, and semantic digest.
    pub(crate) fn tuple(&self) -> ExactTuple {
        tuple(
            ArtifactKind::TargetProfile,
            &self.identity.0,
            &self.revision.0,
            &self.specification_revision.0,
            &self.semantic_digest.0,
        )
    }

    /// The canonical bytes 015's `artifactAdmission` reference bounds count.
    pub(crate) fn canonical_bytes(&self) -> u64 {
        canonical_bytes(self)
    }
}
