// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Intent revision digest.
//!
//! A revision identifies one versioned localization-relevant projection of a
//! message lineage. Equal revision labels in a lineage must identify equal
//! projections, so the digest is computed over the canonical encoding of the
//! complete projection together with the specification that fixes what the
//! projection means.
//!
//! The digest is deliberately not a fast in-process hash. It travels between
//! implementations, and a reader recomputes it from retained source rather than
//! trusting a caller's precomputed value.

use intlify_shared_json::encoding::{self, Domain, EncodingFailure};
use intlify_shared_json::token::{IntegrityDigest, VersionedIdentity};
use serde::Serialize;

use super::model::IntentProjection;

/// Identity of the projection specification this revision is computed under.
pub const PROJECTION_IDENTITY: &str = "intlify-intent-projection";

/// Revision of that projection specification.
pub const PROJECTION_REVISION: &str = "0";

/// The registered digest domain for an Intent revision.
const REVISION_DOMAIN: &str = "intent-semantic-revision";

/// Return the exact projection specification pin.
#[must_use]
pub fn projection_specification() -> VersionedIdentity {
    VersionedIdentity::literal(PROJECTION_IDENTITY, PROJECTION_REVISION)
}

/// Complete failure of a revision computation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RevisionFailure {
    /// The projection could not be canonically encoded.
    Encoding(EncodingFailure),
}

// The digest preimage. Naming the specification inside the hashed value is what
// makes a later projection revision a different digest rather than a silent
// reinterpretation of the same one.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Preimage<'a> {
    projection_specification: VersionedIdentity,
    projection: &'a IntentProjection,
}

/// Compute the Intent revision of one projection.
pub fn intent_revision(projection: &IntentProjection) -> Result<IntegrityDigest, RevisionFailure> {
    let preimage = Preimage {
        projection_specification: projection_specification(),
        projection,
    };
    let value = serde_json::to_value(&preimage)
        .map_err(|_| RevisionFailure::Encoding(EncodingFailure::Serialization))?;
    let digest = encoding::hash(Domain::literal(REVISION_DOMAIN), &value)
        .map_err(RevisionFailure::Encoding)?;
    Ok(IntegrityDigest::from_hash(digest))
}

/// Return the canonical preimage of a revision, for fixtures and review.
///
/// This is the exact value the digest is taken over. Exposing it lets an
/// independent implementation check the framing separately from the hash, so a
/// mismatch says which of the two is wrong.
pub fn revision_preimage(
    projection: &IntentProjection,
) -> Result<serde_json::Value, RevisionFailure> {
    serde_json::to_value(Preimage {
        projection_specification: projection_specification(),
        projection,
    })
    .map_err(|_| RevisionFailure::Encoding(EncodingFailure::Serialization))
}
