// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_measurement::plan::{case_identity_input, MAX_PLAN_BYTES};

use super::super::encoding;
use super::*;
use intlify_measurement::identity::{CommonDomain as InstanceDomain, IntegrityDigest};
use serde_json::{json, Value};

/// Recompute a submitted document's own integrity digest after changing it.
///
/// The change is applied to the serialized document rather than through the
/// record type, because that is what a submitter can actually do: a rehashed
/// document is self-consistent, and admission must still reject it.
fn rehashed(mut value: Value) -> Vec<u8> {
    let digest = IntegrityDigest::from_hash(encoding::record_hash(&value).unwrap());
    value["envelope"]["integrityDigest"] = serde_json::to_value(digest).unwrap();
    serde_json::to_vec(&value).unwrap()
}

fn issue() -> IssuedRunPlan {
    let registry = Registry::load().unwrap();
    let context = CaptureContext::acquire(&registry).unwrap();
    super::issue(&registry, &context).unwrap()
}

#[test]
fn complete_schemas_are_fresh_closed_and_accept_actual_issued_plan_and_case_inputs() {
    let plan_text = include_str!("../../../../schema/measurement-run-plan-v0.schema.json");
    let case_text = include_str!("../../../../schema/measurement-case-identity-v0.schema.json");
    let plan_schema: Value = serde_json::from_str(plan_text).unwrap();
    let case_schema: Value = serde_json::from_str(case_text).unwrap();
    assert_eq!(
        crate::schema::format_schema(record_schema().unwrap()).unwrap(),
        plan_text
    );
    assert_eq!(
        crate::schema::format_schema(super::case_schema().unwrap()).unwrap(),
        case_text
    );
    assert_eq!(plan_schema["additionalProperties"], false);
    assert_eq!(case_schema["additionalProperties"], false);
    let plan_validator = jsonschema::draft7::new(&plan_schema).unwrap();
    let case_validator = jsonschema::draft7::new(&case_schema).unwrap();
    let plan = issue();
    let value = serde_json::to_value(plan.document()).unwrap();
    assert!(
        plan_validator.is_valid(&value),
        "{:?}",
        plan_validator.iter_errors(&value).collect::<Vec<_>>()
    );
    for projection in plan.projections() {
        let value = case_identity_input(projection).unwrap();
        assert!(case_validator.is_valid(&value));
    }
    let mut null_context = value.clone();
    null_context["envelope"]["creationContext"] = Value::Null;
    assert!(!plan_validator.is_valid(&null_context));
    assert!(serde_json::from_value::<RunPlanRecord>(null_context).is_err());
    let mut with_context = value;
    with_context["envelope"]["creationContext"] =
        json!({"createdAtUnixNanoseconds": u64::MAX.to_string()});
    assert!(plan_validator.is_valid(&with_context));
    with_context["envelope"]["creationContext"]["createdAtUnixNanoseconds"] = json!(u64::MAX);
    assert!(!plan_validator.is_valid(&with_context));
}

#[test]
fn issuance_freezes_unique_case_inventory_but_fresh_instances_do_not_change_case_identity() {
    let first = issue();
    let second = issue();
    assert_eq!(first.document().body.case_inventory.len(), 127);
    assert_eq!(
        first.document().body.case_inventory,
        second.document().body.case_inventory
    );
    assert_eq!(first.projections(), second.projections());
    assert_ne!(first.document().identity(), second.document().identity());
    assert_ne!(
        first.document().body.measurement_run,
        second.document().body.measurement_run
    );
    assert_ne!(
        first.document().body.runner_instance_identity,
        second.document().body.runner_instance_identity
    );
    assert_eq!(first.document().identity().domain(), InstanceDomain::Record);
    assert_eq!(
        first.document().body.measurement_run.domain(),
        InstanceDomain::Run
    );
    assert_eq!(
        first.document().body.runner_instance_identity.domain(),
        LOCAL_RUNNER_DOMAIN
    );
    assert_eq!(first.document().body.planned_runner_class, None);
    let bytes = first.encode().unwrap();
    assert_eq!(&first.decode_checked(&bytes).unwrap(), first.document());
    assert!(second.decode_checked(&bytes).is_err());
    let mut cases = std::collections::BTreeSet::new();
    let mut locals = std::collections::BTreeSet::new();
    for (entry, projection) in first
        .document()
        .body
        .case_inventory
        .iter()
        .zip(first.projections())
    {
        assert_eq!(entry.case_identity, projection.identity().unwrap());
        assert!(cases.insert(entry.case_identity.clone()));
        assert!(locals.insert(entry.local_record_identity.clone()));
    }
}

#[test]
fn native_capture_uses_the_plan_already_issued_before_collection_and_retains_it_afterward() {
    let prepared = crate::benchmark::run::PreparedRun::acquire().unwrap();
    let bytes = prepared.common_plan().encode().unwrap();
    let identity = prepared.common_plan().document().identity().clone();
    let recorded = prepared.collect().unwrap();
    assert_eq!(recorded.plan_record().identity(), &identity);
    assert_eq!(recorded.common_plan().encode().unwrap(), bytes);
    let native = recorded
        .decode_checked(&recorded.encode().unwrap())
        .unwrap();
    let value = serde_json::to_value(native.document()).unwrap();
    assert_eq!(
        value["result"]["codec"],
        "intlify-config-owner-run-result/1"
    );
    assert_eq!(
        value["result"]["recordIdentity"]["domain"],
        "intlify-config-owner-result-v1"
    );
    assert_eq!(
        value["result"]["recordIdentity"],
        value["result"]["plan"]["resultIdentity"]
    );
    assert_eq!(
        value["result"]["plan"]["commonRunPlan"],
        serde_json::to_value(identity).unwrap()
    );
    assert_eq!(value["result"]["attempts"].as_array().unwrap().len(), 127);
}

#[test]
fn self_rehashed_changes_do_not_replace_the_plan_issued_for_the_acquired_context() {
    let issued = issue();
    let original = serde_json::to_value(issued.document()).unwrap();
    for (pointer, replacement) in [
        ("/envelope/recordIdentity/value", json!("0".repeat(64))),
        ("/envelope/producingTool/revision", json!("changed")),
        ("/envelope/governingSpecification/revision", json!("1")),
        (
            "/body/measurementRun/domain",
            json!("intlify-verification-record-v0"),
        ),
        ("/body/buildIdentity/checksum", json!("0".repeat(64))),
        ("/body/measurementProfile/revision", json!("1")),
        ("/body/runnerInstanceIdentity/value", json!("0".repeat(64))),
        (
            "/body/plannedRunnerClass",
            json!("invented-qualified-runner"),
        ),
        (
            "/body/caseInventory/0/caseIdentity",
            json!(format!("mc0_{}", "0".repeat(64))),
        ),
        (
            "/body/caseInventory/0/localRecordIdentity",
            json!("changed-local"),
        ),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        // The changed document still decodes as a Run Plan, so the rejection
        // below is admission's, not the decoder's.
        assert!(serde_json::from_value::<RunPlanRecord>(value.clone()).is_ok());
        assert_eq!(
            issued.decode_checked(&rehashed(value)),
            Err(PlanFailure::InvalidRecord),
            "{pointer}"
        );
    }
    for action in 0..3 {
        let mut changed = original.clone();
        let inventory = changed["body"]["caseInventory"].as_array_mut().unwrap();
        match action {
            0 => {
                inventory.pop();
            }
            1 => inventory[1] = inventory[0].clone(),
            _ => inventory.swap(0, 1),
        }
        assert!(issued.decode_checked(&rehashed(changed)).is_err());
    }
}

#[test]
fn malformed_missing_extra_or_duplicate_fields_and_size_overruns_are_rejected() {
    let issued = issue();
    let original = serde_json::to_value(issued.document()).unwrap();
    for path in [
        "",
        "/envelope",
        "/body",
        "/body/caseInventory/0",
        "/body/buildIdentity",
        "/body/measurementRun",
    ] {
        for field in original.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = original.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            assert!(
                issued
                    .decode_checked(&serde_json::to_vec(&missing).unwrap())
                    .is_err(),
                "{path}/{field}"
            );
        }
        let mut extra = original.clone();
        extra.pointer_mut(path).unwrap()["extra"] = json!(true);
        assert!(issued
            .decode_checked(&serde_json::to_vec(&extra).unwrap())
            .is_err());
    }
    let text = String::from_utf8(issued.encode().unwrap()).unwrap();
    let duplicate = text.replacen(
        "\"recordKind\":",
        "\"recordKind\":\"measurement-run-plan\",\"recordKind\":",
        1,
    );
    assert!(issued.decode_checked(duplicate.as_bytes()).is_err());
    let mut exact = issued.encode().unwrap();
    exact.resize(MAX_PLAN_BYTES, b' ');
    assert!(issued.decode_checked(&exact).is_ok());
    exact.push(b' ');
    assert_eq!(issued.decode_checked(&exact), Err(PlanFailure::SizeLimit));
}

#[test]
fn case_identity_preserves_canonical_object_order_and_rejects_unlisted_dimensions() {
    let issued = issue();
    let projection = &issued.projections()[0];
    let original = serde_json::to_value(projection).unwrap();
    let mut reversed = original.clone();
    let map = reversed.as_object_mut().unwrap();
    *map = std::mem::take(map).into_iter().rev().collect();
    let decoded: CaseProjection = serde_json::from_value(reversed).unwrap();
    assert_eq!(decoded.identity().unwrap(), projection.identity().unwrap());
    for field in [
        "implementationRevision",
        "branch",
        "path",
        "workerId",
        "measurementRun",
        "sampleValue",
        "expectedSemanticChecksum",
    ] {
        let mut changed = original.clone();
        changed[field] = json!("not-a-case-dimension");
        assert!(serde_json::from_value::<CaseProjection>(changed).is_err());
    }
    for (pointer, replacement) in [
        ("/ownerIdentity", json!("another-owner")),
        ("/ownerResultSchemaRevision", json!("2")),
        ("/ownerBenchmarkProfileRevision", json!("3")),
        ("/fixture/revision", json!("2")),
        ("/verificationSubject/identity", json!("another-subject")),
        ("/measurementProfileRevision", json!("3")),
        ("/workload/facts/0/observation/value", json!("1561")),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let changed: CaseProjection = serde_json::from_value(changed).unwrap();
        assert_ne!(
            changed.identity().unwrap(),
            projection.identity().unwrap(),
            "{pointer}"
        );
    }
}
