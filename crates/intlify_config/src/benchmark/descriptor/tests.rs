// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::collections::BTreeSet;

use serde_json::{json, Value};

use super::*;

fn clock(resolution: u64) -> ClockDescription {
    ClockDescription {
        provider: "rustix",
        provider_revision: "1.1.4",
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
    for operation in Operation::ALL {
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
    assert_eq!(pairs.len(), 4);
    assert_eq!(identities.len(), 4);
}

#[test]
fn wrong_operation_and_mutated_marker_order_or_nesting_are_rejected() {
    let valid = Descriptors::for_acquisition(Operation::FileMaterialization, clock(10));
    assert_eq!(
        valid.validate(Operation::StructuralAnalysis, clock(10)),
        vec![DescriptorIssue::BoundaryMismatch]
    );
    let mut reordered = valid.clone();
    reordered.boundary.included_markers.reverse();
    assert_eq!(
        reordered.validate(Operation::FileMaterialization, clock(10)),
        vec![DescriptorIssue::BoundaryMismatch]
    );
    let mut nested = valid;
    nested
        .boundary
        .direct_parents
        .push("invented-workflow".into());
    assert_eq!(
        nested.validate(Operation::FileMaterialization, clock(10)),
        vec![DescriptorIssue::BoundaryMismatch]
    );
}

#[test]
fn every_descriptor_leaf_is_validated_not_just_identity_and_revision() {
    let descriptors = Descriptors::for_acquisition(Operation::ProfileSelection, clock(17));
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
        // Closed representations may reject during decoding; otherwise semantic
        // validation must reject against the separate acquisition and registry.
        if let Ok(decoded) = serde_json::from_value::<Descriptors>(tampered) {
            assert!(
                !decoded
                    .validate(Operation::ProfileSelection, clock(17))
                    .is_empty(),
                "unvalidated field {path}"
            );
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
    let original = serde_json::to_value(Descriptors::for_acquisition(
        Operation::AuthoringConstruction,
        clock(1),
    ))
    .unwrap();
    for path in [
        "",
        "/boundary",
        "/method",
        "/method/optimizationBarrier",
        "/clockObservation",
        "/execution",
        "/execution/outputBufferState",
    ] {
        let mut extra = original.clone();
        extra
            .pointer_mut(path)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        assert!(serde_json::from_value::<Descriptors>(extra).is_err());
        for key in original.pointer(path).unwrap().as_object().unwrap().keys() {
            let mut missing = original.clone();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(serde_json::from_value::<Descriptors>(missing).is_err());
        }
    }
}

#[test]
fn resolution_is_bound_to_the_actual_acquisition_not_storage_precision() {
    let operation = Operation::FileMaterialization;
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
    let operation = Operation::StructuralAnalysis;
    let mut changed = Descriptors::for_acquisition(operation, clock(1));
    changed.method.numeric_decision_eligibility = "observational-only".into();
    assert_eq!(
        changed.validate(operation, clock(1)),
        vec![DescriptorIssue::MethodMismatch]
    );
}

#[test]
fn cache_heap_scratch_and_output_states_are_independent_and_not_inferred() {
    let operation = Operation::ProfileSelection;
    let original = Descriptors::for_acquisition(operation, clock(1));
    assert_eq!(original.execution.cache_state, "disabled");
    assert_eq!(original.execution.managed_heap_state, "not-applicable");
    assert_eq!(original.execution.scratch_reuse_state, "fresh");
    assert_eq!(
        original.execution.output_buffer_state.ownership,
        "caller-owned"
    );
    let mut changed = original;
    changed.execution.scratch_reuse_state = "reset-reused".into();
    assert_eq!(
        changed.validate(operation, clock(1)),
        vec![DescriptorIssue::ExecutionMismatch]
    );
}

#[test]
fn simultaneous_descriptor_issues_have_stable_registry_order() {
    let operation = Operation::FileMaterialization;
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
    for operation in Operation::ALL {
        assert!(Descriptors::for_acquisition(operation, clock.description())
            .validate(operation, clock.description())
            .is_empty());
    }
}
