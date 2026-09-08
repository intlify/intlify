// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::*;

use std::sync::Arc;

use serde_json::{json, Value};

use crate::benchmark::clock::tests::ScriptedClock;
use crate::benchmark::operation::{Operation, Output};
use crate::input_limits::Bound;
use crate::materialize::materialize_file;
use crate::structural::selection::SelectorInput;

use super::super::{Recipe, Selector};

fn original() -> Document {
    serde_json::from_str(EXPECTATIONS).unwrap()
}

fn case(operation: Operation, recipe: Recipe, selector: Selector) -> Declaration {
    declarations()
        .into_iter()
        .find(|case| {
            case.operation == operation
                && case.fixture == recipe
                && case.selector == selector
                && case.limit.is_none()
        })
        .unwrap()
}

fn different_digest() -> Digest {
    Frame::new("deliberately-wrong-registry-expectation").finish()
}

#[test]
fn registry_identity_revision_codec_and_exact_inventory_are_required() {
    for (field, expected) in [
        ("identity", RegistryFailure::Identity),
        ("revision", RegistryFailure::Revision),
        ("observationCodec", RegistryFailure::ObservationCodec),
    ] {
        let mut value = serde_json::to_value(original()).unwrap();
        value[field] = json!("unknown");
        assert_eq!(
            Registry::admit_document(serde_json::from_value(value).unwrap()).err(),
            Some(expected)
        );
    }
    let mut missing = original();
    missing.rows.remove(3);
    let mut duplicate = original();
    duplicate.rows[1] = duplicate.rows[0].clone();
    let mut extra = original();
    extra.rows.push(extra.rows[0].clone());
    let mut reordered = original();
    reordered.rows.swap(0, 1);
    let mut unknown = original();
    unknown.rows[0].declaration.fixture_revision = "unknown".into();
    for document in [missing, duplicate, extra, reordered, unknown] {
        assert_eq!(
            Registry::admit_document(document).err(),
            Some(RegistryFailure::Inventory)
        );
    }
}

#[test]
fn expectation_objects_are_closed_and_nullable_fields_must_be_present() {
    let value: Value = serde_json::from_str(EXPECTATIONS).unwrap();
    for path in ["", "/rows/0", "/rows/0/declaration", "/rows/0/result"] {
        for key in value.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = value.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                serde_json::from_value::<Document>(missing).is_err(),
                "{path}/{key}"
            );
        }
        let mut extra = value.clone();
        extra
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("extra".into(), json!(true));
        assert!(serde_json::from_value::<Document>(extra).is_err(), "{path}");
    }
    let duplicate = EXPECTATIONS.replacen(
        "\"identity\":",
        "\"identity\": \"duplicate\", \"identity\":",
        1,
    );
    assert!(serde_json::from_str::<Document>(&duplicate).is_err());
    let null_entry = value["rows"]
        .as_array()
        .unwrap()
        .iter()
        .position(|row| row["result"]["entry"].is_null())
        .unwrap();
    let mut missing_null = value;
    missing_null["rows"][null_entry]["result"]
        .as_object_mut()
        .unwrap()
        .remove("entry");
    assert!(serde_json::from_value::<Document>(missing_null).is_err());
}

#[test]
fn changed_pinned_context_result_or_work_does_not_authorize_a_fixture() {
    let declaration = original().rows[0].declaration.clone();
    for (field, expected) in [
        ("/inputContext", FixtureFailure::InputContextMismatch),
        ("/result/shared", FixtureFailure::ResultMismatch),
        ("/result/entry", FixtureFailure::ResultMismatch),
        ("/logicalWork", FixtureFailure::LogicalWorkMismatch),
    ] {
        let mut value = serde_json::to_value(original()).unwrap();
        *value["rows"][0].pointer_mut(field).unwrap() =
            serde_json::to_value(different_digest()).unwrap();
        let registry = Registry::admit_document(serde_json::from_value(value).unwrap()).unwrap();
        assert_eq!(
            registry.prepare(&declaration).err(),
            Some(expected),
            "{field}"
        );
    }
}

#[test]
fn equivalent_failures_do_not_hide_different_raw_inputs() {
    let declaration = case(
        Operation::FileMaterialization,
        Recipe::InvalidJson,
        Selector::Absent,
    );
    let mut candidate = prepare(&declaration).unwrap();
    let Prepared::Entry { source, .. } = &mut candidate.prepared else {
        unreachable!()
    };
    *source = Arc::from(&b"{sad"[..]);
    let repeated = candidate
        .prepared
        .once(&ScriptedClock::nanos([0, 1]))
        .unwrap()
        .output;
    assert_eq!(repeated.observe().unwrap(), candidate.observation);
    assert!(LogicalWork::observe(&candidate.prepared, &repeated)
        .unwrap()
        .matches_expected(&candidate.logical_work));
    assert_eq!(
        Registry::load().unwrap().admit_candidate(candidate).err(),
        Some(FixtureFailure::InputContextMismatch)
    );
}

#[test]
fn operation_and_preparation_bounds_are_bound_even_when_output_would_not_change() {
    let declaration = case(
        Operation::FileMaterialization,
        Recipe::Minimal,
        Selector::Absent,
    );
    let registry = Registry::load().unwrap();
    let mut operation = prepare(&declaration).unwrap();
    let Prepared::Entry { limits, .. } = &mut operation.prepared else {
        unreachable!()
    };
    limits.raw.max_file_bytes = Bound::new(1_000_001).unwrap();
    let repeated = operation
        .prepared
        .once(&ScriptedClock::nanos([0, 1]))
        .unwrap()
        .output;
    assert_eq!(repeated.observe().unwrap(), operation.observation);
    assert_eq!(
        registry.admit_candidate(operation).err(),
        Some(FixtureFailure::InputContextMismatch)
    );

    let mut preparation = prepare(&declaration).unwrap();
    preparation.input_limits.as_mut().unwrap().value.max_nodes = Bound::new(100_001).unwrap();
    assert_eq!(
        registry.admit_candidate(preparation).err(),
        Some(FixtureFailure::InputContextMismatch)
    );

    let mut structural = prepare(&case(
        Operation::StructuralAnalysis,
        Recipe::Minimal,
        Selector::Absent,
    ))
    .unwrap();
    let Prepared::Structural { limits, .. } = &mut structural.prepared else {
        unreachable!()
    };
    limits.max_profiles = Bound::new(65).unwrap();
    assert_eq!(
        registry.admit_candidate(structural).err(),
        Some(FixtureFailure::InputContextMismatch)
    );
}

#[test]
fn selector_matching_does_not_accept_a_same_length_failure_or_expose_raw_content() {
    let declaration = case(
        Operation::ProfileSelection,
        Recipe::Minimal,
        Selector::Unknown,
    );
    let mut candidate = prepare(&declaration).unwrap();
    let Prepared::Select { analysis, selector } = &mut candidate.prepared else {
        unreachable!()
    };
    *selector = SelectorInput::string("mystery", analysis.benchmark_profile_id_bound());
    let repeated = candidate
        .prepared
        .once(&ScriptedClock::nanos([0, 1]))
        .unwrap()
        .output;
    assert_eq!(repeated.observe().unwrap(), candidate.observation);
    assert!(LogicalWork::observe(&candidate.prepared, &repeated)
        .unwrap()
        .matches_expected(&candidate.logical_work));
    let error = Registry::load()
        .unwrap()
        .admit_candidate(candidate)
        .err()
        .unwrap();
    assert_eq!(
        error,
        FixtureFailure::Context(ContextFailure::SelectorMismatch)
    );
    assert!(!format!("{error:?}").contains("mystery"));

    // Normalized over-limit input has only the bound's first-over witness. The
    // registry must not recover discarded bytes or a final raw string length.
    let mut marker = prepare(&case(
        Operation::ProfileSelection,
        Recipe::Minimal,
        Selector::FirstOverByteLimit,
    ))
    .unwrap();
    let Prepared::Select { analysis, selector } = &mut marker.prepared else {
        unreachable!()
    };
    *selector = SelectorInput::string(
        &"secret-canary".repeat(100),
        analysis.benchmark_profile_id_bound(),
    );
    assert!(Registry::load().unwrap().admit_candidate(marker).is_ok());
}

#[test]
fn authoring_context_retains_source_and_structural_preconditions_not_just_the_root_value() {
    let declaration = case(
        Operation::AuthoringConstruction,
        Recipe::Minimal,
        Selector::Absent,
    );
    let mut candidate = prepare(&declaration).unwrap();
    let padded = prepare(&case(
        Operation::AuthoringConstruction,
        Recipe::PaddedBytes,
        Selector::Absent,
    ))
    .unwrap();
    assert_eq!(candidate.observation, padded.observation);
    candidate.prepared = padded.prepared;
    assert_eq!(
        Registry::load().unwrap().admit_candidate(candidate).err(),
        Some(FixtureFailure::InputContextMismatch)
    );
}

#[test]
fn cached_candidate_summaries_cannot_replace_reobservation_of_actual_output() {
    let declaration = case(
        Operation::FileMaterialization,
        Recipe::Minimal,
        Selector::Absent,
    );
    let mut candidate = prepare(&declaration).unwrap();
    candidate.output = Output::Entry(materialize_file(
        Arc::from(&b"null"[..]),
        candidate.input_limits.unwrap(),
    ));
    // Leave the cached summaries equal to the pinned row deliberately.
    assert_eq!(
        Registry::load().unwrap().admit_candidate(candidate).err(),
        Some(FixtureFailure::ResultMismatch)
    );

    let mut candidate = prepare(&declaration).unwrap();
    let mut work = serde_json::to_value(&candidate.logical_work).unwrap();
    work["facts"][0]["observation"]["value"] = json!("0");
    candidate.logical_work = serde_json::from_value(work).unwrap();
    assert_eq!(
        Registry::load().unwrap().admit_candidate(candidate).err(),
        Some(FixtureFailure::LogicalWorkMismatch)
    );

    let mut candidate = prepare(&declaration).unwrap();
    candidate.observation.shared = different_digest();
    assert_eq!(
        Registry::load().unwrap().admit_candidate(candidate).err(),
        Some(FixtureFailure::ResultMismatch)
    );
}

#[test]
fn candidates_cannot_be_relabelled_or_rebound_to_other_operations() {
    let declaration = case(
        Operation::FileMaterialization,
        Recipe::Minimal,
        Selector::Absent,
    );
    let registry = Registry::load().unwrap();
    let mut unknown = declaration.clone();
    unknown.fixture_revision = "unknown".into();
    assert_eq!(
        registry.prepare(&unknown).err(),
        Some(FixtureFailure::UndeclaredCase)
    );
    let mut relabelled = prepare(&declaration).unwrap();
    relabelled.declaration = case(
        Operation::FileMaterialization,
        Recipe::PaddedBytes,
        Selector::Absent,
    );
    assert_eq!(
        registry.admit_candidate(relabelled).err(),
        Some(FixtureFailure::InputContextMismatch)
    );
    let mut rebound = prepare(&declaration).unwrap();
    rebound.prepared = prepare(&case(
        Operation::AuthoringConstruction,
        Recipe::Minimal,
        Selector::Absent,
    ))
    .unwrap()
    .prepared;
    assert_eq!(
        registry.admit_candidate(rebound).err(),
        Some(FixtureFailure::Context(ContextFailure::OperationMismatch))
    );
}

/// Print candidates for an explicit review, never update files or admit them.
/// Independent semantic/fixture tests must pass before accepting any printed row.
#[test]
#[ignore = "manual owner-fixture review; never auto-update expectations"]
fn print_candidate_expectations_for_review() {
    let rows = declarations()
        .into_iter()
        .map(|declaration| {
            let candidate = prepare(&declaration).unwrap();
            Row {
                declaration,
                input_context: context::observe(&candidate).unwrap(),
                result: candidate.output.observe().unwrap(),
                logical_work: work_digest(&candidate.logical_work).unwrap(),
            }
        })
        .collect();
    let document = Document {
        identity: IDENTITY.into(),
        revision: REVISION.into(),
        observation_codec: OBSERVATION_CODEC.into(),
        rows,
    };
    println!("BEGIN_OWNER_FIXTURE_CANDIDATES");
    println!("{}", serde_json::to_string_pretty(&document).unwrap());
    println!("END_OWNER_FIXTURE_CANDIDATES");
}
