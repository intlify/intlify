// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::{json, Value};

use super::cases::{declarations, registry::Registry};
use super::operation::Operation;
use super::profile::{AdmittedProfile, MeasurementProfile, ProfileIssue};

#[test]
fn smoke_profile_fixes_the_complete_inventory_and_observational_sampling() {
    let registry = Registry::load().unwrap();
    let profile = AdmittedProfile::smoke(&registry);
    let document = profile.document();
    assert_eq!(document.cases(), declarations());
    assert_eq!(document.cases().len(), 127);
    for operation in Operation::ALL {
        assert!(document
            .cases()
            .iter()
            .any(|case| case.operation == operation));
    }
    let value = serde_json::to_value(document).unwrap();
    assert_eq!(value["identity"], "intlify-config-minimum-smoke");
    assert_eq!(value["revision"], "1");
    assert_eq!(
        value["adoptedSpecification"]["identity"],
        "intlify-design-026"
    );
    assert_eq!(value["adoptedSpecification"]["revision"], "0");
    assert_eq!(value["sampling"]["warmupRepetitions"], "1");
    assert_eq!(value["sampling"]["measuredSamples"], "1");
    assert_eq!(value["sampling"]["repetitionsPerSample"], "1");
    assert_eq!(value["sampling"]["aggregation"], "batch_total");
    assert_eq!(value["numericDecisions"], "prohibited-advisory-and-gating");
    assert_eq!(value["fixtureRegistry"]["revision"], "3");
    assert_eq!(value["ordering"], "fixture-registry-order-no-interleaving");
    assert_eq!(
        value["rawSamples"],
        "retain-all-in-order-no-outlier-deletion"
    );
    let decoded: MeasurementProfile = serde_json::from_value(value).unwrap();
    assert!(decoded.validate_smoke(&registry).is_empty());
    assert_eq!(
        AdmittedProfile::admit_smoke(decoded, &registry)
            .unwrap()
            .document(),
        document
    );
}

#[test]
fn altered_sampling_and_inventories_do_not_select_their_own_validation_rules() {
    let registry = Registry::load().unwrap();
    let profile = AdmittedProfile::smoke(&registry);
    let original = serde_json::to_value(profile.document()).unwrap();
    for (pointer, replacement, issue) in [
        (
            "/identity",
            json!("another-profile"),
            ProfileIssue::Identity,
        ),
        ("/revision", json!("0"), ProfileIssue::Identity),
        ("/revision", json!("2"), ProfileIssue::Identity),
        (
            "/adoptedSpecification/revision",
            json!("1"),
            ProfileIssue::Specification,
        ),
        (
            "/fixtureRegistry/revision",
            json!("1"),
            ProfileIssue::FixtureRegistry,
        ),
        (
            "/sampling/warmupRepetitions",
            json!("0"),
            ProfileIssue::Sampling,
        ),
        (
            "/sampling/measuredSamples",
            json!("2"),
            ProfileIssue::Sampling,
        ),
        (
            "/sampling/repetitionsPerSample",
            json!("2"),
            ProfileIssue::Sampling,
        ),
        (
            "/numericDecisions",
            json!("gating"),
            ProfileIssue::NumericPolicy,
        ),
        ("/ordering", json!("fastest-first"), ProfileIssue::Ordering),
        (
            "/rawSamples",
            json!("average-only"),
            ProfileIssue::Retention,
        ),
    ] {
        let mut value = original.clone();
        *value.pointer_mut(pointer).unwrap() = replacement;
        let decoded: MeasurementProfile = serde_json::from_value(value).unwrap();
        assert!(
            decoded.validate_smoke(&registry).contains(&issue),
            "{pointer}"
        );
        assert!(AdmittedProfile::admit_smoke(decoded, &registry).is_err());
    }
    for mutation in 0..4 {
        let mut value = original.clone();
        let cases = value["cases"].as_array_mut().unwrap();
        match mutation {
            0 => {
                cases.pop();
            }
            1 => {
                cases.push(cases[0].clone());
            }
            2 => cases.swap(0, 1),
            _ => cases[0]["fixtureRevision"] = json!("changed"),
        }
        let decoded: MeasurementProfile = serde_json::from_value(value).unwrap();
        assert!(decoded
            .validate_smoke(&registry)
            .contains(&ProfileIssue::Inventory));
    }
}

#[test]
fn every_policy_leaf_is_checked_and_every_fixed_object_is_closed() {
    let registry = Registry::load().unwrap();
    let original = serde_json::to_value(AdmittedProfile::smoke(&registry).document()).unwrap();
    let mut leaves = Vec::new();
    let mut objects = Vec::new();
    paths(&original, String::new(), &mut leaves, &mut objects);
    for pointer in leaves {
        let mut value = original.clone();
        let slot = value.pointer_mut(&pointer).unwrap();
        *slot = match slot {
            Value::String(text) => Value::String(format!("{text}-changed")),
            Value::Bool(flag) => Value::Bool(!*flag),
            Value::Null => json!("not-null"),
            Value::Number(_) => json!(999),
            _ => unreachable!(),
        };
        if let Ok(decoded) = serde_json::from_value::<MeasurementProfile>(value) {
            assert!(
                !decoded.validate_smoke(&registry).is_empty(),
                "unchecked {pointer}"
            );
        }
    }
    for pointer in objects {
        let mut value = original.clone();
        value
            .pointer_mut(&pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknownField".into(), json!(true));
        assert!(
            serde_json::from_value::<MeasurementProfile>(value).is_err(),
            "{pointer}"
        );
        for key in original
            .pointer(&pointer)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
        {
            let mut value = original.clone();
            value
                .pointer_mut(&pointer)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(key);
            assert!(
                serde_json::from_value::<MeasurementProfile>(value).is_err(),
                "{pointer}/{key}"
            );
        }
    }
    for pointer in [
        "/sampling/measuredSamples",
        "/sampling/repetitionsPerSample",
    ] {
        for invalid in [json!("0"), json!(0), json!("18446744073709551616")] {
            let mut value = original.clone();
            *value.pointer_mut(pointer).unwrap() = invalid;
            assert!(serde_json::from_value::<MeasurementProfile>(value).is_err());
        }
    }
}

fn paths(value: &Value, pointer: String, leaves: &mut Vec<String>, objects: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            objects.push(pointer.clone());
            for (key, value) in map {
                let key = key.replace('~', "~0").replace('/', "~1");
                paths(value, format!("{pointer}/{key}"), leaves, objects);
            }
        }
        Value::Array(items) => {
            for (index, value) in items.iter().enumerate() {
                paths(value, format!("{pointer}/{index}"), leaves, objects);
            }
        }
        _ => leaves.push(pointer),
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
mod native {
    use super::*;
    use crate::benchmark::clock::MonotonicClock;
    use crate::benchmark::observation::Frame;
    use crate::benchmark::profile::{
        ProfileCollectionFailure, ProfileCollectionIssue, ProfiledOperation,
    };
    use crate::benchmark::quantity::Quantity;
    use crate::benchmark::sample::CaptureBinding;

    fn binding(run: &str, declaration: &crate::benchmark::cases::Declaration) -> CaptureBinding {
        // Private test locators, not globally issued Run/Case identities.
        let mut run_frame = Frame::new("profile-capture-test-run");
        run_frame.text(run);
        let mut case_frame = Frame::new("profile-capture-test-case");
        case_frame.json(&serde_json::to_value(declaration).unwrap());
        CaptureBinding {
            run: run_frame.finish(),
            case: case_frame.finish(),
        }
    }

    #[test]
    fn all_127_cases_use_the_admitted_profile_for_native_capture_and_revalidation() {
        let registry = Registry::load().unwrap();
        let profile = AdmittedProfile::smoke(&registry);
        let clock = MonotonicClock::acquire().unwrap();
        let mut retained = Vec::new();
        for (index, declaration) in profile.document().cases().iter().enumerate() {
            let ordinal = Quantity::new(u64::try_from(index).unwrap());
            let fixture = registry.prepare(declaration).unwrap();
            let binding = binding("complete-profile-test", declaration);
            let record = profile.collect(&clock, ordinal, &fixture, binding).unwrap();
            let value = serde_json::to_value(&record).unwrap();
            assert_eq!(value["caseOrdinal"], index.to_string());
            assert_eq!(
                value["measurementProfile"]["identity"],
                "intlify-config-minimum-smoke"
            );
            assert_eq!(value["operation"]["capture"]["warmupCompleted"], "1");
            let samples = value["operation"]["capture"]["samples"].as_array().unwrap();
            assert_eq!(samples.len(), 1);
            assert_eq!(samples[0]["repetitionCount"], "1");
            assert_eq!(samples[0]["ordinal"], "0");
            assert_eq!(
                value["operation"]["descriptors"]["method"]["numericDecisionEligibility"],
                "qualified-runner"
            );
            // The profile prohibits numeric decisions without changing the
            // method's 026 duration class or inventing a positive duration.
            let decoded: ProfiledOperation = serde_json::from_value(value).unwrap();
            assert!(decoded
                .validate(&profile, ordinal, &fixture, clock.description(), binding)
                .is_empty());
            assert_eq!(decoded, record);
            retained.push(record);
        }
        drop(registry);
        drop(profile);
        assert_eq!(retained.len(), 127);
        assert!(!serde_json::to_vec(&retained).unwrap().is_empty());
        // This array alone is not a complete Owner Result, admitted Run Plan,
        // or common Measurement Evidence; build/environment and record gates
        // remain responsibilities of the enclosing harness.
    }

    #[test]
    fn profile_and_case_locators_cannot_be_rebound_by_decoded_rows() {
        let registry = Registry::load().unwrap();
        let profile = AdmittedProfile::smoke(&registry);
        let clock = MonotonicClock::acquire().unwrap();
        let declaration = &profile.document().cases()[0];
        let fixture = registry.prepare(declaration).unwrap();
        let first = Quantity::new(0);
        let binding = binding("binding-test", declaration);
        let record = profile.collect(&clock, first, &fixture, binding).unwrap();
        let original = serde_json::to_value(record).unwrap();
        for (pointer, replacement, issue) in [
            (
                "/measurementProfile/identity",
                json!("another-profile"),
                ProfileCollectionIssue::ProfileBinding,
            ),
            (
                "/measurementProfile/revision",
                json!("0"),
                ProfileCollectionIssue::ProfileBinding,
            ),
            (
                "/caseOrdinal",
                json!("1"),
                ProfileCollectionIssue::CaseSelection,
            ),
            (
                "/caseOrdinal",
                json!("18446744073709551615"),
                ProfileCollectionIssue::CaseSelection,
            ),
        ] {
            let mut value = original.clone();
            *value.pointer_mut(pointer).unwrap() = replacement;
            let decoded: ProfiledOperation = serde_json::from_value(value).unwrap();
            assert!(decoded
                .validate(&profile, first, &fixture, clock.description(), binding)
                .contains(&issue));
        }
        for ordinal in [
            Quantity::new(1),
            Quantity::new(127),
            Quantity::new(u64::MAX),
        ] {
            assert!(matches!(
                profile.collect(&clock, ordinal, &fixture, binding),
                Err(ProfileCollectionFailure::CaseSelection)
            ));
        }
        let decoded: ProfiledOperation = serde_json::from_value(original).unwrap();
        let another = registry.prepare(&profile.document().cases()[1]).unwrap();
        assert!(decoded
            .validate(&profile, first, &another, clock.description(), binding)
            .contains(&ProfileCollectionIssue::CaseSelection));
        let mut other_run = binding;
        other_run.run = binding.case;
        assert!(!decoded
            .validate(&profile, first, &fixture, clock.description(), other_run)
            .is_empty());
    }

    #[test]
    fn sample_counts_and_acquired_clock_are_not_inferred_from_the_record() {
        let registry = Registry::load().unwrap();
        let profile = AdmittedProfile::smoke(&registry);
        let clock = MonotonicClock::acquire().unwrap();
        let declaration = &profile.document().cases()[0];
        let fixture = registry.prepare(declaration).unwrap();
        let ordinal = Quantity::new(0);
        let binding = binding("sample-policy-test", declaration);
        let original =
            serde_json::to_value(profile.collect(&clock, ordinal, &fixture, binding).unwrap())
                .unwrap();
        for pointer in [
            "/operation/capture/warmupCompleted",
            "/operation/capture/samples/0/repetitionCount",
        ] {
            let mut value = original.clone();
            *value.pointer_mut(pointer).unwrap() = json!("2");
            let record: ProfiledOperation = serde_json::from_value(value).unwrap();
            assert!(!record
                .validate(&profile, ordinal, &fixture, clock.description(), binding)
                .is_empty());
        }
        let mut value = original.clone();
        let samples = value["operation"]["capture"]["samples"]
            .as_array_mut()
            .unwrap();
        samples.push(samples[0].clone());
        let record: ProfiledOperation = serde_json::from_value(value).unwrap();
        assert!(!record
            .validate(&profile, ordinal, &fixture, clock.description(), binding)
            .is_empty());
        let record: ProfiledOperation = serde_json::from_value(original).unwrap();
        let mut changed_clock = clock.description();
        changed_clock.resolution_nanoseconds =
            Quantity::new(changed_clock.resolution_nanoseconds.get() + 1);
        assert!(!record
            .validate(&profile, ordinal, &fixture, changed_clock, binding)
            .is_empty());
    }
}
