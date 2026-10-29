// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use serde_json::{json, Value};

use crate::benchmark::operation::tests::operations;

use super::super::quantity::Quantity;
use super::*;

fn prepared(operation: Operation) -> Prepared {
    operations()
        .into_iter()
        .find(|prepared| prepared.operation() == operation)
        .unwrap()
}

fn clock(resolution: u64) -> ClockDescription {
    ClockDescription {
        provider: "rustix",
        provider_revision: intlify_measurement::acquisition::PROVIDER_REVISION,
        clock: "posix-clock-monotonic",
        resolution_nanoseconds: Quantity::new(resolution),
        resolution_source: "clock-getres-reported-granularity",
        conversion: "exact-integer-nanoseconds",
    }
}

#[test]
fn all_active_pairs_keep_distinct_owner_boundaries_and_exact_method_meaning() {
    let mut pairs = BTreeSet::new();
    let mut identities = BTreeSet::new();
    for operation in &operations() {
        let descriptors = Descriptors::for_acquisition(operation, clock(100));
        assert!(descriptors.validate(operation, clock(100)).is_empty());
        let wire = serde_json::to_vec(&descriptors).unwrap();
        let decoded: Descriptors = serde_json::from_slice(&wire).unwrap();
        assert_eq!(decoded, descriptors);
        assert!(decoded.validate(operation, clock(100)).is_empty());
        let boundary = &decoded.boundary;
        assert!(pairs.insert((boundary.phase.clone(), boundary.cost.clone())));
        assert!(identities.insert(boundary.identity.clone()));
        assert_eq!(boundary.occurrence_policy, "single");
        assert_eq!(
            boundary.included_markers.first(),
            Some(&boundary.first_included_marker)
        );
        assert_eq!(
            boundary.included_markers.last(),
            Some(&boundary.final_included_marker)
        );
        assert!(boundary.direct_parents.is_empty());
        assert!(boundary.direct_children.is_empty());
        assert_eq!(decoded.method.metric, "wall_duration");
        assert_eq!(decoded.method.canonical_unit, "nanosecond");
        assert_eq!(decoded.method.sample_aggregation_kind, "batch_total");
        assert_eq!(
            decoded.method.numeric_decision_eligibility,
            "qualified-runner"
        );
        assert!(!decoded.method.estimated_overhead_subtraction);
    }
    assert_eq!(pairs.len(), 11);
    assert_eq!(identities.len(), 11);
}

#[test]
fn wrong_operation_and_mutated_marker_order_or_nesting_are_rejected() {
    let operation = &prepared(Operation::FileMaterialization);
    let valid = Descriptors::for_acquisition(operation, clock(10));
    assert_eq!(
        valid.validate(&prepared(Operation::StructuralAnalysis), clock(10)),
        vec![DescriptorIssue::BoundaryMismatch]
    );
    let mut reordered = valid.clone();
    reordered.boundary.included_markers.reverse();
    assert_eq!(
        reordered.validate(operation, clock(10)),
        vec![DescriptorIssue::BoundaryMismatch]
    );
    let mut nested = valid;
    nested
        .boundary
        .direct_parents
        .push("invented-workflow".into());
    assert_eq!(
        nested.validate(operation, clock(10)),
        vec![DescriptorIssue::BoundaryMismatch]
    );
}

#[test]
fn every_descriptor_leaf_is_validated_not_just_identity_and_revision() {
    for operation in &operations() {
        let descriptors = Descriptors::for_acquisition(operation, clock(17));
        let original = serde_json::to_value(descriptors).unwrap();
        let mut paths = Vec::new();
        leaf_paths(&original, "", &mut paths);
        assert!(paths.len() > 50, "include every policy and placement field");
        for path in paths {
            let mut tampered = original.clone();
            let value = tampered.pointer_mut(&path).unwrap();
            *value = match value {
                Value::Bool(value) => Value::Bool(!*value),
                _ => json!("__unexpected_descriptor_value"),
            };
            // Decoding or validation against separate acquisition must reject.
            if let Ok(decoded) = serde_json::from_value::<Descriptors>(tampered) {
                assert!(
                    !decoded.validate(operation, clock(17)).is_empty(),
                    "unvalidated field {path} for {:?}",
                    operation.operation()
                );
            }
        }
    }
}

fn leaf_paths(value: &Value, path: &str, output: &mut Vec<String>) {
    match value {
        Value::Object(object) => {
            for (key, value) in object {
                // All owner descriptor field names are fixed ASCII, not input
                // identifiers requiring pointer escaping.
                leaf_paths(value, &format!("{path}/{key}"), output);
            }
        }
        Value::Array(array) if !array.is_empty() => {
            for (index, value) in array.iter().enumerate() {
                leaf_paths(value, &format!("{path}/{index}"), output);
            }
        }
        _ => output.push(path.into()),
    }
}

#[test]
fn missing_or_unknown_fields_cannot_supply_implicit_method_defaults() {
    for operation in &operations() {
        let original =
            serde_json::to_value(Descriptors::for_acquisition(operation, clock(1))).unwrap();
        let mut paths = vec![
            "",
            "/boundary",
            "/method",
            "/method/optimizationBarrier",
            "/clockObservation",
            "/execution",
            "/execution/outputBufferState",
        ];
        if matches!(
            operation.operation(),
            Operation::LocaleCanonicalization | Operation::LocalePolicyResolution
        ) {
            paths.extend([
                "/localeInput",
                "/localeInput/specification",
                "/localeInput/dataset",
                "/localeInput/provider",
                "/localeInput/providerSchema",
            ]);
        }
        if operation.operation() == Operation::LocalePolicyResolution {
            paths.push("/localePolicyResolutionInput");
        }
        for path in paths {
            let mut extra = original.clone();
            extra
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .insert("unknown".into(), json!(true));
            assert!(
                serde_json::from_value::<Descriptors>(extra).is_err(),
                "{path}"
            );
            for key in original.pointer(path).unwrap().as_object().unwrap().keys() {
                let mut missing = original.clone();
                missing
                    .pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(key);
                assert!(
                    serde_json::from_value::<Descriptors>(missing).is_err(),
                    "{path}/{key}"
                );
            }
        }
    }
}

#[test]
fn resolution_is_bound_to_the_actual_acquisition_not_storage_precision() {
    let operation = &prepared(Operation::FileMaterialization);
    let valid = Descriptors::for_acquisition(operation, clock(100));
    assert_eq!(
        valid.validate(operation, clock(1)),
        vec![DescriptorIssue::ClockBindingMismatch]
    );
    let zero = Descriptors::for_acquisition(operation, clock(0));
    assert_eq!(
        zero.validate(operation, clock(0)),
        vec![DescriptorIssue::InvalidClockObservation]
    );
    let mut unqualified_source = clock(1);
    unqualified_source.resolution_source = "guessed-from-nanosecond-storage";
    assert_eq!(
        Descriptors::for_acquisition(operation, unqualified_source)
            .validate(operation, unqualified_source),
        vec![DescriptorIssue::InvalidClockObservation]
    );
}

#[test]
fn observational_profile_cannot_relax_the_duration_methods_eligibility() {
    let operation = &prepared(Operation::StructuralAnalysis);
    let mut changed = Descriptors::for_acquisition(operation, clock(1));
    changed.method.numeric_decision_eligibility = "observational-only".into();
    assert_eq!(
        changed.validate(operation, clock(1)),
        vec![DescriptorIssue::MethodMismatch]
    );
}

#[test]
fn cache_heap_scratch_and_output_states_are_independent_and_not_inferred() {
    let operation = &prepared(Operation::ProfileSelection);
    let original = Descriptors::for_acquisition(operation, clock(1));
    assert_eq!(original.execution.cache_state, "disabled");
    assert_eq!(original.execution.managed_heap_state, "not-applicable");
    assert_eq!(original.execution.scratch_reuse_state, "fresh");
    assert_eq!(
        original.execution.output_buffer_state,
        OutputBuffer::Applicable {
            ownership: "caller-owned".into(),
            reuse: "fresh".into()
        }
    );
    let mut changed = original;
    changed.execution.scratch_reuse_state = "reset-reused".into();
    assert_eq!(
        changed.validate(operation, clock(1)),
        vec![DescriptorIssue::ExecutionMismatch]
    );
}

#[test]
fn locale_data_is_resident_but_has_no_mutable_scratch_or_output_buffer() {
    let operation = &prepared(Operation::LocaleCanonicalization);
    let descriptors = Descriptors::for_acquisition(operation, clock(1));
    assert_eq!(descriptors.execution.engine_state, "reused");
    assert_eq!(descriptors.execution.initial_preparation_state, "resident");
    assert_eq!(descriptors.execution.cache_state, "disabled");
    assert_eq!(descriptors.execution.scratch_reuse_state, "not-applicable");
    assert_eq!(
        descriptors.execution.output_buffer_state,
        OutputBuffer::NotApplicable {}
    );
    let encoded = serde_json::to_value(&descriptors).unwrap();
    assert_eq!(
        encoded["execution"]["outputBufferState"],
        json!({"state": "not-applicable"})
    );
    assert_eq!(encoded["localeInput"]["identifierByteLimit"], "128");
    assert_eq!(encoded["localeInput"]["identifierByteUnit"], "utf8-octet");
    assert_eq!(
        encoded["localeInput"]["providerReuse"],
        "resident-immutable-provider-reused-between-invocations"
    );
    assert_eq!(
        encoded["localeInput"]["canonicalValueStorage"],
        "result-shares-immutable-provider-storage"
    );
    // Required-nullable: absence is rejected during decoding; null on the
    // locale operation cannot suppress actual provider/data validation.
    let mut changed = encoded;
    changed["localeInput"] = Value::Null;
    let changed: Descriptors = serde_json::from_value(changed).unwrap();
    assert_eq!(
        changed.validate(operation, clock(1)),
        vec![DescriptorIssue::LocaleInputBindingMismatch]
    );
}

#[test]
fn icu4x_descriptors_record_the_engine_state_and_the_admitted_binding() {
    use crate::benchmark::cases::Engine;
    use crate::benchmark::locale_icu4x::PreparedEngine;

    let prepare = |engine| Prepared::Icu4xLocale {
        engine: PreparedEngine::prepare(engine, crate::input_limits::Bound::new(128).unwrap())
            .unwrap(),
        input: std::sync::Arc::from("en-US"),
    };
    let reused = prepare(Engine::Reused);
    let fresh = prepare(Engine::Fresh);
    let descriptors = Descriptors::for_acquisition(&reused, clock(1));
    assert!(descriptors.validate(&reused, clock(1)).is_empty());
    assert_eq!(
        descriptors.boundary,
        Boundary::for_operation(Operation::LocaleCanonicalization)
    );
    // Unlike the finite provider, each call works in and returns new storage.
    assert_eq!(
        descriptors.execution,
        Execution {
            process_state: "reused-process".into(),
            engine_state: "reused".into(),
            initial_preparation_state: "resident".into(),
            cache_state: "disabled".into(),
            runtime_compilation_state: "ahead-of-time".into(),
            managed_heap_state: "not-applicable".into(),
            scratch_reuse_state: "fresh".into(),
            output_buffer_state: OutputBuffer::Applicable {
                ownership: "caller-owned".into(),
                reuse: "fresh".into()
            },
        }
    );
    let encoded = serde_json::to_value(&descriptors).unwrap();
    assert_eq!(encoded["localeInput"], Value::Null);
    assert_eq!(encoded["localeCoreInput"], Value::Null);
    assert_eq!(
        encoded["admittedLocaleInput"]["providerReuse"],
        "resident-admitted-provider-reused-between-invocations"
    );
    let other = Descriptors::for_acquisition(&fresh, clock(1));
    assert!(other.validate(&fresh, clock(1)).is_empty());
    assert_eq!(
        Execution {
            engine_state: "reused".into(),
            ..other.execution.clone()
        },
        descriptors.execution
    );
    assert_eq!(other.execution.engine_state, "fresh");
    // Neither engine's record passes for the other.
    assert_eq!(
        other.validate(&reused, clock(1)),
        vec![
            DescriptorIssue::ExecutionMismatch,
            DescriptorIssue::LocaleInputBindingMismatch
        ]
    );
    // Required-nullable: the admitted facts cannot be dropped.
    let mut changed = encoded;
    changed["admittedLocaleInput"] = Value::Null;
    let changed: Descriptors = serde_json::from_value(changed).unwrap();
    assert_eq!(
        changed.validate(&reused, clock(1)),
        vec![DescriptorIssue::LocaleInputBindingMismatch]
    );
    for operation in &operations() {
        assert_eq!(
            Descriptors::for_acquisition(operation, clock(1)).admitted_locale_input,
            None
        );
    }
}

#[test]
fn locale_resolution_descriptor_binds_its_bounds_roles_and_actual_reuse() {
    let mut operation = prepared(Operation::LocalePolicyResolution);
    let descriptors = Descriptors::for_acquisition(&operation, clock(1));
    assert_eq!(descriptors.execution.scratch_reuse_state, "fresh");
    assert_eq!(descriptors.execution.cache_state, "disabled");
    let encoded = serde_json::to_value(&descriptors).unwrap();
    let input = &encoded["localePolicyResolutionInput"];
    assert_eq!(input["localeOccurrenceLimit"], "128");
    assert_eq!(input["requestedLocaleLimit"], "64");
    assert_eq!(input["identifierByteLimit"], "128");
    assert_eq!(input["occurrenceDomain"].as_array().unwrap().len(), 10);
    assert_eq!(input["selectedProfile"], "app");
    assert_eq!(encoded["localeInput"]["identifierByteLimit"], "128");
    let Prepared::LocalePolicyResolution(locales) = &mut operation else {
        unreachable!()
    };
    let bound = |value| crate::input_limits::Bound::new(value).unwrap();
    locales.policy = crate::resolver::locales::fixture_policy(bound(128), bound(127), bound(64));
    assert_eq!(
        descriptors.validate(&operation, clock(1)),
        [DescriptorIssue::LocalePolicyResolutionInputBindingMismatch]
    );
    let original = prepared(Operation::LocalePolicyResolution);
    for field in ["localePolicyResolutionInput", "localeInput"] {
        let mut missing = encoded.clone();
        missing[field] = Value::Null;
        let decoded: Descriptors = serde_json::from_value(missing).unwrap();
        assert!(!decoded.validate(&original, clock(1)).is_empty());
    }
}

#[test]
fn resource_policy_descriptor_binds_the_capability_and_the_collection() {
    use crate::resolver::limits::ImplementationCapability;

    let mut operation = prepared(Operation::ResourcePolicyBootstrap);
    let descriptors = Descriptors::for_acquisition(&operation, clock(1));
    assert_eq!(descriptors.execution.scratch_reuse_state, "fresh");
    let encoded = serde_json::to_value(&descriptors).unwrap();
    let input = &encoded["resourcePolicyInput"];
    assert_eq!(input["selectedProfile"], "app");
    assert_eq!(input["submittedMembers"], "1");
    assert_eq!(input["artifactLimit"], "1000");
    assert_eq!(input["profileIdByteLimit"], "256");
    for field in [
        "localeInput",
        "localePolicyResolutionInput",
        "admittedLocaleInput",
        "artifactCollectionInput",
        "resolvedBindingInput",
        "targetAndGroupInput",
        "hydrationRelationInput",
    ] {
        assert_eq!(encoded[field], Value::Null, "{field}");
    }
    let Prepared::ResourcePolicy(policy) = &mut operation else {
        unreachable!()
    };
    let mut limits = policy.capability.policy_comparable().clone();
    limits.artifact_admission.max_artifacts = crate::input_limits::Bound::new(999).unwrap();
    policy.capability = ImplementationCapability::fixture_owned(limits);
    assert_eq!(
        descriptors.validate(&operation, clock(1)),
        [DescriptorIssue::ResourcePolicyInputBindingMismatch]
    );
    let original = prepared(Operation::ResourcePolicyBootstrap);
    let mut missing = encoded;
    missing["resourcePolicyInput"] = Value::Null;
    let decoded: Descriptors = serde_json::from_value(missing).unwrap();
    assert_eq!(
        decoded.validate(&original, clock(1)),
        [DescriptorIssue::ResourcePolicyInputBindingMismatch]
    );
    for operation in &operations() {
        assert_eq!(
            Descriptors::for_acquisition(operation, clock(1))
                .resource_policy_input
                .is_some(),
            operation.operation() == Operation::ResourcePolicyBootstrap
        );
    }
}

#[test]
fn artifact_collection_descriptor_binds_the_policy_and_the_collection() {
    use crate::resolver::limits::ConfirmedPolicy;

    let mut operation = prepared(Operation::ArtifactCollectionAdmission);
    let descriptors = Descriptors::for_acquisition(&operation, clock(1));
    assert_eq!(descriptors.execution.scratch_reuse_state, "fresh");
    let encoded = serde_json::to_value(&descriptors).unwrap();
    // 015's pair for the complete collection admission.
    let boundary = &encoded["boundary"];
    assert_eq!(boundary["identity"], "artifact-collection-admission/0");
    assert_eq!(boundary["phase"], "profile_artifact_admit");
    assert_eq!(boundary["cost"], "complete_artifact_collection_admission");
    let input = &encoded["artifactCollectionInput"];
    assert_eq!(input["submittedMembers"], "1");
    assert_eq!(input["artifactLimit"], "1000");
    for field in [
        "localeInput",
        "localePolicyResolutionInput",
        "admittedLocaleInput",
        "resourcePolicyInput",
        "resolvedBindingInput",
        "targetAndGroupInput",
        "hydrationRelationInput",
    ] {
        assert_eq!(encoded[field], Value::Null, "{field}");
    }
    let Prepared::ArtifactCollection(collection) = &mut operation else {
        unreachable!()
    };
    let mut limits = collection.policy.limits().clone();
    limits.artifact_admission.max_artifacts = crate::input_limits::Bound::new(999).unwrap();
    collection.policy = ConfirmedPolicy::fixture_owned(limits);
    assert_eq!(
        descriptors.validate(&operation, clock(1)),
        [DescriptorIssue::ArtifactCollectionInputBindingMismatch]
    );
    let original = prepared(Operation::ArtifactCollectionAdmission);
    let mut missing = encoded;
    missing["artifactCollectionInput"] = Value::Null;
    let decoded: Descriptors = serde_json::from_value(missing).unwrap();
    assert_eq!(
        decoded.validate(&original, clock(1)),
        [DescriptorIssue::ArtifactCollectionInputBindingMismatch]
    );
    for operation in &operations() {
        assert_eq!(
            Descriptors::for_acquisition(operation, clock(1))
                .artifact_collection_input
                .is_some(),
            operation.operation() == Operation::ArtifactCollectionAdmission
        );
    }
}

#[test]
fn resolved_binding_descriptor_binds_the_configuration_and_the_collection() {
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, Recipe, ResolvedBindingRecipe};

    let mut operation = prepared(Operation::ResolvedBindingConstruction);
    let descriptors = Descriptors::for_acquisition(&operation, clock(1));
    assert_eq!(descriptors.execution.scratch_reuse_state, "fresh");
    let encoded = serde_json::to_value(&descriptors).unwrap();
    // 015's pair for the exact resolution and the bindings.
    let boundary = &encoded["boundary"];
    assert_eq!(boundary["identity"], "resolved-binding-construction/0");
    assert_eq!(boundary["phase"], "profile_artifact_admit");
    assert_eq!(boundary["cost"], "resolved_binding_construction");
    let input = &encoded["resolvedBindingInput"];
    assert_eq!(input["selectedProfile"], "app");
    assert_eq!(input["submittedMembers"], "6");
    for field in [
        "localeInput",
        "localePolicyResolutionInput",
        "admittedLocaleInput",
        "resourcePolicyInput",
        "artifactCollectionInput",
        "targetAndGroupInput",
        "hydrationRelationInput",
    ] {
        assert_eq!(encoded[field], Value::Null, "{field}");
    }
    // Another collection is another input.
    let every = declarations()
        .into_iter()
        .find(|case| case.fixture == Recipe::ResolvedBinding(ResolvedBindingRecipe::EveryReference))
        .unwrap();
    let Prepared::ResolvedBinding(other) = prepare(&every).unwrap().prepared else {
        unreachable!()
    };
    let Prepared::ResolvedBinding(bindings) = &mut operation else {
        unreachable!()
    };
    bindings.collection = other.collection;
    assert_eq!(
        descriptors.validate(&operation, clock(1)),
        [DescriptorIssue::ResolvedBindingInputBindingMismatch]
    );
    let original = prepared(Operation::ResolvedBindingConstruction);
    let mut missing = encoded;
    missing["resolvedBindingInput"] = Value::Null;
    let decoded: Descriptors = serde_json::from_value(missing).unwrap();
    assert_eq!(
        decoded.validate(&original, clock(1)),
        [DescriptorIssue::ResolvedBindingInputBindingMismatch]
    );
    for operation in &operations() {
        assert_eq!(
            Descriptors::for_acquisition(operation, clock(1))
                .resolved_binding_input
                .is_some(),
            operation.operation() == Operation::ResolvedBindingConstruction
        );
    }
}

#[test]
fn target_and_group_descriptor_binds_the_configuration_and_the_collection() {
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, Recipe, TargetAndGroupRecipe};

    let mut operation = prepared(Operation::TargetAndGroupResolution);
    let descriptors = Descriptors::for_acquisition(&operation, clock(1));
    // Each invocation builds its own evaluation and state: nothing of one is
    // reused by the next.
    assert_eq!(descriptors.execution.scratch_reuse_state, "fresh");
    let encoded = serde_json::to_value(&descriptors).unwrap();
    // 015's first pair of the target and group resolution.
    let boundary = &encoded["boundary"];
    assert_eq!(boundary["identity"], "target-and-group-resolution/0");
    assert_eq!(boundary["phase"], "profile_target_group_resolve");
    assert_eq!(boundary["cost"], "target_and_group_resolution");
    let input = &encoded["targetAndGroupInput"];
    assert_eq!(input["selectedProfile"], "app");
    assert_eq!(input["submittedMembers"], "6");
    for field in [
        "localeInput",
        "localePolicyResolutionInput",
        "admittedLocaleInput",
        "resourcePolicyInput",
        "artifactCollectionInput",
        "resolvedBindingInput",
        "hydrationRelationInput",
    ] {
        assert_eq!(encoded[field], Value::Null, "{field}");
    }
    // Another collection is another input.
    let storefront = declarations()
        .into_iter()
        .find(|case| {
            case.fixture == Recipe::TargetAndGroup(TargetAndGroupRecipe::Storefront)
                && case.limit.is_none()
        })
        .unwrap();
    let Prepared::TargetAndGroup(other) = prepare(&storefront).unwrap().prepared else {
        unreachable!()
    };
    let Prepared::TargetAndGroup(targets) = &mut operation else {
        unreachable!()
    };
    targets.collection = other.collection;
    assert_eq!(
        descriptors.validate(&operation, clock(1)),
        [DescriptorIssue::TargetAndGroupInputBindingMismatch]
    );
    let original = prepared(Operation::TargetAndGroupResolution);
    let mut missing = encoded;
    missing["targetAndGroupInput"] = Value::Null;
    let decoded: Descriptors = serde_json::from_value(missing).unwrap();
    assert_eq!(
        decoded.validate(&original, clock(1)),
        [DescriptorIssue::TargetAndGroupInputBindingMismatch]
    );
    for operation in &operations() {
        assert_eq!(
            Descriptors::for_acquisition(operation, clock(1))
                .target_and_group_input
                .is_some(),
            operation.operation() == Operation::TargetAndGroupResolution
        );
    }
}

#[test]
fn hydration_relation_descriptor_binds_the_configuration_and_the_collection() {
    use crate::benchmark::cases::prepare::prepare;
    use crate::benchmark::cases::{declarations, HydrationRecipe, Recipe};

    let mut operation = prepared(Operation::HydrationRelationValidation);
    let descriptors = Descriptors::for_acquisition(&operation, clock(1));
    // Each invocation builds its own evaluation and state.
    assert_eq!(descriptors.execution.scratch_reuse_state, "fresh");
    let encoded = serde_json::to_value(&descriptors).unwrap();
    // 015's second pair of the target and group resolution.
    let boundary = &encoded["boundary"];
    assert_eq!(boundary["identity"], "hydration-relation-validation/0");
    assert_eq!(boundary["phase"], "profile_target_group_resolve");
    assert_eq!(boundary["cost"], "hydration_relation_validation");
    let input = &encoded["hydrationRelationInput"];
    assert_eq!(input["selectedProfile"], "app");
    assert_eq!(input["submittedMembers"], "7");
    for field in [
        "localeInput",
        "localePolicyResolutionInput",
        "admittedLocaleInput",
        "resourcePolicyInput",
        "artifactCollectionInput",
        "resolvedBindingInput",
        "targetAndGroupInput",
    ] {
        assert_eq!(encoded[field], Value::Null, "{field}");
    }
    // Another collection is another input.
    let fan = declarations()
        .into_iter()
        .find(|case| {
            case.fixture == Recipe::Hydration(HydrationRecipe::FanOutFanIn) && case.limit.is_none()
        })
        .unwrap();
    let Prepared::Hydration(other) = prepare(&fan).unwrap().prepared else {
        unreachable!()
    };
    let Prepared::Hydration(hydration) = &mut operation else {
        unreachable!()
    };
    hydration.collection = other.collection;
    assert_eq!(
        descriptors.validate(&operation, clock(1)),
        [DescriptorIssue::HydrationRelationInputBindingMismatch]
    );
    let original = prepared(Operation::HydrationRelationValidation);
    let mut missing = encoded;
    missing["hydrationRelationInput"] = Value::Null;
    let decoded: Descriptors = serde_json::from_value(missing).unwrap();
    assert_eq!(
        decoded.validate(&original, clock(1)),
        [DescriptorIssue::HydrationRelationInputBindingMismatch]
    );
    for operation in &operations() {
        assert_eq!(
            Descriptors::for_acquisition(operation, clock(1))
                .hydration_relation_input
                .is_some(),
            operation.operation() == Operation::HydrationRelationValidation
        );
    }
}

#[test]
fn icu4x_resolution_descriptors_bind_the_admitted_provider_beside_the_resolution_input() {
    use crate::benchmark::cases::{
        declarations, prepare::prepare, Engine, LocalePolicyResolutionRecipe, Recipe,
    };

    let prepared = |fixture| {
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == fixture)
            .unwrap();
        prepare(&declaration).unwrap().prepared
    };
    let input = LocalePolicyResolutionRecipe::EveryRole;
    let finite = prepared(Recipe::LocalePolicyResolution(input));
    let reused = prepared(Recipe::Icu4xLocalePolicyResolution {
        input,
        engine: Engine::Reused,
    });
    let fresh = prepared(Recipe::Icu4xLocalePolicyResolution {
        input,
        engine: Engine::Fresh,
    });
    let record = |operation| Descriptors::for_acquisition(operation, clock(1));
    for (operation, engine, reuse) in [
        (
            &reused,
            "reused",
            "resident-admitted-provider-reused-between-invocations",
        ),
        (
            &fresh,
            "fresh",
            "artifact-admitted-and-bound-before-each-invocation",
        ),
    ] {
        let descriptors = record(operation);
        assert!(descriptors.validate(operation, clock(1)).is_empty());
        assert_eq!(
            descriptors.boundary,
            Boundary::for_operation(Operation::LocalePolicyResolution)
        );
        assert_eq!(descriptors.execution.engine_state, engine);
        let encoded = serde_json::to_value(&descriptors).unwrap();
        assert_eq!(encoded["localeInput"], Value::Null);
        assert_eq!(encoded["admittedLocaleInput"]["providerReuse"], reuse);
        assert_eq!(
            descriptors.locale_policy_resolution_input,
            record(&finite).locale_policy_resolution_input
        );
    }
    // Phases 7 and 8's own state is the one their reused engine runs in, so
    // only the provider's facts tell the finite record from the ICU4X one.
    assert_eq!(record(&reused).execution, record(&finite).execution);
    assert_eq!(
        record(&finite).validate(&reused, clock(1)),
        [DescriptorIssue::LocaleInputBindingMismatch]
    );
    assert_eq!(
        record(&fresh).validate(&reused, clock(1)),
        [
            DescriptorIssue::ExecutionMismatch,
            DescriptorIssue::LocaleInputBindingMismatch
        ]
    );
}

#[test]
fn simultaneous_descriptor_issues_have_stable_registry_order() {
    let operation = &prepared(Operation::FileMaterialization);
    let mut broken = Descriptors::for_acquisition(operation, clock(2));
    broken.boundary.metric = "peak_live_bytes".into();
    broken.method.estimated_overhead_subtraction = true;
    broken.clock_observation.resolution_nanoseconds = Quantity::new(0);
    broken.execution.initial_preparation_state = "absent".into();
    assert_eq!(
        broken.validate(operation, clock(2)),
        vec![
            DescriptorIssue::BoundaryMismatch,
            DescriptorIssue::MethodMismatch,
            DescriptorIssue::ClockBindingMismatch,
            DescriptorIssue::InvalidClockObservation,
            DescriptorIssue::ExecutionMismatch,
        ]
    );
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn actual_provider_description_is_supported_without_a_numeric_threshold() {
    let clock = crate::benchmark::clock::MonotonicClock::acquire().unwrap();
    for operation in &operations() {
        assert!(Descriptors::for_acquisition(operation, clock.description())
            .validate(operation, clock.description())
            .is_empty());
    }
}
