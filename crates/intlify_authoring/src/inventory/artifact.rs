// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The `authoring-inventory` kind of design 017's authoring artifact.
//!
//! The envelope, its digest and its references are shared by every kind and
//! live in [`crate::artifact`]. What is here is this kind's single-value tag
//! and its named type, which is what the committed inventory schema is
//! generated from.

use intlify_shared_json::encoding::EncodingFailure;
use intlify_shared_json::token::IntegrityDigest;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::AuthoringInventory;
use crate::artifact::{
    ArtifactKind, ArtifactRelation, AuthoringArtifact, AuthoringArtifactReference,
    AuthoringSpecification, RevisionZero,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum InventoryKind {
    #[serde(rename = "authoring-inventory")]
    Value,
}

/// One sealed `authoring-inventory` artifact.
///
/// This is the `authoring-inventory` case of 017's `AuthoringArtifact<K, B>`.
/// Deserializing one does not check its digest; [`Self::verify_integrity`]
/// does, and admission calls it before anything reads the body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InventoryArtifact {
    kind: InventoryKind,
    schema_revision: RevisionZero,
    authoring_specification: AuthoringSpecification,
    body: AuthoringInventory,
    integrity_digest: IntegrityDigest,
}

impl AuthoringArtifact for InventoryArtifact {
    type Body = AuthoringInventory;

    const KIND: ArtifactKind = ArtifactKind::AuthoringInventory;

    fn envelope(body: AuthoringInventory, integrity_digest: IntegrityDigest) -> Self {
        Self {
            kind: InventoryKind::Value,
            schema_revision: RevisionZero::Value,
            authoring_specification: AuthoringSpecification::CURRENT,
            body,
            integrity_digest,
        }
    }

    fn into_body(self) -> AuthoringInventory {
        self.body
    }

    fn body(&self) -> &AuthoringInventory {
        &self.body
    }

    fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }
}

// The inventory kind predates the shared trait, and its callers name these
// methods directly. Each one is the shared behaviour, not a second copy of it.
impl InventoryArtifact {
    /// Seal one inventory, computing its integrity digest over its own preimage.
    pub fn seal(body: AuthoringInventory) -> Result<Self, EncodingFailure> {
        <Self as AuthoringArtifact>::seal(body)
    }

    /// Return whether the stored digest is the digest of this content.
    pub fn verify_integrity(&self) -> Result<bool, EncodingFailure> {
        <Self as AuthoringArtifact>::verify_integrity(self)
    }

    /// Borrow the sealed inventory.
    #[must_use]
    pub const fn body(&self) -> &AuthoringInventory {
        &self.body
    }

    /// Return the reference another artifact uses to name this one.
    #[must_use]
    pub fn reference(&self) -> AuthoringArtifactReference {
        <Self as AuthoringArtifact>::reference(self)
    }

    /// Compare two artifacts that may claim the same reference.
    ///
    /// Equal references with unequal content are an identity conflict.
    #[must_use]
    pub fn relation(&self, other: &Self) -> ArtifactRelation {
        <Self as AuthoringArtifact>::relation(self, other)
    }
}
