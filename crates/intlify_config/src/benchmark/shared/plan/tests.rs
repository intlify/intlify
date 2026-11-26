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
    // Each decoder requires a member, null included, exactly when its schema
    // does. The identity input is written, never read back; what a reader
    // decodes is the projection inside it, and a projection whose member paths
    // all occur in an earlier one has none left to try.
    let decodes = |left_out| serde_json::from_value::<RunPlanRecord>(left_out).is_ok();
    assert_eq!(
        crate::schema::omission_disagreements(&value, "", &plan_validator, decodes),
        Vec::<String>::new()
    );
    let decodes = |left_out: Value| {
        serde_json::from_value::<CaseProjection>(left_out["projection"].clone()).is_ok()
    };
    let mut seen = std::collections::BTreeSet::new();
    for projection in plan.projections() {
        let input = case_identity_input(projection).unwrap();
        let paths = crate::schema::member_paths(&input, "/projection");
        if paths.keys().all(|path| seen.contains(path)) {
            continue;
        }
        seen.extend(paths.into_keys());
        assert_eq!(
            crate::schema::omission_disagreements(&input, "/projection", &case_validator, decodes),
            Vec::<String>::new()
        );
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
fn each_case_projection_carries_the_execution_state_its_prepared_case_runs_in() {
    let registry = Registry::load().unwrap();
    let plan = issue();
    let (mut engines, mut resolvers) = (Vec::new(), Vec::new());
    for (projection, declaration) in plan.projections().iter().zip(registry.declarations()) {
        let fixture = registry.prepare(declaration).unwrap();
        let prepared = fixture.prepared();
        assert_eq!(
            projection.execution_state,
            prepared_core(
                prepared.operation(),
                prepared.icu4x_engine(),
                prepared.resolver_state()
            )
        );
        if prepared.icu4x_engine().is_some() {
            engines.push(projection.execution_state.engine_state.as_str());
        }
        // A complete invocation's variant names the state of the resolver
        // its preparation invokes, which sets its engine state.
        assert_eq!(projection.variant.resolver, prepared.resolver_state());
        if let Some(state) = projection.variant.resolver {
            resolvers.push((state, projection.execution_state.engine_state.as_str()));
        }
    }
    assert_eq!(engines, ["reused", "fresh"].repeat(16));
    let states = [
        (ResolverState::Reused, "reused"),
        (ResolverState::Fresh, "fresh"),
        (ResolverState::ReusedAfterBlocked, "reused"),
    ];
    // Each file recipe's three cases, then each programmatic recipe's.
    let file = states.map(|state| [state; 6]).concat();
    let programmatic = states.map(|state| [state; 5]).concat();
    assert_eq!(resolvers, [file, programmatic].concat());
}

#[test]
fn issuance_freezes_unique_case_inventory_but_fresh_instances_do_not_change_case_identity() {
    let first = issue();
    let second = issue();
    assert_eq!(first.document().body.case_inventory.len(), 490);
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
        "intlify-config-owner-run-result/0"
    );
    assert_eq!(
        value["result"]["recordIdentity"]["domain"],
        "intlify-config-owner-result-v0"
    );
    assert_eq!(
        value["result"]["recordIdentity"],
        value["result"]["plan"]["resultIdentity"]
    );
    assert_eq!(
        value["result"]["plan"]["commonRunPlan"],
        serde_json::to_value(identity).unwrap()
    );
    assert_eq!(value["result"]["attempts"].as_array().unwrap().len(), 490);
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

#[test]
fn every_projection_declares_the_common_dimensions_it_serializes() {
    let plan = issue();
    for projection in plan.projections() {
        let common = CommonProjection::common(projection);
        assert_eq!(common.eligibility(), Ok(()));
        assert_eq!(
            (common.category, common.operation_class, common.surface),
            grouping(&projection.owner_phase)
        );
        let value = serde_json::to_value(projection).unwrap();
        for (field, declared) in [
            ("category", serde_json::to_value(common.category)),
            (
                "operationClass",
                serde_json::to_value(common.operation_class),
            ),
            ("performanceSurface", serde_json::to_value(common.surface)),
            ("metric", serde_json::to_value(common.metric)),
            (
                "sampleAggregation",
                serde_json::to_value(common.aggregation),
            ),
        ] {
            assert_eq!(value[field], declared.unwrap(), "{field}");
        }
        // A dimension that does not apply to a duration case is absent.
        assert_eq!(common.memory_domain, None);
        assert!(value.get("memoryObservationDomain").is_none());
        // Only an Evidence stage's case, whose cost both entry kinds have,
        // declares its entry kind (D15:2797; plan 025 decision #255).
        let evidence = projection.owner_phase == "profile_evidence_materialize";
        let programmatic = value["variant"]["recipe"]
            .get("programmatic-evidence")
            .is_some();
        let entry = if programmatic { "programmatic" } else { "file" };
        assert_eq!(
            value["variant"].get("entryKind"),
            evidence.then(|| json!(entry)).as_ref()
        );
        // Only a complete invocation's case, which runs on a resolver in a
        // declared state, names that state, as its recipe does (plan 025
        // decision #33); a replay's resolver is no variant of its own
        // (D15:2792).
        let replay = projection.owner_cost == "materialized_reresolution";
        let e2e = projection.owner_phase == "profile_resolve_e2e" && !replay;
        let resolver = value["variant"].get("resolver");
        assert_eq!(resolver.is_some(), e2e);
        if let Some(resolver) = resolver {
            let recipe = &value["variant"]["recipe"];
            let workflow = recipe
                .get("workflow")
                .or(recipe.get("programmatic-workflow"));
            assert_eq!(Some(resolver), workflow.map(|recipe| &recipe["resolver"]));
        }
    }
}

/// A memory case of a reused resolver, made from one actual projection.
fn memory_case(mut projection: CaseProjection) -> CaseProjection {
    projection.category = Category::Memory;
    projection.operation_class = OperationClass::PeakLive;
    projection.memory_observation_domain = Some(crate::benchmark::memory_domain::invocation(
        ResolverState::Reused,
    ));
    projection.variant.entry_kind = Some(EntryKind::File);
    projection.variant.resolver = Some(ResolverState::Reused);
    projection.metric = Metric::PeakLiveBytes;
    projection.sample_aggregation = Aggregation::PeakOverBatch;
    projection
}

#[test]
fn a_dimension_that_applies_is_present_and_one_that_does_not_is_never_null() {
    let plan = issue();
    let duration = plan.projections()[0].clone();
    let memory = memory_case(duration.clone());
    let common = CommonProjection::common(&memory);
    assert_eq!(common.eligibility(), Ok(()));
    // 015's domain states the reuse state the case's Execution State has.
    assert!(crate::benchmark::memory_domain::agrees(
        common.memory_domain.unwrap(),
        &memory.execution_state
    ));
    let value = serde_json::to_value(&memory).unwrap();
    assert_eq!(value["variant"]["entryKind"], "file");
    assert_eq!(value["variant"]["resolver"], "reused");
    assert_eq!(
        value["memoryObservationDomain"]["identity"],
        json!({"identity": "profile-resolver-invocation", "revision": "0"})
    );
    assert_eq!(
        serde_json::from_value::<CaseProjection>(value.clone()).unwrap(),
        memory
    );
    // The committed case schema admits the memory case too.
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../schema/measurement-case-identity-v0.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::draft7::new(&schema).unwrap();
    let input = case_identity_input(&memory).unwrap();
    assert!(
        validator.is_valid(&input),
        "{:?}",
        validator.iter_errors(&input).collect::<Vec<_>>()
    );
    // An explicit null is refused by the decoder and by the schema alike.
    for pointer in [
        "/memoryObservationDomain",
        "/variant/entryKind",
        "/variant/resolver",
    ] {
        let mut nulled = value.clone();
        *nulled.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(
            serde_json::from_value::<CaseProjection>(nulled.clone()).is_err(),
            "{pointer}"
        );
        let mut input = input.clone();
        input["projection"] = nulled;
        assert!(!validator.is_valid(&input), "{pointer}");
    }
    // The memory case is another case than the duration case it came from.
    assert_ne!(memory.identity().unwrap(), duration.identity().unwrap());
}

#[test]
fn a_memory_run_projects_each_construction_and_invocation_case_as_its_peak() {
    use crate::benchmark::instrument::tests::{ScriptedObserver, DESCRIPTION};
    use crate::benchmark::memory_domain;
    use crate::benchmark::operation::Operation;
    const CONSTRUCTION: Operation = Operation::ConstructionAuthorityAdmission;
    const FILE: Operation = Operation::FileEntryCompleteWorkflow;
    const PROGRAMMATIC: Operation = Operation::ProgrammaticEntryCompleteWorkflow;

    let registry = Registry::load().unwrap();
    let context =
        CaptureContext::acquire_memory(&registry, Box::new(ScriptedObserver::new([])), DESCRIPTION)
            .unwrap();
    let memory = super::issue(&registry, &context).unwrap();
    let duration = issue();
    let twins = duration
        .projections()
        .iter()
        .filter_map(|projection| {
            [CONSTRUCTION, FILE, PROGRAMMATIC]
                .into_iter()
                .find(|operation| operation.cost() == projection.phase_cost().1)
                .map(|operation| (projection, operation))
        })
        .collect::<Vec<_>>();
    assert_eq!((memory.projections().len(), twins.len()), (73, 73));
    let cases = |wanted| twins.iter().filter(|(_, each)| *each == wanted).count();
    assert_eq!([CONSTRUCTION, FILE, PROGRAMMATIC].map(cases), [40, 18, 15]);
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../schema/measurement-case-identity-v0.schema.json"
    ))
    .unwrap();
    let validator = jsonschema::draft7::new(&schema).unwrap();
    for (memory, (duration, operation)) in memory.projections().iter().zip(twins) {
        let common = CommonProjection::common(memory);
        assert_eq!(common.eligibility(), Ok(()));
        let (cost, domain, entry) = match (operation, duration.variant.resolver) {
            (CONSTRUCTION, None) => (
                "construction_peak_live_memory",
                memory_domain::construction(),
                None,
            ),
            (FILE, Some(state)) => (
                "invocation_peak_live_memory",
                memory_domain::invocation(state),
                Some(EntryKind::File),
            ),
            (PROGRAMMATIC, Some(state)) => (
                "invocation_peak_live_memory",
                memory_domain::invocation(state),
                Some(EntryKind::Programmatic),
            ),
            other => panic!("{other:?}"),
        };
        assert_eq!(memory.phase_cost(), ("profile_resolve_peak_memory", cost));
        assert_eq!(common.category, Category::Memory);
        assert_eq!(common.operation_class, OperationClass::PeakLive);
        assert_eq!(common.surface, Surface::Core);
        assert_eq!(common.metric, Metric::PeakLiveBytes);
        assert_eq!(common.aggregation, Aggregation::PeakOverBatch);
        assert_eq!(common.memory_domain, Some(&domain));
        assert!(memory_domain::agrees(
            common.memory_domain.unwrap(),
            &memory.execution_state
        ));
        assert_eq!(
            memory.measurement_method,
            Method::observed_peak(DESCRIPTION)
        );
        assert_eq!(
            Some(&memory.interval_boundary),
            Boundary::for_memory(operation).as_ref()
        );
        // The fixture, workload, and execution state are the duration
        // case's, its variant is too but for the entry kind only the memory
        // cost leaves unnamed (D15:2797), and the memory case is another
        // case.
        assert_eq!(memory.variant.entry_kind, entry);
        assert_eq!(duration.variant.entry_kind, None);
        let variant = Variant {
            entry_kind: None,
            ..memory.variant.clone()
        };
        assert_eq!(
            (
                &memory.fixture,
                &variant,
                &memory.workload,
                &memory.execution_state
            ),
            (
                &duration.fixture,
                &duration.variant,
                &duration.workload,
                &duration.execution_state
            )
        );
        assert_ne!(memory.identity().unwrap(), duration.identity().unwrap());
        let input = case_identity_input(memory).unwrap();
        assert!(
            validator.is_valid(&input),
            "{:?}",
            validator.iter_errors(&input).collect::<Vec<_>>()
        );
    }
}

type Grouping = (Category, OperationClass, Surface);
const COMPONENT: Grouping = (
    Category::Toolchain,
    OperationClass::Component,
    Surface::Core,
);
const WORKFLOW: Grouping = (
    Category::Toolchain,
    OperationClass::Workflow,
    Surface::ProductWorkflow,
);
const PEAK_LIVE: Grouping = (Category::Memory, OperationClass::PeakLive, Surface::Core);

/// Design 026's minimum projection mappings of design 015's phases, row by
/// row (D26:2804-2813).
const MAPPINGS: [(&str, Grouping); 10] = [
    ("profile_resolver_construct", COMPONENT),
    ("profile_entry_materialize", COMPONENT),
    ("profile_structural_admit", COMPONENT),
    ("profile_select", COMPONENT),
    ("profile_locale_resolve", COMPONENT),
    ("profile_artifact_admit", COMPONENT),
    ("profile_target_group_resolve", COMPONENT),
    ("profile_evidence_materialize", COMPONENT),
    ("profile_resolve_e2e", WORKFLOW),
    ("profile_resolve_peak_memory", PEAK_LIVE),
];

#[test]
fn each_phase_is_grouped_as_026_maps_it() {
    use crate::benchmark::operation::Operation;
    for (phase, grouped) in MAPPINGS {
        assert_eq!(grouping(phase), grouped, "{phase}");
    }
    // Every phase this owner measures, by duration or by memory, is one of
    // the table's.
    let listed = |phase: &str| MAPPINGS.iter().any(|row| row.0 == phase);
    for operation in Operation::ALL {
        assert!(listed(operation.phase()), "{operation:?}");
        if let Some(memory) = operation.peak_memory() {
            assert!(listed(memory.phase), "{operation:?}");
        }
    }
}

/// One actual projection, grouped as a complete invocation's cost.
fn workflow_case(mut projection: CaseProjection) -> CaseProjection {
    (
        projection.category,
        projection.operation_class,
        projection.performance_surface,
    ) = grouping("profile_resolve_e2e");
    projection
}

#[test]
fn a_workflow_case_on_the_product_workflow_surface_is_admitted() {
    let plan = issue();
    let component = plan.projections()[0].clone();
    let workflow = workflow_case(component.clone());
    assert_eq!(CommonProjection::common(&workflow).eligibility(), Ok(()));
    let value = serde_json::to_value(&workflow).unwrap();
    assert_eq!(value["category"], "toolchain");
    assert_eq!(value["operationClass"], "workflow");
    assert_eq!(value["performanceSurface"], "product_workflow");
    assert_eq!(
        serde_json::from_value::<CaseProjection>(value).unwrap(),
        workflow
    );
    // The committed case schema admits it, and it is another case than the
    // component it was made from.
    let case_text = include_str!("../../../../schema/measurement-case-identity-v0.schema.json");
    let schema: Value = serde_json::from_str(case_text).unwrap();
    let validator = jsonschema::draft7::new(&schema).unwrap();
    let input = case_identity_input(&workflow).unwrap();
    assert!(
        validator.is_valid(&input),
        "{:?}",
        validator.iter_errors(&input).collect::<Vec<_>>()
    );
    assert_ne!(workflow.identity().unwrap(), component.identity().unwrap());
    // Each committed schema that carries the dimensions lists exactly the
    // classes and surfaces revision "0" admits.
    for text in [
        case_text,
        include_str!("../../../../schema/measurement-evidence-set-v0.schema.json"),
        include_str!("../../../../schema/measurement-structured-report-v0.schema.json"),
    ] {
        let schema: Value = serde_json::from_str(text).unwrap();
        assert_eq!(
            schema["definitions"]["OperationClass"]["enum"],
            json!(["component", "workflow", "peak_live"])
        );
        assert_eq!(
            schema["definitions"]["Surface"]["enum"],
            json!(["core", "product_workflow"])
        );
    }
}

#[test]
fn a_memory_case_is_on_the_surface_of_its_pair_not_of_its_duration() {
    use crate::benchmark::instrument::tests::DESCRIPTION;
    use crate::benchmark::operation::Operation;
    let operation = Operation::ConstructionAuthorityAdmission;
    let memory = CaseProjection::memory(
        workflow_case(issue().projections()[0].clone()),
        operation.peak_memory().unwrap(),
        Boundary::for_memory(operation).unwrap(),
        Method::observed_peak(DESCRIPTION),
    );
    // Memory is not a surface: the case keeps the one 026 maps its pair to.
    let common = CommonProjection::common(&memory);
    assert_eq!(
        (common.category, common.operation_class, common.surface),
        PEAK_LIVE
    );
    assert_eq!(common.eligibility(), Ok(()));
}
