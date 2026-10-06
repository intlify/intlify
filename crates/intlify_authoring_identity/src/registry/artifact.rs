// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The `intent-registry` and `intent-registry-update` kinds of design 017's
//! authoring artifact.
//!
//! The envelope, its digest and its references are shared by every kind and
//! live in `intlify_authoring::artifact`. What is here is each kind's
//! single-value tag and its named type, which is what each committed schema is
//! generated from.

use intlify_authoring::artifact::{AuthoringSpecification, RevisionZero};
use intlify_authoring::{ArtifactKind, AuthoringArtifact, IntegrityDigest};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::snapshot::IntentRegistrySnapshot;
use super::update::IntentRegistryUpdate;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum RegistryKind {
    #[serde(rename = "intent-registry")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum RegistryUpdateKind {
    #[serde(rename = "intent-registry-update")]
    Value,
}

/// One sealed `intent-registry` artifact.
///
/// This is the `intent-registry` case of 017's `AuthoringArtifact<K, B>`.
/// Deserializing one does not check its digest; admission does, before
/// anything reads the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryArtifact {
    kind: RegistryKind,
    schema_revision: RevisionZero,
    authoring_specification: AuthoringSpecification,
    body: IntentRegistrySnapshot,
    integrity_digest: IntegrityDigest,
}

impl AuthoringArtifact for RegistryArtifact {
    type Body = IntentRegistrySnapshot;

    const KIND: ArtifactKind = ArtifactKind::IntentRegistry;

    fn envelope(body: IntentRegistrySnapshot, integrity_digest: IntegrityDigest) -> Self {
        Self {
            kind: RegistryKind::Value,
            schema_revision: RevisionZero::Value,
            authoring_specification: AuthoringSpecification::CURRENT,
            body,
            integrity_digest,
        }
    }

    fn into_body(self) -> IntentRegistrySnapshot {
        self.body
    }

    fn body(&self) -> &IntentRegistrySnapshot {
        &self.body
    }

    fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }
}

/// One sealed `intent-registry-update` artifact.
///
/// This is the `intent-registry-update` case of 017's `AuthoringArtifact<K,
/// B>`. Deserializing one does not check its digest; admission does, before
/// anything reads the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegistryUpdateArtifact {
    kind: RegistryUpdateKind,
    schema_revision: RevisionZero,
    authoring_specification: AuthoringSpecification,
    body: IntentRegistryUpdate,
    integrity_digest: IntegrityDigest,
}

impl AuthoringArtifact for RegistryUpdateArtifact {
    type Body = IntentRegistryUpdate;

    const KIND: ArtifactKind = ArtifactKind::IntentRegistryUpdate;

    fn envelope(body: IntentRegistryUpdate, integrity_digest: IntegrityDigest) -> Self {
        Self {
            kind: RegistryUpdateKind::Value,
            schema_revision: RevisionZero::Value,
            authoring_specification: AuthoringSpecification::CURRENT,
            body,
            integrity_digest,
        }
    }

    fn into_body(self) -> IntentRegistryUpdate {
        self.body
    }

    fn body(&self) -> &IntentRegistryUpdate {
        &self.body
    }

    fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }
}
