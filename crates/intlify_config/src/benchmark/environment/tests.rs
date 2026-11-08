// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::json;

#[cfg(any(target_os = "linux", target_os = "macos"))]
use crate::benchmark::clock::MonotonicClock;
use crate::benchmark::instrument::tests::DESCRIPTION;

use super::*;

// The kernel, target and parallelism views are the shared acquisition's, and
// their closed vocabularies are pinned in `intlify_measurement`. What this owner
// adds is its runner context and the codec that frames them.

#[test]
fn the_runner_cannot_be_promoted_by_a_ci_label_or_submitted_evidence() {
    assert_eq!(
        serde_json::to_value(RunnerContext::LocalUncontrolled).unwrap(),
        json!("local-uncontrolled")
    );
    for invalid in [
        json!("qualified"),
        json!("controlled-unqualified"),
        json!({"kind":"qualified","runner":"CI"}),
    ] {
        assert!(serde_json::from_value::<RunnerContext>(invalid).is_err());
    }
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn native_acquisition_retains_one_owned_snapshot_and_the_actual_clock() {
    let clock = MonotonicClock::acquire().unwrap();
    let acquired =
        ObservedEnvironment::acquire(InstrumentDescription::Clock(clock.description())).unwrap();
    let value = serde_json::to_value(acquired.document()).unwrap();
    assert_eq!(value["codec"], "intlify-config-environment-inputs/0");
    assert_eq!(value["runner"], "local-uncontrolled");
    assert_eq!(value["kernelView"]["state"], "observed");
    assert_eq!(value["clock"]["provider"], clock.description().provider);
    assert_eq!(
        value["clock"]["resolutionNanoseconds"],
        clock.description().resolution_nanoseconds.get().to_string()
    );
    assert_eq!(value["libraryTarget"]["os"]["state"], "observed");
    let bytes = serde_json::to_vec(acquired.document()).unwrap();
    let decoded: EnvironmentInputs = serde_json::from_slice(&bytes).unwrap();
    assert!(acquired.validate(&decoded).is_empty());
    assert_eq!(acquired.checksum(), checksum(&decoded).unwrap());
    let text = String::from_utf8(bytes.clone()).unwrap();
    for forbidden in [
        "hostname",
        "username",
        "nodename",
        "domainname",
        "RUSTFLAGS",
        env!("CARGO_MANIFEST_DIR"),
    ] {
        assert!(!text.contains(forbidden));
    }
    // No repeated host query is needed to retain or serialize the snapshot.
    drop(acquired);
    assert_eq!(serde_json::to_vec(&decoded).unwrap(), bytes);
}

#[test]
#[cfg(any(target_os = "linux", target_os = "macos"))]
fn submitted_environment_is_checked_against_acquisition_not_itself() {
    let clock = MonotonicClock::acquire().unwrap();
    let acquired =
        ObservedEnvironment::acquire(InstrumentDescription::Clock(clock.description())).unwrap();
    let original = serde_json::to_value(acquired.document()).unwrap();
    for (pointer, value) in [
        ("/codec", json!("different-codec")),
        ("/clock/providerRevision", json!("different-provider")),
        ("/clock/resolutionNanoseconds", json!("0")),
        (
            "/kernelView",
            json!({"state":"unavailable","reason":"unsupported-platform"}),
        ),
        (
            "/availableParallelismHint",
            json!({"state":"unavailable","reason":"quantity-overflow"}),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        let submitted: EnvironmentInputs = serde_json::from_value(changed).unwrap();
        assert_ne!(checksum(&submitted).unwrap(), acquired.checksum());
        assert_eq!(
            acquired.validate(&submitted),
            vec![EnvironmentIssue::ObservationMismatch]
        );
    }
    for key in original.as_object().unwrap().keys() {
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove(key);
        // A memory run reads no clock, so a document without one decodes,
        // but it is not the environment this duration run acquired.
        match serde_json::from_value::<EnvironmentInputs>(changed) {
            Ok(submitted) => {
                assert_eq!(key, "clock");
                assert_eq!(
                    acquired.validate(&submitted),
                    vec![EnvironmentIssue::ObservationMismatch]
                );
            }
            Err(_) => assert_ne!(key, "clock"),
        }
    }
    for pointer in [
        "",
        "/libraryTarget",
        "/kernelView",
        "/kernelView/value",
        "/clock",
    ] {
        let mut changed = original.clone();
        changed
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!("private"));
        assert!(
            serde_json::from_value::<EnvironmentInputs>(changed).is_err(),
            "{pointer}"
        );
    }
}

#[test]
fn a_memory_run_records_its_observer_as_it_describes_itself_and_no_clock() {
    let acquired =
        ObservedEnvironment::acquire(InstrumentDescription::Observer(DESCRIPTION)).unwrap();
    let value = serde_json::to_value(acquired.document()).unwrap();
    assert!(value.get("clock").is_none());
    assert_eq!(
        value["memoryObserver"],
        json!({
            "provider": "test-provider",
            "providerRevision": "9.9.9",
            "identity": "intlify-config-test-observer",
            "revision": "0",
            "counting": "scripted-reports",
            "limitations": ["reports-are-written-by-the-test"],
        })
    );
    let decoded: EnvironmentInputs = serde_json::from_value(value.clone()).unwrap();
    assert!(acquired.validate(&decoded).is_empty());
    assert_eq!(acquired.checksum(), checksum(&decoded).unwrap());
    // An observer is not an optional null, and another one is not this run's.
    let mut null = value.clone();
    null["memoryObserver"] = json!(null);
    assert!(serde_json::from_value::<EnvironmentInputs>(null).is_err());
    let mut other = value;
    other["memoryObserver"]["revision"] = json!("1");
    let other: EnvironmentInputs = serde_json::from_value(other).unwrap();
    assert_eq!(
        acquired.validate(&other),
        vec![EnvironmentIssue::ObservationMismatch]
    );
}
