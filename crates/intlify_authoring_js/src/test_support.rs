// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Inputs the unit tests share.
//!
//! These mirror `tests/support`, which unit tests cannot reach. Every value a
//! module under test needs is written out rather than defaulted: the owner,
//! the context, the registered intrinsics, the grammar, and every bound.

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    Completeness, IntegrityDigest, OwnerIdentity, OwnerKind, SourceSnapshot, SurfaceVocabulary,
    Token,
};
use intlify_shared_json::encoding::digest_bytes;
use oxc_allocator::Allocator;

use crate::binding::{Bindings, Intrinsic, IntrinsicBinding};
use crate::grammar::Grammar;
use crate::limits::tests::generous;
use crate::parse::{self, Parsed, Reading};
use crate::profile::JsAuthoringProfile;
use crate::report::Reporter;
use crate::unit::{admit_units, AdmittedUnit, SourceUnit, UnitMember};

/// The module the fixtures register their intrinsics under.
pub(crate) const MODULE: &str = "fixture-authoring";

/// The imports most fixtures begin with.
pub(crate) const PRELUDE: &str = "import { intent, mf2, noIntent } from 'fixture-authoring'\n";

/// The unit every single-unit fixture is.
pub(crate) const UNIT: &str = "checkout";

pub(crate) fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

pub(crate) fn token(value: &str) -> Token {
    Token::new(value).expect("a token")
}

/// A test context pinned to the profile this Producer implements.
pub(crate) fn context() -> TestContext {
    TestContext::builder(
        owner(),
        SurfaceVocabulary::new(["checkout", "nav"]).expect("a vocabulary"),
    )
    .authoring_profile(JsAuthoringProfile::new().identity().clone())
    .usage_profile(JsAuthoringProfile::usage_profile())
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .expect("checked test context")
}

/// The three fixture intrinsics.
pub(crate) fn intrinsics() -> [IntrinsicBinding; 3] {
    [
        IntrinsicBinding::new(MODULE, "intent", Intrinsic::Intent),
        IntrinsicBinding::new(MODULE, "mf2", Intrinsic::Mf2),
        IntrinsicBinding::new(MODULE, "noIntent", Intrinsic::NoIntent),
    ]
}

pub(crate) fn bindings() -> Bindings {
    Bindings::new(intrinsics()).expect("a consistent binding set")
}

pub(crate) fn profile() -> JsAuthoringProfile {
    JsAuthoringProfile::new()
        .with_bindings(intrinsics())
        .expect("a consistent binding set")
}

/// A snapshot that names exactly `bytes`, at revision "1".
pub(crate) fn snapshot(unit: &str, grammar: Grammar, bytes: &[u8]) -> SourceSnapshot {
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

/// A reporter for the one unit holding `text`, with room for any fixture.
pub(crate) fn reporter(text: &str) -> Reporter {
    Reporter::new(
        snapshot(UNIT, Grammar::JsModule, text.as_bytes()),
        generous().authoring.diagnostics,
    )
}

/// Parse `text`, which the fixture expects the host to accept.
pub(crate) fn parse<'a>(arena: &'a Allocator, text: &'a str, grammar: Grammar) -> Box<Parsed<'a>> {
    match parse::read(arena, text, grammar, &generous(), &token(UNIT), &|| false) {
        Ok(Reading::Accepted(parsed)) => parsed,
        other => panic!("{text:?} is accepted under {grammar:?}, not {other:?}"),
    }
}

/// Admit a complete scope of the given units, each a member at revision "1".
pub(crate) fn admit<'b>(units: &[(&str, Grammar, &'b [u8])]) -> Vec<AdmittedUnit<'b>> {
    let membership: Vec<UnitMember> = units
        .iter()
        .map(|(unit, _, _)| UnitMember::new(unit, "1").expect("checked member"))
        .collect();
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
        &generous(),
    )
    .expect("admitted units")
}

/// The half-open byte range of the only occurrence of `needle` in `text`.
///
/// Ranges are found in the source text itself rather than read back from the
/// code under test, so an expectation never copies what it is checking.
pub(crate) fn at(text: &str, needle: &str) -> (u64, u64) {
    let mut found = text.match_indices(needle);
    let (start, _) = found
        .next()
        .unwrap_or_else(|| panic!("{needle:?} is in the fixture"));
    assert!(
        found.next().is_none(),
        "{needle:?} occurs once in the fixture"
    );
    (start as u64, (start + needle.len()) as u64)
}

/// The range of the `nth` occurrence of `needle`, counting from zero.
pub(crate) fn nth(text: &str, needle: &str, nth: usize) -> (u64, u64) {
    let (start, _) = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle:?} occurs {} times", nth + 1));
    (start as u64, (start + needle.len()) as u64)
}
