// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Inputs the integration tests name explicitly.
//!
//! Every value a Producer needs is written out here rather than defaulted:
//! the owner, the context and its profile pin, the registered intrinsics, the
//! grammar of each unit, and every bound.

#![allow(dead_code, reason = "each test file uses its own subset")]

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    AuthoringLimits, Completeness, Detail, Diagnostic, IntegrityDigest, Location, Occurrence,
    OccurrenceRole, OwnerIdentity, OwnerKind, ReasonFamily, SourceSnapshot, SurfaceVocabulary,
};
use intlify_authoring_js::{
    admit_units, analyze_unit, AdmittedUnit, DomGlobal, Grammar, Intrinsic, IntrinsicBinding,
    JsAnalysisWorkspace, JsAuthoringLimits, JsAuthoringProfile, ProducerFailure, SourceUnit,
    UnitAnalysis, UnitMember,
};
use intlify_shared_json::encoding::digest_bytes;

/// The module the fixtures register their intrinsics under.
///
/// It names no published package; it is only the specifier the test profile
/// registers, and a unit has to import it for anything to be recognized.
pub const MODULE: &str = "fixture-authoring";

/// The imports most fixtures begin with.
pub const PRELUDE: &str = "import { intent, mf2, noIntent } from 'fixture-authoring'\n";

pub fn owner() -> OwnerIdentity {
    OwnerIdentity::new(OwnerKind::Application, "storefront").expect("checked project id")
}

/// A test context pinned to the profile this Producer implements.
///
/// It supplies the default source locale and surface class a declaration
/// without metadata resolves against, and registers the usage profile
/// automatically recognized text takes its usage from.
pub fn context() -> TestContext {
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

/// The profile registering the three fixture intrinsics.
pub fn profile() -> JsAuthoringProfile {
    JsAuthoringProfile::new()
        .with_bindings([
            IntrinsicBinding::new(MODULE, "intent", Intrinsic::Intent),
            IntrinsicBinding::new(MODULE, "mf2", Intrinsic::Mf2),
            IntrinsicBinding::new(MODULE, "noIntent", Intrinsic::NoIntent),
        ])
        .expect("a consistent binding set")
}

/// The fixture profile, also admitting the standard `document`.
pub fn dom_profile() -> JsAuthoringProfile {
    profile().with_dom_globals([DomGlobal::Document])
}

/// Analyze one module-goal unit holding `text` under the DOM profile.
pub fn analyze_dom(text: &str) -> UnitAnalysis {
    analyze_dom_as(Grammar::JsModule, text)
}

/// Analyze one unit holding `text` under the DOM profile.
pub fn analyze_dom_as(grammar: Grammar, text: &str) -> UnitAnalysis {
    try_analyze(grammar, text, &dom_profile(), &limits()).expect("the analysis runs")
}

pub fn limits() -> JsAuthoringLimits {
    JsAuthoringLimits {
        units: 16,
        unit_bytes: 64 * 1024,
        total_bytes: 256 * 1024,
        ast_nodes: 64 * 1024,
        input_segments: 1024,
        references: 256,
        exclusions: 256,
        parameter_bindings: 64,
        alias_chain: 16,
        tracked_origins: 64,
        proof_steps: 1 << 20,
        authoring: AuthoringLimits {
            declarations: 256,
            message_text_bytes: 64 * 1024,
            emitted_mf2_bytes: 128 * 1024,
            extraction_segments: 4096,
            parameter_names: 64,
            parameter_name_bytes: 256,
            metadata_value_bytes: 4096,
            vocabulary_members: 256,
            projection_nodes: 4096,
            projection_depth: 32,
            diagnostics: 256,
        },
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

/// Read one unit under a profile that registers no intrinsic.
pub fn read_unit<C>(
    unit: &AdmittedUnit<'_>,
    limits: &JsAuthoringLimits,
    workspace: &mut JsAnalysisWorkspace,
    cancelled: &C,
) -> Result<UnitAnalysis, ProducerFailure>
where
    C: Fn() -> bool + ?Sized,
{
    analyze_unit(
        &context(),
        &JsAuthoringProfile::new(),
        unit,
        limits,
        workspace,
        cancelled,
    )
}

/// Analyze one module-goal unit holding `text` under the fixture profile.
pub fn analyze(text: &str) -> UnitAnalysis {
    analyze_as(Grammar::JsModule, text)
}

/// Analyze one unit holding `text` under the fixture profile.
pub fn analyze_as(grammar: Grammar, text: &str) -> UnitAnalysis {
    try_analyze(grammar, text, &profile(), &limits()).expect("the analysis runs")
}

/// Analyze one unit, returning an operational failure instead of panicking.
pub fn try_analyze(
    grammar: Grammar,
    text: &str,
    profile: &JsAuthoringProfile,
    limits: &JsAuthoringLimits,
) -> Result<UnitAnalysis, ProducerFailure> {
    let units = admit(&[("checkout", grammar, text.as_bytes())]);
    analyze_unit(
        &context(),
        profile,
        &units[0],
        limits,
        &mut JsAnalysisWorkspace::new(),
        &never,
    )
}

/// The half-open byte range of the only occurrence of `needle` in `text`.
///
/// Ranges are found in the source text itself rather than read back from
/// the analysis, so an expectation never copies what it is checking.
pub fn at(text: &str, needle: &str) -> (u64, u64) {
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
pub fn nth(text: &str, needle: &str, nth: usize) -> (u64, u64) {
    let (start, _) = text
        .match_indices(needle)
        .nth(nth)
        .unwrap_or_else(|| panic!("{needle:?} occurs {} times", nth + 1));
    (start as u64, (start + needle.len()) as u64)
}

/// The range an occurrence addresses, with its role.
pub fn occurrence(occurrence: &Occurrence) -> (OccurrenceRole, (u64, u64)) {
    let range = occurrence.range();
    (occurrence.role(), (range.start(), range.end()))
}

/// One diagnostic reduced to what a fixture asserts about it: the reason's
/// code, the cause within it, and the range it points at.
pub type Reported = (String, Option<Detail>, Option<(u64, u64)>);

/// Reduce one diagnostic to what a fixture asserts about it.
pub fn reported(diagnostic: &Diagnostic) -> Reported {
    let range = match diagnostic.location() {
        Location::Region(region) => Some((region.range().start(), region.range().end())),
        Location::Occurrence(occurrence) => {
            Some((occurrence.range().start(), occurrence.range().end()))
        }
        Location::Unit(_) => None,
    };
    (
        diagnostic.origin().code().to_owned(),
        diagnostic.detail(),
        range,
    )
}

/// Every diagnostic of an analysis, reduced.
pub fn diagnostics(analysis: &UnitAnalysis) -> Vec<Reported> {
    analysis.diagnostics().iter().map(reported).collect()
}

/// A reduced diagnostic, spelled the way a fixture writes one.
pub fn expect(family: ReasonFamily, detail: Detail, range: (u64, u64)) -> Reported {
    (family.as_str().to_owned(), Some(detail), Some(range))
}
