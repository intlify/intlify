// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Admission: everything checked before a unit is parsed.
//!
//! Every refusal here is operational. None of these is a mistake an author
//! could fix by editing source, so none becomes a diagnostic.

mod support;

use intlify_authoring::test_context::TestContext;
use intlify_authoring::{
    AuthoringBasis, AuthoringContext, CanonicalLocale, Completeness, ContextKind,
    ExactInputBinding, LocaleData, LocaleFailure, OwnerIdentity, OwnerKind, SnapshotMismatch,
    SourceSnapshot, SurfaceVocabulary, Token, VersionedIdentity,
};
use intlify_authoring_js::{
    admit_units, AdmittedUnit, Grammar, JsAuthoringLimits, JsAuthoringProfile, JsLimitKind,
    ProducerFailure, SourceUnit, UnitMember,
};
use support::{context, limits, member, owner, snapshot};

fn admit<'b>(
    context: &dyn AuthoringContext,
    completeness: Completeness,
    membership: &[UnitMember],
    units: &[SourceUnit<'b>],
    limits: &JsAuthoringLimits,
) -> Result<Vec<AdmittedUnit<'b>>, ProducerFailure> {
    admit_units(
        context,
        &JsAuthoringProfile::new(),
        completeness,
        membership,
        units,
        limits,
    )
}

fn unit<'b>(name: &str, bytes: &'b [u8]) -> SourceUnit<'b> {
    SourceUnit::new(snapshot(name, Grammar::JsModule, bytes), bytes)
}

fn token(value: &str) -> Token {
    Token::new(value).expect("a token")
}

const A: &[u8] = b"f('a')\n";
const B: &[u8] = b"f('b')\n";

#[test]
fn a_context_pinning_another_profile_is_refused() {
    // The builder's own default names the language-neutral profile. A context
    // that was never told which host profile analyzed the source cannot vouch
    // for what this Producer recognized.
    let unpinned = TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
        .build()
        .unwrap();
    assert_eq!(
        admit(
            &unpinned,
            Completeness::Complete,
            &[member("a")],
            &[unit("a", A)],
            &limits()
        ),
        Err(ProducerFailure::ProfileMismatch)
    );
    let other_revision =
        TestContext::builder(owner(), SurfaceVocabulary::new(["checkout"]).unwrap())
            .authoring_profile(VersionedIdentity::literal("intlify-js-dom-authoring", "1"))
            .build()
            .unwrap();
    assert_eq!(
        admit(
            &other_revision,
            Completeness::Complete,
            &[member("a")],
            &[unit("a", A)],
            &limits()
        ),
        Err(ProducerFailure::ProfileMismatch)
    );
}

/// A context that copies a test context but claims a production kind.
struct Relabelled(AuthoringBasis, TestContext);

impl AuthoringContext for Relabelled {
    fn basis(&self) -> &AuthoringBasis {
        &self.0
    }
    fn owner(&self) -> &OwnerIdentity {
        self.1.owner()
    }
    fn default_source_locale(&self) -> Option<&CanonicalLocale> {
        self.1.default_source_locale()
    }
    fn surface_vocabulary(&self) -> &SurfaceVocabulary {
        self.1.surface_vocabulary()
    }
    fn canonicalize(&self, identifier: &str) -> Result<CanonicalLocale, LocaleFailure> {
        self.1.canonicalize(identifier)
    }
    fn usage_profile(&self) -> Option<&VersionedIdentity> {
        self.1.usage_profile()
    }
}

#[test]
fn a_production_context_is_refused_before_anything_is_read() {
    let digest = format!("sha256:{}", "0".repeat(64));
    let basis = AuthoringBasis::new(
        JsAuthoringProfile::new().identity().clone(),
        ContextKind::ApplicationProfile,
        ExactInputBinding::new("storefront", "0", &digest).unwrap(),
        ExactInputBinding::new("vocabulary", "0", &digest).unwrap(),
        VersionedIdentity::literal("canonicalization", "0"),
        LocaleData::new("data", &digest).unwrap(),
        None,
    );
    let relabelled = Relabelled(basis, context());
    // The unit's bytes do not match its snapshot. Refusing the context first
    // shows nothing about the units was looked at.
    let mismatched = SourceUnit::new(snapshot("a", Grammar::JsModule, A), B);
    assert_eq!(
        admit(
            &relabelled,
            Completeness::Complete,
            &[member("a")],
            &[mismatched],
            &limits()
        ),
        Err(ProducerFailure::ProductionContextUnsupported(
            ContextKind::ApplicationProfile
        ))
    );
}

#[test]
fn units_are_admitted_in_unit_order_whatever_order_they_arrive_in() {
    let admitted = admit(
        &context(),
        Completeness::Complete,
        &[member("b"), member("a")],
        &[unit("b", B), unit("a", A)],
        &limits(),
    )
    .unwrap();
    let names: Vec<&str> = admitted
        .iter()
        .map(|unit| unit.snapshot().unit().as_str())
        .collect();
    assert_eq!(names, ["a", "b"]);
    assert_eq!(admitted[0].text(), Some("f('a')\n"));
    assert_eq!(admitted[0].grammar(), Grammar::JsModule);
}

#[test]
fn a_unit_of_another_owner_is_refused() {
    let foreign = SourceSnapshot::new(
        OwnerIdentity::new(OwnerKind::Library, "storefront").unwrap(),
        "a",
        "1",
        Grammar::JsModule.identity(),
        A.len() as u64,
        snapshot("a", Grammar::JsModule, A).utf8_digest().as_str(),
    )
    .unwrap();
    assert_eq!(
        admit(
            &context(),
            Completeness::Complete,
            &[member("a")],
            &[SourceUnit::new(foreign, A)],
            &limits()
        ),
        Err(ProducerFailure::ForeignOwner { unit: token("a") })
    );
}

#[test]
fn a_grammar_outside_the_registry_is_refused() {
    for grammar in [
        VersionedIdentity::literal("intlify-grammar-jsx-module", "0"),
        VersionedIdentity::literal("intlify-grammar-js-module", "1"),
    ] {
        let unregistered = SourceSnapshot::new(
            owner(),
            "a",
            "1",
            grammar,
            A.len() as u64,
            snapshot("a", Grammar::JsModule, A).utf8_digest().as_str(),
        )
        .unwrap();
        assert_eq!(
            admit(
                &context(),
                Completeness::Complete,
                &[member("a")],
                &[SourceUnit::new(unregistered, A)],
                &limits()
            ),
            Err(ProducerFailure::UnregisteredGrammar { unit: token("a") })
        );
    }
}

#[test]
fn bytes_that_are_not_the_snapshots_are_refused_and_say_how() {
    let named = snapshot("a", Grammar::JsModule, A);
    assert_eq!(
        admit(
            &context(),
            Completeness::Complete,
            &[member("a")],
            &[SourceUnit::new(named.clone(), b"f('a')")],
            &limits()
        ),
        Err(ProducerFailure::Snapshot {
            unit: token("a"),
            mismatch: SnapshotMismatch::ByteLength
        })
    );
    // Same length, different bytes: equal length is never evidence alone.
    assert_eq!(B.len(), A.len());
    assert_eq!(
        admit(
            &context(),
            Completeness::Complete,
            &[member("a")],
            &[SourceUnit::new(named, B)],
            &limits()
        ),
        Err(ProducerFailure::Snapshot {
            unit: token("a"),
            mismatch: SnapshotMismatch::Utf8Digest
        })
    );
}

#[test]
fn a_unit_that_is_not_text_is_admitted_for_analysis_to_report() {
    // The bytes are exactly what the snapshot names, so the caller attached
    // them correctly. The unit itself is what is wrong, and that is the
    // author's to fix, so admission lets analysis report it.
    let binary: &[u8] = &[0x66, 0x28, 0xff, 0x29];
    let admitted = admit(
        &context(),
        Completeness::Complete,
        &[member("a")],
        &[unit("a", binary)],
        &limits(),
    )
    .unwrap();
    assert_eq!(admitted[0].text(), None);
}

#[test]
fn a_unit_is_named_once_in_the_scope_and_once_in_the_supply() {
    assert_eq!(
        admit(
            &context(),
            Completeness::Partial,
            &[member("a"), member("a")],
            &[unit("a", A)],
            &limits()
        ),
        Err(ProducerFailure::DuplicateMember { unit: token("a") })
    );
    assert_eq!(
        admit(
            &context(),
            Completeness::Partial,
            &[member("a")],
            &[unit("a", A), unit("a", B)],
            &limits()
        ),
        Err(ProducerFailure::DuplicateUnit { unit: token("a") })
    );
}

#[test]
fn a_unit_outside_the_declared_scope_is_refused() {
    assert_eq!(
        admit(
            &context(),
            Completeness::Partial,
            &[member("a")],
            &[unit("b", B)],
            &limits()
        ),
        Err(ProducerFailure::NotAMember { unit: token("b") })
    );
    // The scope contains the unit at another revision.
    assert_eq!(
        admit(
            &context(),
            Completeness::Partial,
            &[UnitMember::new("a", "2").unwrap()],
            &[unit("a", A)],
            &limits()
        ),
        Err(ProducerFailure::NotAMember { unit: token("a") })
    );
}

#[test]
fn a_complete_scope_needs_the_bytes_of_every_member() {
    assert_eq!(
        admit(
            &context(),
            Completeness::Complete,
            &[member("a"), member("b"), member("c")],
            &[unit("a", A)],
            &limits()
        ),
        Err(ProducerFailure::MissingMember { unit: token("b") }),
        "the first missing member in unit order is named"
    );
    // A partial scope may supply a subset, and says so.
    let admitted = admit(
        &context(),
        Completeness::Partial,
        &[member("a"), member("b")],
        &[unit("b", B)],
        &limits(),
    )
    .unwrap();
    assert_eq!(admitted.len(), 1);
}

#[test]
fn an_empty_complete_scope_is_admitted() {
    assert_eq!(
        admit(&context(), Completeness::Complete, &[], &[], &limits()),
        Ok(Vec::new())
    );
}

#[test]
fn unit_counts_are_bounded_exactly_for_the_scope_and_the_supply() {
    let mut bounded = limits();
    bounded.units = 2;
    assert!(admit(
        &context(),
        Completeness::Complete,
        &[member("a"), member("b")],
        &[unit("a", A), unit("b", B)],
        &bounded
    )
    .is_ok());
    assert_eq!(
        admit(
            &context(),
            Completeness::Partial,
            &[member("a"), member("b"), member("c")],
            &[unit("a", A)],
            &bounded
        ),
        Err(ProducerFailure::Limit(JsLimitKind::Units)),
        "a scope larger than the bound is refused even when little is supplied"
    );
    assert_eq!(
        admit(
            &context(),
            Completeness::Partial,
            &[member("a"), member("b")],
            &[unit("a", A), unit("b", B), unit("c", A)],
            &bounded
        ),
        Err(ProducerFailure::Limit(JsLimitKind::Units))
    );
}

#[test]
fn unit_bytes_are_bounded_exactly_and_counted_as_bytes() {
    // Three scalars, nine bytes: a bound counted in scalars would admit it.
    let text = "日本語".as_bytes();
    let mut bounded = limits();
    bounded.unit_bytes = 9;
    assert!(admit(
        &context(),
        Completeness::Complete,
        &[member("a")],
        &[unit("a", text)],
        &bounded
    )
    .is_ok());
    bounded.unit_bytes = 8;
    assert_eq!(
        admit(
            &context(),
            Completeness::Complete,
            &[member("a")],
            &[unit("a", text)],
            &bounded
        ),
        Err(ProducerFailure::Limit(JsLimitKind::UnitBytes))
    );
}

#[test]
fn total_bytes_are_bounded_exactly() {
    let mut bounded = limits();
    bounded.total_bytes = (A.len() + B.len()) as u64;
    let units = [unit("a", A), unit("b", B)];
    assert!(admit(
        &context(),
        Completeness::Complete,
        &[member("a"), member("b")],
        &units,
        &bounded
    )
    .is_ok());
    bounded.total_bytes -= 1;
    assert_eq!(
        admit(
            &context(),
            Completeness::Complete,
            &[member("a"), member("b")],
            &units,
            &bounded
        ),
        Err(ProducerFailure::Limit(JsLimitKind::TotalBytes))
    );
}

#[test]
fn the_failure_reported_does_not_depend_on_supply_order() {
    let foreign = SourceSnapshot::new(
        OwnerIdentity::new(OwnerKind::Library, "storefront").unwrap(),
        "a",
        "1",
        Grammar::JsModule.identity(),
        A.len() as u64,
        snapshot("a", Grammar::JsModule, A).utf8_digest().as_str(),
    )
    .unwrap();
    let unregistered = SourceSnapshot::new(
        owner(),
        "b",
        "1",
        VersionedIdentity::literal("intlify-grammar-jsx-module", "0"),
        B.len() as u64,
        snapshot("b", Grammar::JsModule, B).utf8_digest().as_str(),
    )
    .unwrap();
    let first = SourceUnit::new(foreign, A);
    let second = SourceUnit::new(unregistered, B);
    for units in [
        [first.clone(), second.clone()],
        [second.clone(), first.clone()],
    ] {
        assert_eq!(
            admit(
                &context(),
                Completeness::Complete,
                &[member("b"), member("a")],
                &units,
                &limits()
            ),
            Err(ProducerFailure::ForeignOwner { unit: token("a") })
        );
    }
}
