// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Assembling an invocation's units into one `authoring-inventory`, and what
//! that inventory may be used for.
//!
//! Every expected range is found in the source text by searching for the
//! bytes it covers, never read back from the analysis under test.

mod support;

use intlify_authoring::test_context::{admit_inventory, LocaleRule, TestContext};
use intlify_authoring::{
    intent_revision, AnalysisWorkspace, AuthoringFailure, AuthoringInventory, Completeness,
    InventoryArtifact, LimitKind, OccurrenceRole, Outcome, SourceBytes, UnitOutcome,
};
use intlify_authoring_js::{
    admit_units, analyze_unit, assemble_inventory, AdmittedUnit, AssembledInventory,
    CheckedInventory, Grammar, JsAnalysisWorkspace, JsAuthoringLimits, ProducerFailure, SourceUnit,
    UnitAnalysis, UnitMember,
};
use serde_json::Value;
use support::{
    at, context, context_builder, dom_profile, limits, member, never, nth, occurrence, profile,
    snapshot, PRELUDE,
};

/// Design 028's representative application, byte for byte.
const APPLICATION: &str = include_str!("../fixtures/phase2/representative-application.js");

/// The scope every fixture declares.
const SCOPE: &str = "storefront-web";

/// Small units, each importing what it uses.
const UNIT_A: &str = "import { intent } from 'fixture-authoring'\nintent('A')\n";
const UNIT_B: &str = "import { intent } from 'fixture-authoring'\nintent('B')\n";
const BLOCKED_A: &str =
    "import { intent } from 'fixture-authoring'\nintent('A')\nintent(computed())\n";
const BLOCKED_B: &str =
    "import { intent } from 'fixture-authoring'\nintent('B')\nintent(computed())\n";

fn members(units: &[&str]) -> Vec<UnitMember> {
    units.iter().map(|unit| member(unit)).collect()
}

/// Admit module-goal units under the DOM profile.
fn admitted<'b>(
    units: &[(&str, &'b [u8])],
    completeness: Completeness,
    membership: &[UnitMember],
) -> Vec<AdmittedUnit<'b>> {
    let supplied: Vec<SourceUnit<'b>> = units
        .iter()
        .map(|(unit, bytes)| SourceUnit::new(snapshot(unit, Grammar::JsModule, bytes), bytes))
        .collect();
    admit_units(
        &context(),
        &dom_profile(),
        completeness,
        membership,
        &supplied,
        &limits(),
    )
    .expect("admitted units")
}

/// Read every unit with one reused workspace.
fn read_all(units: &[AdmittedUnit<'_>], limits: &JsAuthoringLimits) -> Vec<UnitAnalysis> {
    let mut workspace = JsAnalysisWorkspace::new();
    units
        .iter()
        .map(|unit| {
            analyze_unit(
                &context(),
                &dom_profile(),
                unit,
                limits,
                &mut workspace,
                &never,
            )
            .expect("the analysis runs")
        })
        .collect()
}

fn assemble(
    completeness: Completeness,
    membership: &[UnitMember],
    analyses: &[UnitAnalysis],
) -> Result<AssembledInventory, ProducerFailure> {
    assemble_inventory(
        &context(),
        &dom_profile(),
        SCOPE,
        completeness,
        membership,
        analyses,
        &limits(),
    )
}

/// Admit, read and assemble a complete scope of the given units.
fn complete(units: &[(&str, &[u8])]) -> AssembledInventory {
    let names: Vec<&str> = units.iter().map(|(unit, _)| *unit).collect();
    let membership = members(&names);
    let admitted = admitted(units, Completeness::Complete, &membership);
    let analyses = read_all(&admitted, &limits());
    assemble(Completeness::Complete, &membership, &analyses).expect("assembled")
}

fn bytes(artifact: &InventoryArtifact) -> Vec<u8> {
    serde_json::to_vec(artifact).expect("serializable")
}

fn outcomes(inventory: &AuthoringInventory) -> Vec<(String, UnitOutcome)> {
    inventory
        .units()
        .iter()
        .map(|unit| (unit.source().unit().as_str().to_owned(), unit.outcome()))
        .collect()
}

/// Check that no two declarations of one unit claim overlapping source, so
/// nothing was extracted twice.
fn assert_disjoint(inventory: &AuthoringInventory) {
    for pair in inventory.declarations().windows(2) {
        let (left, right) = (pair[0].occurrence(), pair[1].occurrence());
        if left.source() == right.source() {
            assert!(
                left.range().end() <= right.range().start(),
                "{left:?} overlaps {right:?}"
            );
        }
    }
}

#[test]
fn the_representative_application_is_one_complete_checked_inventory() {
    let assembled = complete(&[("app", APPLICATION.as_bytes())]);
    assert_eq!(assembled.diagnostics(), []);
    assert_eq!(assembled.outcome(), Outcome::Checked);
    let Some(CheckedInventory::Complete(artifact)) = assembled.checked_inventory() else {
        panic!("a complete checked inventory");
    };
    let inventory = artifact.body();
    assert!(inventory.is_complete_checked());
    assert_eq!(inventory.scope().as_str(), SCOPE);
    assert_eq!(
        outcomes(inventory),
        [("app".to_owned(), UnitOutcome::Checked)]
    );
    assert_disjoint(inventory);

    let tag = at(APPLICATION, "mf2`Hello {$name}!`");
    let declared: Vec<_> = inventory
        .declarations()
        .iter()
        .map(|facts| {
            let description = facts
                .projection()
                .description
                .as_ref()
                .map(|text| text.as_str().to_owned());
            (occurrence(facts.occurrence()), description)
        })
        .collect();
    assert_eq!(
        declared,
        [
            (
                (OccurrenceRole::Mf2Declaration, tag),
                Some("Greeting addressed to the signed-in user".to_owned())
            ),
            ((OccurrenceRole::UiLiteral, at(APPLICATION, "'Save'")), None),
            (
                (OccurrenceRole::IntentLiteral, at(APPLICATION, "'Welcome'")),
                None
            ),
        ]
    );
    let referenced: Vec<_> = inventory
        .references()
        .iter()
        .map(|reference| {
            let names: Vec<_> = reference
                .declarations()
                .iter()
                .map(|declaration| occurrence(declaration).1)
                .collect();
            (occurrence(reference.occurrence()).1, names)
        })
        .collect();
    // Both greetings name the one declaration occurrence.
    assert_eq!(
        referenced,
        [
            (
                at(APPLICATION, "save.textContent = 'Save'"),
                vec![at(APPLICATION, "'Save'")]
            ),
            (
                at(APPLICATION, "intent('Welcome')"),
                vec![at(APPLICATION, "'Welcome'")]
            ),
            (nth(APPLICATION, "intent(greeting, { name })", 0), vec![tag]),
            (nth(APPLICATION, "intent(greeting, { name })", 1), vec![tag]),
        ]
    );
    let exclusions = inventory.exclusions();
    assert_eq!(exclusions.len(), 1);
    assert_eq!(
        occurrence(exclusions[0].occurrence()).1,
        at(APPLICATION, "noIntent('Intlify', 'Product name')")
    );
    assert_eq!(exclusions[0].reason(), "Product name");
}

#[test]
fn the_sealed_artifact_is_admitted_again_against_its_own_bytes() {
    let assembled = complete(&[("app", APPLICATION.as_bytes())]);
    let artifact = assembled.inspection_inventory();
    let encoded = bytes(artifact);
    let admit = |sources: &[SourceBytes<'_>]| {
        admit_inventory(
            &encoded,
            &context(),
            sources,
            &limits().authoring,
            &mut AnalysisWorkspace::new(),
        )
        .expect("admitted")
    };
    let verified = admit(&[SourceBytes {
        unit: "app",
        bytes: APPLICATION.as_bytes(),
    }]);
    assert_eq!(verified.artifact(), artifact);
    assert_eq!(verified.reference(), artifact.reference());
    assert!(verified.unverified_units().is_empty());
    // Without the bytes the ranges are checked for shape only, and the result
    // says so.
    let unverified = admit(&[]);
    assert_eq!(unverified.artifact(), artifact);
    let names: Vec<&str> = unverified
        .unverified_units()
        .iter()
        .map(intlify_authoring::Token::as_str)
        .collect();
    assert_eq!(names, ["app"]);
}

#[test]
fn a_partial_scope_is_checked_for_its_part_and_claims_no_more() {
    let membership = members(&["a", "b"]);
    let admitted = admitted(
        &[("a", UNIT_A.as_bytes())],
        Completeness::Partial,
        &membership,
    );
    let analyses = read_all(&admitted, &limits());
    let assembled = assemble(Completeness::Partial, &membership, &analyses).unwrap();
    assert_eq!(assembled.outcome(), Outcome::Checked);
    let Some(CheckedInventory::Partial(artifact)) = assembled.checked_inventory() else {
        panic!("a partial checked inventory");
    };
    // The omitted member is not in the view, and the view is no complete
    // input: neither the type nor the inventory says otherwise.
    assert_eq!(
        outcomes(artifact.body()),
        [("a".to_owned(), UnitOutcome::Checked)]
    );
    assert_eq!(artifact.body().declarations().len(), 1);
    assert_eq!(
        assembled
            .checked_inventory()
            .and_then(CheckedInventory::complete),
        None
    );
    assert!(!artifact.body().is_complete_checked());
}

#[test]
fn a_complete_scope_has_to_have_every_member_read() {
    let membership = members(&["a", "b"]);
    let admitted = admitted(
        &[("a", UNIT_A.as_bytes()), ("b", UNIT_B.as_bytes())],
        Completeness::Complete,
        &membership,
    );
    let analyses = read_all(&admitted, &limits());
    // Dropping a member's analysis and still calling the scope complete would
    // claim the absence of declarations nobody looked for.
    assert_eq!(
        assemble(Completeness::Complete, &membership, &analyses[..1]).unwrap_err(),
        ProducerFailure::MissingMember {
            unit: member("b").unit().clone()
        }
    );
    // The same analysis is a sound partial view.
    let partial = assemble(Completeness::Partial, &membership, &analyses[..1]).unwrap();
    assert!(matches!(
        partial.checked_inventory(),
        Some(CheckedInventory::Partial(_))
    ));
}

#[test]
fn a_unit_that_could_not_be_read_is_recorded_as_failed_and_claims_nothing() {
    let assembled = complete(&[
        ("a", UNIT_A.as_bytes()),
        ("b", "const = 1\n".as_bytes()),
        ("c", &[0x66_u8, 0xff][..]),
    ]);
    assert_eq!(assembled.outcome(), Outcome::Blocked);
    assert_eq!(assembled.checked_inventory(), None);
    let inventory = assembled.inspection_inventory().body();
    assert_eq!(
        outcomes(inventory),
        [
            ("a".to_owned(), UnitOutcome::Checked),
            ("b".to_owned(), UnitOutcome::Failed),
            ("c".to_owned(), UnitOutcome::Failed),
        ]
    );
    assert!(!inventory.is_complete_checked());
    // Only the unit that was read has facts.
    let sources: Vec<&str> = inventory
        .declarations()
        .iter()
        .map(|facts| facts.occurrence().source().unit().as_str())
        .collect();
    assert_eq!(sources, ["a"]);
    let details: Vec<&str> = assembled
        .diagnostics()
        .iter()
        .filter_map(intlify_authoring::Diagnostic::detail)
        .map(intlify_authoring::Detail::as_str)
        .collect();
    assert_eq!(details, ["host-syntax-invalid", "unit-not-text"]);
}

#[test]
fn a_cancelled_unit_leaves_nothing_to_assemble() {
    let admitted = admitted(
        &[("a", APPLICATION.as_bytes())],
        Completeness::Complete,
        &members(&["a"]),
    );
    let stopped = analyze_unit(
        &context(),
        &dom_profile(),
        &admitted[0],
        &limits(),
        &mut JsAnalysisWorkspace::new(),
        &|| true,
    );
    assert_eq!(stopped, Err(ProducerFailure::Cancelled));
}

#[test]
fn an_unreferenced_declaration_is_still_a_live_declaration() {
    let text = format!("{PRELUDE}const unused = mf2`Unused`\n");
    let assembled = complete(&[("a", text.as_bytes())]);
    let inventory = assembled.checked_inventory().unwrap().artifact().body();
    let declared: Vec<_> = inventory
        .declarations()
        .iter()
        .map(|facts| occurrence(facts.occurrence()))
        .collect();
    assert_eq!(
        declared,
        [(OccurrenceRole::Mf2Declaration, at(&text, "mf2`Unused`"))]
    );
    assert_eq!(inventory.references(), []);
}

#[test]
fn a_reference_records_where_it_is_written_not_whether_it_runs() {
    // Code after a return and under a false condition never runs, and the
    // references written there are recorded like any other: a reference is a
    // finite fact about source, not a claim of final reachability.
    let text = format!(
        "{PRELUDE}const greeting = mf2`Hi`\nexport function render() {{\n  intent(greeting)\n  if (false) {{ intent(greeting) }}\n  return\n  intent(greeting)\n}}\n"
    );
    let assembled = complete(&[("a", text.as_bytes())]);
    let inventory = assembled.checked_inventory().unwrap().artifact().body();
    let referenced: Vec<_> = inventory
        .references()
        .iter()
        .map(|reference| occurrence(reference.occurrence()).1)
        .collect();
    assert_eq!(
        referenced,
        [
            nth(&text, "intent(greeting)", 0),
            nth(&text, "intent(greeting)", 1),
            nth(&text, "intent(greeting)", 2),
        ]
    );
}

#[test]
fn a_blocked_result_is_never_complete_input() {
    let text = format!("{PRELUDE}intent('A')\nintent(computed())\n");
    let assembled = complete(&[("a", text.as_bytes())]);
    assert_eq!(assembled.outcome(), Outcome::Blocked);
    assert_eq!(assembled.checked_inventory(), None);
    let inventory = assembled.inspection_inventory().body();
    assert!(!inventory.is_complete_checked());
    // What was established still supports inspection.
    assert_eq!(
        outcomes(inventory),
        [("a".to_owned(), UnitOutcome::Blocked)]
    );
    assert_eq!(inventory.declarations().len(), 1);
}

#[test]
fn analyses_are_checked_against_the_scope_before_any_fact_is_read() {
    let membership = members(&["a", "b"]);
    let admitted = admitted(
        &[("a", UNIT_A.as_bytes()), ("b", UNIT_B.as_bytes())],
        Completeness::Complete,
        &membership,
    );
    let analyses = read_all(&admitted, &limits());
    let unit = |name: &str| member(name).unit().clone();

    let twice = [analyses[0].clone(), analyses[0].clone()];
    assert_eq!(
        assemble(Completeness::Partial, &membership, &twice).unwrap_err(),
        ProducerFailure::DuplicateUnit { unit: unit("a") }
    );
    assert_eq!(
        assemble(Completeness::Partial, &members(&["a"]), &analyses).unwrap_err(),
        ProducerFailure::NotAMember { unit: unit("b") }
    );
    assert_eq!(
        assemble(Completeness::Partial, &members(&["a", "a"]), &analyses[..1]).unwrap_err(),
        ProducerFailure::DuplicateMember { unit: unit("a") }
    );
    assert_eq!(
        assemble_inventory(
            &context(),
            &dom_profile(),
            "",
            Completeness::Complete,
            &membership,
            &analyses,
            &limits(),
        )
        .unwrap_err(),
        ProducerFailure::InvalidScope
    );
    let mut few = limits();
    few.units = 1;
    assert_eq!(
        assemble_inventory(
            &context(),
            &dom_profile(),
            SCOPE,
            Completeness::Complete,
            &membership,
            &analyses,
            &few,
        )
        .unwrap_err(),
        ProducerFailure::Limit(intlify_authoring_js::JsLimitKind::Units)
    );
}

#[test]
fn a_unit_of_another_owner_is_refused() {
    let other = intlify_authoring::OwnerIdentity::new(
        intlify_authoring::OwnerKind::Application,
        "back-office",
    )
    .unwrap();
    let theirs = TestContext::builder(
        other.clone(),
        intlify_authoring::SurfaceVocabulary::new(["checkout", "nav"]).unwrap(),
    )
    .authoring_profile(dom_profile().identity().clone())
    .usage_profile(intlify_authoring_js::JsAuthoringProfile::usage_profile())
    .default_source_locale("en")
    .default_surface_class("checkout")
    .build()
    .unwrap();
    let snapshot = intlify_authoring::SourceSnapshot::new(
        other,
        "a",
        "1",
        Grammar::JsModule.identity(),
        UNIT_A.len() as u64,
        support::snapshot("a", Grammar::JsModule, UNIT_A.as_bytes())
            .utf8_digest()
            .as_str(),
    )
    .unwrap();
    let admitted = admit_units(
        &theirs,
        &dom_profile(),
        Completeness::Complete,
        &members(&["a"]),
        &[SourceUnit::new(snapshot, UNIT_A.as_bytes())],
        &limits(),
    )
    .unwrap();
    let analysis = analyze_unit(
        &theirs,
        &dom_profile(),
        &admitted[0],
        &limits(),
        &mut JsAnalysisWorkspace::new(),
        &never,
    )
    .unwrap();
    assert_eq!(
        assemble(Completeness::Complete, &members(&["a"]), &[analysis]).unwrap_err(),
        ProducerFailure::ForeignOwner {
            unit: member("a").unit().clone()
        }
    );
}

#[test]
fn diagnostics_come_in_reporting_order_across_units() {
    // The shared crate's records sort after the host's, so reading the units
    // in turn would not give 016's order.
    let missing = "import { intent } from 'fixture-authoring'\nintent('Hello {$name}!')\n";
    let assembled = complete(&[("a", missing.as_bytes()), ("b", BLOCKED_B.as_bytes())]);
    let reported: Vec<(&str, &str)> = assembled
        .diagnostics()
        .iter()
        .map(|record| {
            (
                record.location().source().unit().as_str(),
                record
                    .detail()
                    .map_or(record.origin().code(), |detail| detail.as_str()),
            )
        })
        .collect();
    assert_eq!(
        reported,
        [("b", "message-dynamic"), ("a", "parameter-missing")]
    );
}

#[test]
fn units_read_under_other_inputs_are_not_put_into_one_inventory() {
    let membership = members(&["a"]);
    let admitted = admitted(
        &[("a", UNIT_A.as_bytes())],
        Completeness::Complete,
        &membership,
    );
    let unit = member("a").unit().clone();
    // Read under a profile that admits no DOM global.
    let other_profile = analyze_unit(
        &context(),
        &profile(),
        &admitted[0],
        &limits(),
        &mut JsAnalysisWorkspace::new(),
        &never,
    )
    .unwrap();
    assert_eq!(
        assemble(Completeness::Complete, &membership, &[other_profile]).unwrap_err(),
        ProducerFailure::ForeignAnalysis { unit: unit.clone() }
    );
    // Read under a context with another class default.
    let other_context = context_builder()
        .default_surface_class("nav")
        .build()
        .unwrap();
    let analysis = analyze_unit(
        &other_context,
        &dom_profile(),
        &admitted[0],
        &limits(),
        &mut JsAnalysisWorkspace::new(),
        &never,
    )
    .unwrap();
    assert_eq!(
        assemble(Completeness::Complete, &membership, &[analysis]).unwrap_err(),
        ProducerFailure::ForeignAnalysis { unit }
    );
}

#[test]
fn the_whole_invocation_is_bounded_exactly() {
    let units: [(&str, &[u8]); 2] = [("a", BLOCKED_A.as_bytes()), ("b", BLOCKED_B.as_bytes())];
    let membership = members(&["a", "b"]);
    let admitted = admitted(&units, Completeness::Complete, &membership);
    let at_most = |declarations: u64, diagnostics: u64| {
        let mut bounded = limits();
        bounded.authoring.declarations = declarations;
        bounded.authoring.diagnostics = diagnostics;
        // Each unit stays within the bound on its own.
        let analyses = read_all(&admitted, &bounded);
        for analysis in &analyses {
            assert_eq!(analysis.inspection_facts().declarations().len(), 1);
            assert_eq!(analysis.diagnostics().len(), 1);
        }
        assemble_inventory(
            &context(),
            &dom_profile(),
            SCOPE,
            Completeness::Complete,
            &membership,
            &analyses,
            &bounded,
        )
    };
    assert!(at_most(2, 2).is_ok());
    assert_eq!(
        at_most(1, 2).unwrap_err(),
        ProducerFailure::Authoring(AuthoringFailure::Limit(LimitKind::Declarations))
    );
    assert_eq!(
        at_most(2, 1).unwrap_err(),
        ProducerFailure::Authoring(AuthoringFailure::Limit(LimitKind::Diagnostics))
    );
}

#[test]
fn the_artifact_does_not_depend_on_order_or_scheduling() {
    let a =
        format!("{PRELUDE}const greeting = mf2`Hello {{$name}}!`\nintent(greeting, {{ name }})\n");
    let b = APPLICATION.to_owned();
    let c = format!("{PRELUDE}intent('C')\nintent(computed())\n");
    let units: [(&str, &[u8]); 3] = [
        ("a", a.as_bytes()),
        ("b", b.as_bytes()),
        ("c", c.as_bytes()),
    ];
    let membership = members(&["c", "a", "b"]);

    let in_order = complete(&units);

    let reversed: Vec<(&str, &[u8])> = units.iter().rev().copied().collect();
    let admitted_reversed = admitted(&reversed, Completeness::Complete, &membership);
    let mut analyses = read_all(&admitted_reversed, &limits());
    analyses.rotate_left(1);
    let shuffled = assemble(Completeness::Complete, &membership, &analyses).unwrap();

    // Each unit read on its own thread, with its own workspace.
    let admitted = admitted(&units, Completeness::Complete, &membership);
    let threaded: Vec<UnitAnalysis> = std::thread::scope(|scope| {
        let handles: Vec<_> = admitted
            .iter()
            .map(|unit| {
                scope.spawn(move || {
                    analyze_unit(
                        &context(),
                        &dom_profile(),
                        unit,
                        &limits(),
                        &mut JsAnalysisWorkspace::new(),
                        &never,
                    )
                    .unwrap()
                })
            })
            .collect();
        handles
            .into_iter()
            .rev()
            .map(|handle| handle.join().unwrap())
            .collect()
    });
    let threaded = assemble(Completeness::Complete, &membership, &threaded).unwrap();

    let expected = bytes(in_order.inspection_inventory());
    assert_eq!(bytes(shuffled.inspection_inventory()), expected);
    assert_eq!(bytes(threaded.inspection_inventory()), expected);
    assert_eq!(shuffled.diagnostics(), in_order.diagnostics());
    assert_eq!(threaded.diagnostics(), in_order.diagnostics());
    assert_disjoint(in_order.inspection_inventory().body());
}

/// The source an `intent()` call needs to declare a vector's message, with
/// the annotation and parameters its projection implies.
fn host_source(vector: &Value) -> String {
    let projection = &vector["preimage"]["projection"];
    let mut members = Vec::new();
    let locale = projection["sourceLocale"].as_str().unwrap();
    if locale != "en" {
        members.push(format!("\"sourceLocale\": \"{locale}\""));
    }
    if let Some(description) = projection["description"].as_str() {
        members.push(format!("\"description\": \"{description}\""));
    }
    let annotation = if members.is_empty() {
        String::new()
    } else {
        format!("/* @intlify {{ {} }} */\n", members.join(", "))
    };
    let names: Vec<&str> = projection["parameters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| name.as_str().unwrap())
        .collect();
    let parameters = if names.is_empty() {
        String::new()
    } else {
        format!(", {{ {} }}", names.join(", "))
    };
    let source = vector["mf2Source"].as_str().unwrap();
    assert!(!source.contains('`') && !source.contains("${") && !source.contains('\\'));
    format!("{PRELUDE}{annotation}intent(`{source}`{parameters})\n")
}

#[test]
fn host_declarations_reach_the_committed_independent_revision_vectors() {
    // The vectors were written from the shared crate and are re-hashed by an
    // independent implementation. Reaching the same projection and revision
    // through host source ties this Producer to both.
    let document: Value = serde_json::from_str(include_str!(
        "../../intlify_authoring/fixtures/phase1/revision-vectors.json"
    ))
    .unwrap();
    let context: TestContext = context_builder()
        .default_surface_class("checkout")
        .rule(LocaleRule::canonical("ja", "ja"))
        .build()
        .unwrap();
    let vectors = document["vectors"].as_array().unwrap();
    assert_eq!(vectors.len(), 11);
    for vector in vectors {
        let id = vector["id"].as_str().unwrap();
        let text = host_source(vector);
        let analysis =
            support::try_analyze_in(&context, Grammar::JsModule, &text, &profile(), &limits())
                .unwrap();
        assert_eq!(analysis.diagnostics(), [], "{id}");
        let facts = &analysis.inspection_facts().declarations()[0];
        assert_eq!(
            serde_json::to_value(facts.projection()).unwrap(),
            vector["preimage"]["projection"],
            "{id}"
        );
        assert_eq!(
            intent_revision(facts.projection()).unwrap().as_str(),
            vector["revision"].as_str().unwrap(),
            "{id}"
        );
    }
}
