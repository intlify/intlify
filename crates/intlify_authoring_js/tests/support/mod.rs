// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Inputs the integration tests name explicitly.
//!
//! Every value a Producer needs is written out here rather than defaulted:
//! the owner, the context and its profile pin, the grammar of each unit, and
//! every bound.

#![allow(dead_code, reason = "each test file uses its own subset")]

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    Completeness, IntegrityDigest, OwnerIdentity, OwnerKind, SourceSnapshot, SurfaceVocabulary,
};
use intlify_authoring_js::{
    admit_units, AdmittedUnit, Grammar, JsAuthoringLimits, JsAuthoringProfile, SourceUnit,
    UnitMember,
};
use intlify_shared_json::encoding::digest_bytes;

pub fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

/// A test context pinned to the profile this Producer implements.
pub fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout"]).expect("a vocabulary"),
    )
    .authoring_profile(JsAuthoringProfile::new().identity().clone())
    .default_source_locale("en")
    .build()
    .expect("checked test context")
}

pub fn limits() -> JsAuthoringLimits {
    JsAuthoringLimits {
        units: 16,
        unit_bytes: 64 * 1024,
        total_bytes: 256 * 1024,
        ast_nodes: 64 * 1024,
        input_segments: 1024,
    }
    .validate()
    .expect("satisfiable bounds")
}

/// A snapshot that names exactly `bytes`, at revision "1".
pub fn snapshot(unit: &str, grammar: Grammar, bytes: &[u8]) -> SourceSnapshot {
    SourceSnapshot::new(
        owner(),
        unit,
        "1",
        grammar.identity(),
        bytes.len() as u64,
        IntegrityDigest::from_hash(digest_bytes(bytes)).as_str(),
    )
    .expect("checked snapshot")
}

pub fn member(unit: &str) -> UnitMember {
    UnitMember::new(unit, "1").expect("checked member")
}

/// Admit a complete scope of the given units, each a member at revision "1".
pub fn admit<'b>(units: &[(&str, Grammar, &'b [u8])]) -> Vec<AdmittedUnit<'b>> {
    let membership: Vec<UnitMember> = units.iter().map(|(unit, _, _)| member(unit)).collect();
    let supplied: Vec<SourceUnit<'b>> = units
        .iter()
        .map(|(unit, grammar, bytes)| SourceUnit::new(snapshot(unit, *grammar, bytes), bytes))
        .collect();
    admit_units(
        &context(),
        &JsAuthoringProfile::new(),
        Completeness::Complete,
        &membership,
        &supplied,
        &limits(),
    )
    .expect("admitted units")
}

/// A probe that never asks to stop.
pub fn never() -> bool {
    false
}
