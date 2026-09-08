// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::{json, Value};

use super::catalog::{validate_records, ValidationFailure as V};
use super::owner::OwnerInput;
use super::*;
use crate::benchmark::run::PreparedRun;
use crate::benchmark::shared::identity::IntegrityDigest;
use crate::benchmark::shared::measurement::Outcome;
use crate::benchmark::shared::{decode, encoding};

fn capture() -> RecordedRun {
    PreparedRun::acquire().unwrap().collect().unwrap()
}

fn value(bytes: &[u8]) -> Value {
    serde_json::from_slice(bytes).unwrap()
}

fn reseal(value: &mut Value) -> Vec<u8> {
    value["envelope"]["integrityDigest"] = serde_json::to_value(IntegrityDigest::from_hash(
        encoding::record_hash(value).unwrap(),
    ))
    .unwrap();
    serde_json::to_vec(value).unwrap()
}

#[test]
fn actual_owner_run_projects_all_six_pairs_and_raw_samples_through_report_admission() {
    let run = capture();
    let artifacts = produce(&run).unwrap();
    let checked = validate(&run, &artifacts).unwrap();
    assert_eq!(checked.outcome, Outcome::Complete);
    assert_eq!(
        (
            checked.planned_cases,
            checked.measured_cases,
            checked.non_measured_cases
        ),
        (127, 127, 0)
    );
    let evidence = value(artifacts.evidence.as_ref().unwrap());
    let evaluation = value(&artifacts.evaluation);
    let report = value(&artifacts.report);
    let native = value(&artifacts.owner_result[0]);
    assert_eq!(
        evidence["body"]["ownerResult"]["resultIdentity"],
        native["result"]["recordIdentity"]
    );
    assert_eq!(
        evidence["body"]["ownerResult"]["checksum"],
        native["checksum"]
    );
    assert_eq!(
        evidence["body"]["ownerResult"]["resultSchema"],
        "intlify-config-owner-run-result/1"
    );
    assert_eq!(
        evaluation["body"]["ownerResultInput"]["result"]["kind"],
        "resolved"
    );
    let cases = evidence["body"]["cases"].as_array().unwrap();
    let rows = report["body"]["sections"][0]["rows"].as_array().unwrap();
    let mut pairs = std::collections::BTreeSet::new();
    for (index, (case, row)) in cases.iter().zip(rows).enumerate() {
        pairs.insert((
            row["ownerPhase"].as_str().unwrap(),
            row["ownerCost"].as_str().unwrap(),
        ));
        assert_eq!(row["caseIdentity"], case["caseIdentity"]);
        assert_eq!(case["samples"].as_array().unwrap().len(), 1);
        let projected = &case["samples"][0];
        let raw = &native["result"]["attempts"][index]["result"]["detail"]["operation"]
            ["operation"]["capture"]["samples"][0];
        assert_eq!(projected["aggregateQuantity"], raw["aggregateNanoseconds"]);
        assert_eq!(projected["repetitionCount"], raw["repetitionCount"]);
        assert_eq!(
            projected["executionIdentity"]["value"],
            raw["executionIdentity"]
        );
        assert_eq!(
            projected["semanticObservation"]["completeObservation"],
            raw["semanticObservation"]
        );
        assert_eq!(
            row["samples"][0]["aggregateQuantity"],
            projected["aggregateQuantity"]
        );
        assert_eq!(
            row["samples"][0]["repetitionCount"],
            projected["repetitionCount"]
        );
        assert_eq!(row["category"], "toolchain");
        assert_eq!(row["operationClass"], "component");
        assert_eq!(row["performanceSurface"], "core");
        assert_eq!(row["metric"], "wall_duration");
        assert_eq!(row["unit"], "nanosecond");
    }
    assert_eq!(pairs.len(), 6);
    assert_eq!(
        report["body"]["sections"][0]["numericPolicy"],
        "observational-only-no-numeric-decisions"
    );
    assert_eq!(
        report["body"]["sections"][0]["truncation"]["kind"],
        "complete"
    );
    let storage = tempfile::tempdir().unwrap();
    for (name, bytes) in [
        ("plan", &artifacts.run_plan),
        ("native", &artifacts.owner_result[0]),
        ("evidence", artifacts.evidence.as_ref().unwrap()),
        ("evaluation", &artifacts.evaluation),
        ("report", &artifacts.report),
    ] {
        let path = storage.path().join(format!("{name}.json"));
        std::fs::write(&path, bytes).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), *bytes);
    }
}

#[test]
fn missing_unsupported_malformed_and_duplicate_native_inputs_keep_the_complete_negative_inventory()
{
    let run = capture();
    let raw = run.encode().unwrap();
    let mut unknown = value(&raw);
    unknown["result"]["codec"] = json!("intlify-config-owner-run-result/99");
    let unknown = serde_json::to_vec(&unknown).unwrap();
    let scenarios: Vec<(Vec<&[u8]>, Outcome, &str)> = vec![
        (vec![], Outcome::Incomplete, "unavailable"),
        (vec![&unknown], Outcome::Incomplete, "unavailable"),
        (vec![b"{"], Outcome::Invalid, "invalid"),
        (vec![&raw, &raw], Outcome::Invalid, "invalid"),
    ];
    for (inputs, outcome, state) in scenarios {
        let artifacts = produce_with_owner(&run, &inputs).unwrap();
        let admitted = validate(&run, &artifacts).unwrap();
        assert_eq!(admitted.outcome, outcome);
        assert_eq!(
            (admitted.measured_cases, admitted.non_measured_cases),
            (0, 127)
        );
        assert!(artifacts.evidence.is_none());
        let evaluation = value(&artifacts.evaluation);
        assert_eq!(
            evaluation["body"]["ownerResultInput"]["result"]["kind"],
            state
        );
        assert!(!evaluation["body"]["reasons"].as_array().unwrap().is_empty());
        assert_eq!(
            evaluation["body"]["caseInventory"]
                .as_array()
                .unwrap()
                .len(),
            127
        );
        let report = value(&artifacts.report);
        assert_eq!(report["body"]["sections"][0]["rows"], json!([]));
        assert_eq!(
            report["body"]["sections"][0]["missingCaseInventory"]
                .as_array()
                .unwrap()
                .len(),
            127
        );
    }
}

#[test]
fn self_rehashed_common_content_never_replaces_the_independently_acquired_owner_facts() {
    let run = capture();
    let artifacts = produce(&run).unwrap();
    for (pointer, replacement) in [
        (
            "/body/cases/0/samples/0/aggregateQuantity",
            json!("18446744073709551615"),
        ),
        ("/body/cases/0/samples/0/repetitionCount", json!("2")),
        ("/body/cases/0/samples/0/ordinal", json!("1")),
        (
            "/body/build/sourceControlState",
            json!({"kind":"observed","value":"clean"}),
        ),
        ("/body/binding/measurementRun/value", json!("0".repeat(64))),
        (
            "/body/cases/0/ownerAttempt/reference/localRecordIdentity",
            json!("attempt-999"),
        ),
    ] {
        let mut changed = artifacts.clone();
        let mut evidence = value(changed.evidence.as_ref().unwrap());
        *evidence.pointer_mut(pointer).expect(pointer) = replacement;
        changed.evidence = Some(reseal(&mut evidence));
        assert!(validate(&run, &changed).is_err(), "{pointer}");
    }
    for (pointer, replacement) in [
        (
            "/body/sections/0/rows/0/samples/0/aggregateQuantity",
            json!("18446744073709551615"),
        ),
        ("/body/sections/0/rows/0/unit", json!("octet")),
        ("/body/sections/0/rows/0/operationClass", json!("workflow")),
        ("/body/sections/0/numericPolicy", json!("gating")),
        ("/body/sections/0/truncation", json!({"kind":"truncated"})),
    ] {
        let mut changed = artifacts.clone();
        let mut report = value(&changed.report);
        *report.pointer_mut(pointer).expect(pointer) = replacement;
        changed.report = reseal(&mut report);
        assert!(validate(&run, &changed).is_err(), "{pointer}");
    }
}

#[test]
fn withholding_eligible_evidence_cannot_manufacture_projection_ineligibility() {
    let run = capture();
    let mut artifacts = produce(&run).unwrap();
    let native = run.encode().unwrap();
    let owner = OwnerInput::resolve(&run, &[&native]);
    let evaluation = Evaluation::seal(
        EvaluationKind::Value,
        RecordIdentity::fresh(InstanceDomain::Record).unwrap(),
        evaluate::evaluate(&run, &owner, None).unwrap(),
    )
    .unwrap();
    let report = Report::seal(
        ReportKind::Value,
        RecordIdentity::fresh(InstanceDomain::Record).unwrap(),
        evaluate::report(&evaluation, None).unwrap(),
    )
    .unwrap();
    artifacts.evidence = None;
    artifacts.evaluation = encode(&evaluation).unwrap();
    artifacts.report = encode(&report).unwrap();
    artifacts.report_identity = report.identity().clone();
    assert_eq!(validate(&run, &artifacts).unwrap_err(), V::MissingRecord);
}

#[test]
fn finite_catalog_rejects_duplicates_conflicts_and_unsupported_tuples_without_input_order_selection(
) {
    let run = capture();
    let artifacts = produce(&run).unwrap();
    let owner = [artifacts.owner_result[0].as_slice()];
    let mut records = vec![
        artifacts.run_plan.as_slice(),
        artifacts.evidence.as_ref().unwrap(),
        &artifacts.evaluation,
        &artifacts.report,
    ];
    records.reverse();
    assert_eq!(
        validate_records(&run, &owner, &records, &artifacts.report_identity).unwrap(),
        validate(&run, &artifacts).unwrap()
    );
    records.push(&artifacts.report);
    assert_eq!(
        validate_records(&run, &owner, &records, &artifacts.report_identity).unwrap_err(),
        V::DuplicateRecord
    );
    records.pop();
    let mut conflict = value(&artifacts.report);
    conflict["envelope"]["creationContext"] = json!({"createdAtUnixNanoseconds":"0"});
    let conflict = reseal(&mut conflict);
    records.push(&conflict);
    assert_eq!(
        validate_records(&run, &owner, &records, &artifacts.report_identity).unwrap_err(),
        V::IdentityConflict
    );
    let mut future = value(&artifacts.report);
    future["envelope"]["recordSchemaRevision"] = json!("99");
    future["body"] = json!({"unknownFutureNumbers": 42});
    let future = serde_json::to_vec(&future).unwrap();
    assert_eq!(
        validate_records(&run, &owner, &[&future], &artifacts.report_identity).unwrap_err(),
        V::Decode(decode::DecodeFailure::Unsupported)
    );
}

#[test]
fn complete_schemas_are_fresh_and_agree_with_the_independent_draft7_oracle() {
    use crate::schema;
    let run = capture();
    let artifacts = produce(&run).unwrap();
    let negative = produce_with_owner(&run, &[]).unwrap();
    let invalid = produce_with_owner(&run, &[b"{"]).unwrap();
    let schemas = [
        (
            schema::measurement_evidence_set_schema().unwrap(),
            include_str!("../../../../schema/measurement-evidence-set-v0.schema.json"),
            vec![artifacts.evidence.as_ref().unwrap().as_slice()],
        ),
        (
            schema::measurement_run_evaluation_schema().unwrap(),
            include_str!("../../../../schema/measurement-run-evaluation-v0.schema.json"),
            vec![
                artifacts.evaluation.as_slice(),
                &negative.evaluation,
                &invalid.evaluation,
            ],
        ),
        (
            schema::measurement_structured_report_schema().unwrap(),
            include_str!("../../../../schema/measurement-structured-report-v0.schema.json"),
            vec![
                artifacts.report.as_slice(),
                &negative.report,
                &invalid.report,
            ],
        ),
    ];
    for (generated, checked_in, records) in schemas {
        assert_eq!(
            schema::format_schema(generated.clone()).unwrap(),
            checked_in
        );
        let oracle = jsonschema::draft7::new(&generated).unwrap();
        for bytes in records {
            let record = value(bytes);
            let issues = oracle
                .iter_errors(&record)
                .map(|error| error.to_string())
                .collect::<Vec<_>>();
            assert!(issues.is_empty(), "{issues:?}");
            for mutate in [
                |v: &mut Value| {
                    v["envelope"]["creationContext"] = Value::Null;
                },
                |v: &mut Value| {
                    v["envelope"]["recordSchemaRevision"] = json!(0);
                },
                |v: &mut Value| {
                    v["body"]["unexpected"] = json!(true);
                },
                |v: &mut Value| {
                    v["envelope"]
                        .as_object_mut()
                        .unwrap()
                        .remove("producingTool");
                },
            ] {
                let mut bad = record.clone();
                mutate(&mut bad);
                assert!(!oracle.is_valid(&bad));
            }
        }
    }
}

#[test]
fn all_environment_fields_are_ordered_typed_and_not_upgraded_from_hints_or_unknown_facts() {
    use crate::benchmark::shared::reason::EnvironmentField;
    let run = capture();
    let artifacts = produce(&run).unwrap();
    let evidence = value(artifacts.evidence.as_ref().unwrap());
    let fields = evidence["body"]["environment"]["fields"]
        .as_array()
        .unwrap();
    assert_eq!(fields.len(), 27);
    for (entry, expected) in fields.iter().zip(EnvironmentField::ALL) {
        assert_eq!(entry["observation"]["field"], expected.name());
    }
    for field in [
        "kernel_build",
        "logical_cpu_count",
        "build_configuration",
        "locale_service",
        "execution_kind",
        "container_emulator_simulator",
    ] {
        let entry = fields
            .iter()
            .find(|entry| entry["observation"]["field"] == field)
            .unwrap();
        assert_eq!(entry["observation"]["state"]["kind"], "unavailable");
        assert!(!entry["observation"]["state"]["reasons"]
            .as_array()
            .unwrap()
            .is_empty());
    }
    for field in [
        "language_runtime",
        "browser",
        "virtual_machine",
        "jit_gc_configuration",
        "memory_observer",
    ] {
        let entry = fields
            .iter()
            .find(|entry| entry["observation"]["field"] == field)
            .unwrap();
        assert_eq!(entry["observation"]["state"]["kind"], "not-applicable");
        assert!(entry["observation"]["state"]["applicabilityRule"].is_object());
    }
    let schema = crate::schema::measurement_evidence_set_schema().unwrap();
    let oracle = jsonschema::draft7::new(&schema).unwrap();
    for mutate in [
        |v: &mut Value| {
            v["body"]["environment"]["fields"]
                .as_array_mut()
                .unwrap()
                .pop();
        },
        |v: &mut Value| {
            v["body"]["environment"]["fields"][1] = v["body"]["environment"]["fields"][0].clone();
        },
        |v: &mut Value| {
            v["body"]["environment"]["fields"]
                .as_array_mut()
                .unwrap()
                .swap(0, 1);
        },
        |v: &mut Value| {
            v["body"]["environment"]["fields"][0]["observation"]["field"] = json!("hostname");
        },
        |v: &mut Value| {
            v["body"]["environment"]["fields"][5]["observation"]["state"] =
                json!({"kind":"unavailable","reasons":[]});
        },
        |v: &mut Value| {
            v["body"]["environment"]["fields"][8]["observation"]["state"] =
                json!({"kind":"observed","value":"0"});
        },
    ] {
        let mut changed = artifacts.clone();
        let mut bad = evidence.clone();
        mutate(&mut bad);
        assert!(!oracle.is_valid(&bad));
        changed.evidence = Some(reseal(&mut bad));
        assert!(validate(&run, &changed).is_err());
    }
    // Syntactically valid but unproven non-applicability remains inadmissible.
    let mut bad = evidence;
    bad["body"]["environment"]["fields"][23]["observation"]["state"] = json!({
        "kind":"not-applicable", "applicabilityRule":{"identity":"made-up-rule","revision":"0"}
    });
    let mut changed = artifacts;
    changed.evidence = Some(reseal(&mut bad));
    assert!(validate(&run, &changed).is_err());
}

#[test]
fn complete_inventory_and_exact_nested_references_cannot_be_rewritten_into_success() {
    let run = capture();
    let artifacts = produce(&run).unwrap();
    for mutate in [
        |v: &mut Value| {
            v["body"]["cases"].as_array_mut().unwrap().pop();
        },
        |v: &mut Value| {
            v["body"]["cases"].as_array_mut().unwrap().swap(0, 1);
        },
        |v: &mut Value| {
            v["body"]["cases"][1] = v["body"]["cases"][0].clone();
        },
        |v: &mut Value| {
            v["body"]["cases"][0]["caseIdentity"] = json!(format!("mc0_{}", "0".repeat(64)));
        },
        |v: &mut Value| {
            v["body"]["cases"][0]["result"] = json!({"kind":"not-applicable","applicabilityRule":{"identity":"unproven","revision":"0"}});
        },
        |v: &mut Value| {
            v["body"]["cases"][0]["result"]["evidence"]["reference"]["localRecordIdentity"] =
                json!("measurement-case-1");
        },
        |v: &mut Value| {
            v["body"]["caseInventory"][0]["requirement"] = json!("optional");
        },
        |v: &mut Value| {
            v["body"]["cases"][0]["localRecordIdentity"] =
                v["body"]["caseInventory"][0]["localRecordIdentity"].clone();
        },
    ] {
        let mut changed = artifacts.clone();
        let mut evaluation = value(&changed.evaluation);
        mutate(&mut evaluation);
        changed.evaluation = reseal(&mut evaluation);
        assert!(validate(&run, &changed).is_err());
    }
    let mut changed = artifacts.clone();
    let negative = produce_with_owner(&run, &[]).unwrap();
    changed.evaluation = negative.evaluation;
    changed.report = negative.report;
    changed.report_identity = negative.report_identity;
    assert!(validate(&run, &changed).is_err());
    let another = capture();
    assert!(validate(&another, &artifacts).is_err());
}

#[test]
fn new_record_instances_preserve_semantics_without_retiming_or_copying_issuer_bytes() {
    let run = capture();
    let first = produce(&run).unwrap();
    let second = produce(&run).unwrap();
    assert_eq!(first.run_plan, second.run_plan);
    assert_eq!(first.owner_result, second.owner_result);
    assert_ne!(first.report_identity, second.report_identity);
    assert_eq!(
        validate(&run, &first).unwrap(),
        validate(&run, &second).unwrap()
    );
    let left = value(first.evidence.as_ref().unwrap());
    let right = value(second.evidence.as_ref().unwrap());
    assert_ne!(
        left["envelope"]["recordIdentity"],
        right["envelope"]["recordIdentity"]
    );
    assert_eq!(left["body"]["cases"], right["body"]["cases"]);
    assert_eq!(left["body"]["ownerResult"], right["body"]["ownerResult"]);
    // Formatting and object-member order are not new semantic observations.
    let mut pretty = first.clone();
    pretty.report = serde_json::to_vec_pretty(&value(&first.report)).unwrap();
    assert_eq!(
        validate(&run, &pretty).unwrap(),
        validate(&run, &first).unwrap()
    );
}
