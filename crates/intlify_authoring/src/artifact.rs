// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 017's authoring artifact envelope, shared by its five kinds.
//!
//! An artifact is a closed body sealed under a digest that covers everything
//! but the digest itself. 017 writes it once, as `AuthoringArtifact<K, B>`, and
//! each registered kind fills it with its own body. Here each kind is its own
//! named type, so its committed schema names it and admits that kind alone,
//! while [`AuthoringArtifact`] supplies sealing, integrity, references and
//! comparison once for all of them.
//!
//! Reading follows 017's order: bounded strict decoding, then the exact kind,
//! schema and specification tuple, then the closed body, then the integrity
//! digest. [`read_sealed`] performs those steps for any kind, so a reader of a
//! later kind cannot skip one of them or take them in another order.

use std::cmp::Ordering;

use intlify_shared_json::decode::{self, DecodeFailure};
use intlify_shared_json::encoding::{hash_with_excluded_member, Domain, EncodingFailure};
use intlify_shared_json::token::{IntegrityDigest, Token, VersionedIdentity};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

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
// a kind's schema as strict as its reader. They carry no doc comments on
// purpose: a doc comment becomes a description in every committed schema
// that names the type.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum RevisionZero {
    #[serde(rename = "0")]
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Design016 {
    #[serde(rename = "intlify-design-016")]
    Value,
}

// The specification every authoring artifact this reader implements names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AuthoringSpecification {
    identity: Design016,
    revision: RevisionZero,
}

impl AuthoringSpecification {
    /// The one specification this reader implements.
    pub const CURRENT: Self = Self {
        identity: Design016::Value,
        revision: RevisionZero::Value,
    };
}

/// What every sealed authoring artifact shares, whatever its kind.
///
/// A kind's type holds the envelope fields, with its kind, schema revision and
/// specification each a single-value type, and implements the four required
/// methods. Sealing, integrity, references and comparison come from here, so
/// every kind computes its digest over the same preimage and names itself the
/// same way.
pub trait AuthoringArtifact: Serialize + Sized {
    /// The closed body this kind carries.
    type Body: PartialEq;

    /// The registered kind this type is.
    const KIND: ArtifactKind;

    /// Put a body and a digest into this kind's envelope.
    ///
    /// The result is not sealed unless its digest is the digest of its
    /// content, which is why [`Self::seal`] is how a body becomes an artifact.
    fn envelope(body: Self::Body, integrity_digest: IntegrityDigest) -> Self;

    /// Take the body back out of the envelope.
    fn into_body(self) -> Self::Body;

    /// Borrow the body.
    fn body(&self) -> &Self::Body;

    /// Borrow the stored integrity digest.
    fn integrity_digest(&self) -> &IntegrityDigest;

    /// Seal one body, computing its integrity digest over its own preimage.
    fn seal(body: Self::Body) -> Result<Self, EncodingFailure> {
        // The member has to be present while hashing, because it is the one
        // member the preimage excludes.
        let unsealed = Self::envelope(body, IntegrityDigest::from_hash([0; 32]));
        let digest = integrity_of(&unsealed)?;
        Ok(Self::envelope(unsealed.into_body(), digest))
    }

    /// Return whether the stored digest is the digest of this content.
    fn verify_integrity(&self) -> Result<bool, EncodingFailure> {
        Ok(integrity_of(self)? == *self.integrity_digest())
    }

    /// Return the reference another artifact uses to name this one.
    fn reference(&self) -> AuthoringArtifactReference {
        AuthoringArtifactReference {
            kind: Self::KIND,
            schema_revision: Token::literal(ARTIFACT_SCHEMA_REVISION),
            authoring_specification: VersionedIdentity::literal(
                AUTHORING_SPECIFICATION_IDENTITY,
                AUTHORING_SPECIFICATION_REVISION,
            ),
            integrity_digest: self.integrity_digest().clone(),
        }
    }

    /// Compare two artifacts that may claim the same reference.
    ///
    /// Equal references with unequal content are an identity conflict. A
    /// digest names content; it does not merge two different contents that
    /// happen to present the same digest, so the content is compared rather
    /// than trusted.
    fn relation(&self, other: &Self) -> ArtifactRelation {
        match (
            self.reference() == other.reference(),
            self.body() == other.body(),
        ) {
            (true, true) => ArtifactRelation::Same,
            (true, false) => ArtifactRelation::Conflict,
            (false, _) => ArtifactRelation::Distinct,
        }
    }
}

/// The digest of one artifact: everything but its top-level digest member.
fn integrity_of<A: Serialize>(artifact: &A) -> Result<IntegrityDigest, EncodingFailure> {
    hash_with_excluded_member(
        Domain::literal(ARTIFACT_INTEGRITY_DOMAIN),
        artifact,
        &["integrityDigest"],
    )
    .map(IntegrityDigest::from_hash)
}

/// Why bytes are not one sealed artifact of the requested kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadFailure {
    /// The bytes are not one bounded value in the shared JSON encoding.
    Unreadable(DecodeFailure),
    /// The kind, schema revision or specification is not the requested one.
    /// A registered kind read as another is reported here too, rather than as
    /// a malformed body.
    Unsupported,
    /// The value is not the closed shape of the requested kind, or its
    /// integrity preimage cannot be built or encoded under the shared
    /// encoding's limits. Either way the digest cannot be checked, which is
    /// different from a digest that was checked and does not match.
    Shape,
    /// The stored digest is not the digest of the stored content.
    Integrity,
}

/// Read one sealed artifact of kind `A`.
///
/// The tuple is selected before the body is decoded: a different kind or
/// revision has a different body shape, so decoding it as `A` would report a
/// shape failure for what is really a version this reader does not implement.
/// Integrity is checked before anything reads the body. Nothing here checks
/// what the body means; each kind's admission does that next.
pub fn read_sealed<A>(bytes: &[u8]) -> Result<A, ReadFailure>
where
    A: AuthoringArtifact + DeserializeOwned,
{
    let value = decode::value(bytes, true).map_err(ReadFailure::Unreadable)?;
    select(&value, A::KIND)?;
    let artifact: A = decode::typed(value).map_err(|_| ReadFailure::Shape)?;
    if !artifact
        .verify_integrity()
        .map_err(|_| ReadFailure::Shape)?
    {
        return Err(ReadFailure::Integrity);
    }
    Ok(artifact)
}

/// Check that a decoded value claims exactly the requested tuple.
fn select(value: &Value, kind: ArtifactKind) -> Result<(), ReadFailure> {
    let object = value.as_object().ok_or(ReadFailure::Shape)?;
    let text = |member: &str| {
        object
            .get(member)
            .and_then(Value::as_str)
            .ok_or(ReadFailure::Shape)
    };
    if ArtifactKind::from_wire(text("kind")?) != Some(kind) {
        return Err(ReadFailure::Unsupported);
    }
    if text("schemaRevision")? != ARTIFACT_SCHEMA_REVISION {
        return Err(ReadFailure::Unsupported);
    }
    let specification = object
        .get("authoringSpecification")
        .and_then(Value::as_object)
        .ok_or(ReadFailure::Shape)?;
    let part = |member: &str| {
        specification
            .get(member)
            .and_then(Value::as_str)
            .ok_or(ReadFailure::Shape)
    };
    if part("identity")? != AUTHORING_SPECIFICATION_IDENTITY
        || part("revision")? != AUTHORING_SPECIFICATION_REVISION
    {
        return Err(ReadFailure::Unsupported);
    }
    Ok(())
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

    /// Return whether this names the requested kind under the one schema
    /// revision and specification this reader implements.
    ///
    /// A reference is data until it resolves. This answers only whether it
    /// could name an artifact this reader can read, not whether one exists.
    #[must_use]
    pub fn is_current(&self, kind: ArtifactKind) -> bool {
        self.kind == kind
            && self.schema_revision.as_str() == ARTIFACT_SCHEMA_REVISION
            && self.authoring_specification.identity().as_str() == AUTHORING_SPECIFICATION_IDENTITY
            && self.authoring_specification.revision().as_str() == AUTHORING_SPECIFICATION_REVISION
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

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn reference(kind: &str, schema: &str, identity: &str, revision: &str) -> Value {
        json!({
            "kind": kind,
            "schemaRevision": schema,
            "authoringSpecification": { "identity": identity, "revision": revision },
            "integrityDigest": format!("sha256:{}", "0".repeat(64)),
        })
    }

    #[test]
    fn a_reference_is_current_only_for_its_own_kind_under_this_reader() {
        let read = |value: Value| -> AuthoringArtifactReference {
            serde_json::from_value(value).expect("a well-formed reference")
        };
        let registry = read(reference("intent-registry", "0", "intlify-design-016", "0"));
        assert!(registry.is_current(ArtifactKind::IntentRegistry));
        // A reference names one kind; it is not a reference to any other.
        assert!(!registry.is_current(ArtifactKind::IntentRegistryUpdate));
        // A later schema or specification is a reference this reader cannot
        // resolve, even though it decodes.
        for (schema, identity, revision) in [
            ("1", "intlify-design-016", "0"),
            ("0", "intlify-design-016", "1"),
            ("0", "intlify-design-017", "0"),
        ] {
            let later = read(reference("intent-registry", schema, identity, revision));
            assert!(!later.is_current(ArtifactKind::IntentRegistry));
        }
    }

    #[test]
    fn a_kind_is_found_only_by_its_exact_spelling() {
        for kind in [
            ArtifactKind::AuthoringInventory,
            ArtifactKind::MessageIntent,
            ArtifactKind::MessageReference,
            ArtifactKind::IntentRegistry,
            ArtifactKind::IntentRegistryUpdate,
        ] {
            assert_eq!(ArtifactKind::from_wire(kind.as_str()), Some(kind));
            assert_eq!(
                serde_json::to_value(kind).unwrap(),
                json!(kind.as_str()),
                "the wire spelling and the serializer agree"
            );
        }
        assert_eq!(ArtifactKind::from_wire("Intent-Registry"), None);
        assert_eq!(ArtifactKind::from_wire("intent-registry "), None);
    }
}
