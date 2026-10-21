// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The vocabulary vectors an independent implementation checks.
//!
//! They hold sealed vocabularies with their integrity and semantic preimages
//! and semantic digests, edits that must keep or change the semantic digest,
//! and sealed artifacts that must each fail one of 017's reader steps 3 to 6.
//! The Node checker re-frames and re-hashes every preimage and decides each
//! failure itself, so a disagreement says whether this crate or the checker is
//! wrong. The vocabularies are test values, not one any project admits.

use serde_json::{json, Value};

use crate::artifacts::vocabulary::{
    read, IntentSurfaceClassVocabularyArtifact, VocabularyLimits, INTEGRITY_DOMAIN, SEMANTIC_DOMAIN,
};
use crate::resolver::vocabulary::body_reason;

/// The fixture vocabulary's identity.
pub(crate) const IDENTITY: &str = "intlify-test-vocabulary";

/// The fixture vocabulary's revision.
pub(crate) const REVISION: &str = "0";

/// The fixture vocabulary's members, the surface classes coverage tests use.
pub(crate) const MEMBERS: [&str; 3] = ["checkout", "internal-tool", "nav"];

/// The fixture vocabulary's semantic digest, as the Node checker computes it.
pub(crate) const SEMANTIC_DIGEST: &str =
    "sha256:ca5f503b95aad972565514d3b68b3e8be4a811bb2fc983be16163b88c1fffbb2";

/// Limits no vector comes near.
const ROOM: VocabularyLimits = VocabularyLimits {
    artifact_bytes: u64::MAX,
    depth: u64::MAX,
    members: u64::MAX,
    member_bytes: u64::MAX,
    total_member_bytes: u64::MAX,
};

fn sealed(
    identity: &'static str,
    revision: &'static str,
    members: &[&str],
) -> Result<Value, String> {
    let artifact = IntentSurfaceClassVocabularyArtifact::sealed(identity, revision, members)
        .map_err(|failure| format!("{failure:?}"))?;
    serde_json::to_value(artifact).map_err(|error| error.to_string())
}

/// One intact vocabulary with its preimages and semantic digest, read back
/// through the boundary and the reader like any other.
fn vocabulary(id: &str, artifact: &Value) -> Result<Value, String> {
    let bytes = serde_json::to_vec(artifact).map_err(|error| error.to_string())?;
    let formal = read(Some(&bytes), &ROOM).map_err(|failure| format!("{id}: {failure:?}"))?;
    let intact = formal
        .admit()
        .map_err(|failure| format!("{id}: {failure:?}"))?;
    let mut integrity = artifact.clone();
    integrity
        .as_object_mut()
        .and_then(|object| object.remove("integrityDigest"))
        .ok_or(format!("{id}: no integrity digest"))?;
    let semantic: IntentSurfaceClassVocabularyArtifact =
        serde_json::from_value(artifact.clone()).map_err(|error| error.to_string())?;
    Ok(json!({
        "id": id,
        "artifact": artifact,
        "integrityPreimage": integrity,
        "semanticPreimage": semantic.semantic_preimage(),
        "semanticDigest": intact.semantic_digest().as_str(),
    }))
}

/// One artifact the reader must refuse, with 015's reason for the first step
/// it fails.
fn refused(id: &str, artifact: &Value) -> Result<Value, String> {
    let bytes = serde_json::to_vec(artifact).map_err(|error| error.to_string())?;
    let formal = read(Some(&bytes), &ROOM).map_err(|failure| format!("{id}: {failure:?}"))?;
    let failure = formal
        .admit()
        .err()
        .ok_or(format!("{id}: the artifact was not refused"))?;
    Ok(json!({
        "id": id,
        "artifact": artifact,
        "reason": body_reason(failure).as_str(),
    }))
}

/// The complete vectors document.
pub fn document() -> Result<Value, String> {
    let fixture = sealed(IDENTITY, REVISION, &MEMBERS)?;
    let vocabularies = vec![
        vocabulary("fixture", &fixture)?,
        vocabulary("empty", &sealed(IDENTITY, REVISION, &[])?)?,
        vocabulary(
            "renamed",
            &sealed("intlify-renamed-vocabulary", REVISION, &MEMBERS)?,
        )?,
        vocabulary("revised", &sealed(IDENTITY, "1", &MEMBERS)?)?,
        vocabulary(
            "member-changed",
            &sealed(
                IDENTITY,
                REVISION,
                &["checkout", "internal-tool", "navigation"],
            )?,
        )?,
    ];
    let mut stale = fixture;
    stale["members"] = json!(["checkout", "internal-tool", "navigation"]);
    let refusals = vec![
        refused("stale-integrity", &stale)?,
        refused(
            "member-not-a-token",
            &sealed(IDENTITY, REVISION, &["checkout", "internal tool", "nav"])?,
        )?,
        refused(
            "repeat-not-adjacent",
            &sealed(IDENTITY, REVISION, &["checkout", "nav", "checkout"])?,
        )?,
        refused(
            "unsorted",
            &sealed(IDENTITY, REVISION, &["nav", "checkout", "internal-tool"])?,
        )?,
    ];
    Ok(json!({
        "note": "Intent Surface-Class Vocabulary Artifacts sealed by intlify_config. An \
                 independent implementation recomputes each integrity digest over its \
                 artifact without the top-level integrityDigest and each semantic digest over \
                 kind, schemaRevision, and members; checks that renaming or revising a \
                 vocabulary keeps its semantic digest while changing one member or emptying it \
                 changes the digest; and decides for every refused artifact which of 017's \
                 reader steps 3 to 6 fails first. None of these vocabularies is one a project \
                 admits.",
        "integrityDomain": INTEGRITY_DOMAIN,
        "semanticDomain": SEMANTIC_DOMAIN,
        "vocabularies": vocabularies,
        "sameSemanticDigest": [["fixture", "renamed"], ["fixture", "revised"]],
        "differentSemanticDigest": [["fixture", "member-changed"], ["fixture", "empty"]],
        "refused": refusals,
    }))
}

/// The fixture vocabulary past the boundary, for the checks over it.
#[cfg(test)]
pub(crate) fn formal() -> crate::artifacts::vocabulary::FormalVocabulary {
    let artifact = sealed(IDENTITY, REVISION, &MEMBERS).unwrap();
    read(Some(&serde_json::to_vec(&artifact).unwrap()), &ROOM).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    const COMMITTED: &str = include_str!("../../../fixtures/vocabulary/vectors.json");

    fn committed() -> Value {
        serde_json::from_str(COMMITTED).unwrap()
    }

    fn cases<'a>(vectors: &'a Value, list: &str) -> &'a [Value] {
        vectors[list].as_array().unwrap()
    }

    fn reread(artifact: &Value) -> crate::artifacts::vocabulary::FormalVocabulary {
        read(Some(&serde_json::to_vec(artifact).unwrap()), &ROOM).unwrap()
    }

    #[test]
    fn the_committed_vectors_are_the_ones_this_crate_writes() {
        // Compared as values: the repository formatter owns the file's layout.
        assert_eq!(committed(), document().unwrap());
    }

    #[test]
    fn the_digests_are_the_vectors_an_independent_implementation_checked() {
        let vectors = committed();
        for case in cases(&vectors, "vocabularies") {
            let intact = reread(&case["artifact"]).admit().unwrap();
            assert_eq!(
                intact.semantic_digest().as_str(),
                case["semanticDigest"],
                "{}",
                case["id"]
            );
        }
        let fixture = &cases(&vectors, "vocabularies")[0];
        assert_eq!(fixture["id"], "fixture");
        assert_eq!(fixture["semanticDigest"], SEMANTIC_DIGEST);
    }

    #[test]
    fn every_refused_vector_fails_where_the_independent_implementation_says() {
        let vectors = committed();
        for case in cases(&vectors, "refused") {
            let failure = reread(&case["artifact"]).admit().unwrap_err();
            assert_eq!(
                body_reason(failure).as_str(),
                case["reason"],
                "{}",
                case["id"]
            );
        }
    }

    #[test]
    fn the_fixture_is_the_vocabulary_coverage_tests_use() {
        let intact = formal().admit().unwrap();
        assert_eq!(intact.identity().as_str(), IDENTITY);
        assert_eq!(intact.revision().as_str(), REVISION);
        assert_eq!(
            intact
                .members()
                .iter()
                .map(intlify_shared_json::token::Token::as_str)
                .collect::<Vec<_>>(),
            MEMBERS
        );
        assert_eq!(intact.semantic_digest().as_str(), SEMANTIC_DIGEST);
    }
}
