// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The `message-intent` and `message-reference` kinds of design 017's
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

use super::model::{MessageIntentBody, MessageReferenceBody};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum MessageIntentKind {
    #[serde(rename = "message-intent")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum MessageReferenceKind {
    #[serde(rename = "message-reference")]
    Value,
}

/// One sealed `message-intent` artifact.
///
/// This is the `message-intent` case of 017's `AuthoringArtifact<K, B>`.
/// Deserializing one does not check its digest; admission does, before
/// anything reads the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageIntentArtifact {
    kind: MessageIntentKind,
    schema_revision: RevisionZero,
    authoring_specification: AuthoringSpecification,
    body: MessageIntentBody,
    integrity_digest: IntegrityDigest,
}

impl AuthoringArtifact for MessageIntentArtifact {
    type Body = MessageIntentBody;

    const KIND: ArtifactKind = ArtifactKind::MessageIntent;

    fn envelope(body: MessageIntentBody, integrity_digest: IntegrityDigest) -> Self {
        Self {
            kind: MessageIntentKind::Value,
            schema_revision: RevisionZero::Value,
            authoring_specification: AuthoringSpecification::CURRENT,
            body,
            integrity_digest,
        }
    }

    fn into_body(self) -> MessageIntentBody {
        self.body
    }

    fn body(&self) -> &MessageIntentBody {
        &self.body
    }

    fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }
}

/// One sealed `message-reference` artifact.
///
/// This is the `message-reference` case of 017's `AuthoringArtifact<K, B>`.
/// Deserializing one does not check its digest; admission does, before
/// anything reads the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MessageReferenceArtifact {
    kind: MessageReferenceKind,
    schema_revision: RevisionZero,
    authoring_specification: AuthoringSpecification,
    body: MessageReferenceBody,
    integrity_digest: IntegrityDigest,
}

impl AuthoringArtifact for MessageReferenceArtifact {
    type Body = MessageReferenceBody;

    const KIND: ArtifactKind = ArtifactKind::MessageReference;

    fn envelope(body: MessageReferenceBody, integrity_digest: IntegrityDigest) -> Self {
        Self {
            kind: MessageReferenceKind::Value,
            schema_revision: RevisionZero::Value,
            authoring_specification: AuthoringSpecification::CURRENT,
            body,
            integrity_digest,
        }
    }

    fn into_body(self) -> MessageReferenceBody {
        self.body
    }

    fn body(&self) -> &MessageReferenceBody {
        &self.body
    }

    fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }
}
