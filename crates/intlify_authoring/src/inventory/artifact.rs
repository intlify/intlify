// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's authoring artifact envelope, for the `authoring-inventory` kind.
//!
//! An artifact is a closed body sealed under a digest that covers everything
//! but the digest itself. The kind, schema revision and specification are each
//! a single value here, so the committed schema refuses exactly what the reader
//! refuses rather than admitting any of the five registered kinds with an
//! inventory body.
//!
//! The other four kinds 017 registers are Phase 3 work. They appear only in
//! [`ArtifactKind`], so a reader that meets one reports it as unsupported
//! rather than as malformed.

use std::cmp::Ordering;

use intlify_shared_json::encoding::{hash_with_excluded_member, Domain, EncodingFailure};
use intlify_shared_json::token::{IntegrityDigest, Token, VersionedIdentity};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::model::AuthoringInventory;

/// The digest domain 017 registers for authoring artifact integrity.
pub const ARTIFACT_INTEGRITY_DOMAIN: &str = "authoring-artifact-integrity";

/// The specification identity every authoring artifact is governed by.
pub const AUTHORING_SPECIFICATION_IDENTITY: &str = "intlify-design-016";

/// The one specification revision this reader implements.
pub const AUTHORING_SPECIFICATION_REVISION: &str = "0";

/// The one artifact schema revision this reader implements.
pub const ARTIFACT_SCHEMA_REVISION: &str = "0";

/// The five authoring artifact kinds design 017 registers.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub enum ArtifactKind {
    AuthoringInventory,
    MessageIntent,
    MessageReference,
    IntentRegistry,
    IntentRegistryUpdate,
}

impl ArtifactKind {
    /// Return the exact wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthoringInventory => "authoring-inventory",
            Self::MessageIntent => "message-intent",
            Self::MessageReference => "message-reference",
            Self::IntentRegistry => "intent-registry",
            Self::IntentRegistryUpdate => "intent-registry-update",
        }
    }

    /// Find a registered kind by its wire spelling.
    #[must_use]
    pub fn from_wire(spelling: &str) -> Option<Self> {
        [
            Self::AuthoringInventory,
            Self::MessageIntent,
            Self::MessageReference,
            Self::IntentRegistry,
            Self::IntentRegistryUpdate,
        ]
        .into_iter()
        .find(|kind| kind.as_str() == spelling)
    }
}

// Single-value types. Each serializes as one fixed spelling and refuses every
// other, and schemars renders each as a one-member enum, which is what makes
// the schema as strict as the reader.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum InventoryKind {
    #[serde(rename = "authoring-inventory")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum RevisionZero {
    #[serde(rename = "0")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
enum Design016 {
    #[serde(rename = "intlify-design-016")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AuthoringSpecification {
    identity: Design016,
    revision: RevisionZero,
}

impl AuthoringSpecification {
    const CURRENT: Self = Self {
        identity: Design016::Value,
        revision: RevisionZero::Value,
    };
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

impl InventoryArtifact {
    /// Seal one inventory, computing its integrity digest over its own preimage.
    pub fn seal(body: AuthoringInventory) -> Result<Self, EncodingFailure> {
        let mut artifact = Self {
            kind: InventoryKind::Value,
            schema_revision: RevisionZero::Value,
            authoring_specification: AuthoringSpecification::CURRENT,
            body,
            // Overwritten below. The member has to be present while hashing,
            // because it is the one member the preimage excludes.
            integrity_digest: IntegrityDigest::from_hash([0; 32]),
        };
        artifact.integrity_digest = artifact.computed_digest()?;
        Ok(artifact)
    }

    fn computed_digest(&self) -> Result<IntegrityDigest, EncodingFailure> {
        hash_with_excluded_member(
            Domain::literal(ARTIFACT_INTEGRITY_DOMAIN),
            self,
            &["integrityDigest"],
        )
        .map(IntegrityDigest::from_hash)
    }

    /// Return whether the stored digest is the digest of this content.
    pub fn verify_integrity(&self) -> Result<bool, EncodingFailure> {
        Ok(self.computed_digest()? == self.integrity_digest)
    }

    /// Borrow the sealed inventory.
    #[must_use]
    pub const fn body(&self) -> &AuthoringInventory {
        &self.body
    }

    /// Return the reference another artifact uses to name this one.
    #[must_use]
    pub fn reference(&self) -> AuthoringArtifactReference {
        AuthoringArtifactReference {
            kind: ArtifactKind::AuthoringInventory,
            schema_revision: Token::literal(ARTIFACT_SCHEMA_REVISION),
            authoring_specification: VersionedIdentity::literal(
                AUTHORING_SPECIFICATION_IDENTITY,
                AUTHORING_SPECIFICATION_REVISION,
            ),
            integrity_digest: self.integrity_digest.clone(),
        }
    }

    /// Compare two artifacts that may claim the same reference.
    ///
    /// Equal references with unequal content are an identity conflict. A
    /// digest names content; it does not merge two different contents that
    /// happen to present the same digest, so the content is compared rather
    /// than trusted.
    #[must_use]
    pub fn relation(&self, other: &Self) -> ArtifactRelation {
        match (
            self.reference() == other.reference(),
            self.body == other.body,
        ) {
            (true, true) => ArtifactRelation::Same,
            (true, false) => ArtifactRelation::Conflict,
            (false, _) => ArtifactRelation::Distinct,
        }
    }
}

/// How two artifacts relate when compared by reference and content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactRelation {
    /// The same reference names the same content.
    Same,
    /// Different references; the artifacts are different artifacts.
    Distinct,
    /// The same reference names different content.
    Conflict,
}

/// The fields another artifact uses to name one authoring artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoringArtifactReference {
    kind: ArtifactKind,
    schema_revision: Token,
    authoring_specification: VersionedIdentity,
    integrity_digest: IntegrityDigest,
}

impl AuthoringArtifactReference {
    /// Return the referenced kind.
    #[must_use]
    pub const fn kind(&self) -> ArtifactKind {
        self.kind
    }

    /// Borrow the referenced integrity digest.
    #[must_use]
    pub const fn integrity_digest(&self) -> &IntegrityDigest {
        &self.integrity_digest
    }

    /// Compare two references in 017's canonical order.
    ///
    /// The order is kind, schema revision, specification identity and
    /// revision, then digest, each by unsigned UTF-8 bytes. The kind compares
    /// by its spelling rather than by its position in the enum, so adding a
    /// kind can never reorder existing references.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.kind
            .as_str()
            .as_bytes()
            .cmp(other.kind.as_str().as_bytes())
            .then_with(|| {
                self.schema_revision
                    .as_str()
                    .as_bytes()
                    .cmp(other.schema_revision.as_str().as_bytes())
            })
            .then_with(|| {
                self.authoring_specification
                    .identity()
                    .as_str()
                    .as_bytes()
                    .cmp(other.authoring_specification.identity().as_str().as_bytes())
            })
            .then_with(|| {
                self.authoring_specification
                    .revision()
                    .as_str()
                    .as_bytes()
                    .cmp(other.authoring_specification.revision().as_str().as_bytes())
            })
            .then_with(|| {
                self.integrity_digest
                    .as_str()
                    .as_bytes()
                    .cmp(other.integrity_digest.as_str().as_bytes())
            })
    }
}
