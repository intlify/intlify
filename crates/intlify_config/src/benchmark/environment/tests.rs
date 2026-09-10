// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::json;

use super::*;

#[test]
fn system_identifiers_are_closed_and_never_echo_unknown_input() {
    for (input, expected) in [
        ("linux", TargetOs::Linux),
        ("macos", TargetOs::MacOs),
        ("windows", TargetOs::Windows),
    ] {
        assert_eq!(target_os(input), Acquired::Observed { value: expected });
    }
    for input in ["unknown", "private-host", " Linux"] {
        assert!(matches!(target_os(input), Acquired::Unavailable { .. }));
    }
    for (input, expected) in [
        (b"Linux".as_slice(), KernelFamily::Linux),
        (b"Darwin".as_slice(), KernelFamily::Darwin),
    ] {
        assert_eq!(kernel_family(input), Acquired::Observed { value: expected });
    }
    for (input, expected) in [
        (b"aarch64".as_slice(), Architecture::Aarch64),
        (b"arm64".as_slice(), Architecture::Aarch64),
        (b"x86_64".as_slice(), Architecture::X86_64),
        (b"riscv64".as_slice(), Architecture::Riscv64),
    ] {
        assert_eq!(architecture(input), Acquired::Observed { value: expected });
    }
    for input in [b"private-host-name".as_slice(), &[0xff], b"", b" Linux"] {
        let family = kernel_family(input);
        let architecture = architecture(input);
        assert!(matches!(family, Acquired::Unavailable { .. }));
        assert!(matches!(architecture, Acquired::Unavailable { .. }));
        assert!(!serde_json::to_string(&family).unwrap().contains("private"));
        assert!(!serde_json::to_string(&architecture)
            .unwrap()
            .contains("private"));
    }
}

#[test]
fn kernel_release_is_whole_controlled_input_not_an_os_or_build_version_guess() {
    for input in ["6.8.0", "24.1.0", "1.2.3.4"] {
        let acquired = kernel_release(input.as_bytes());
        assert_eq!(
            serde_json::to_value(acquired).unwrap(),
            json!({"state":"observed","value":input})
        );
    }
    for input in [
        "",
        "1..0",
        "01.2",
        "1.2.3.4.5",
        "24.1.0-private-host",
        "6.8.0/secret",
        "6.8.0\n",
        " 6.8.0",
    ] {
        let acquired = kernel_release(input.as_bytes());
        assert_eq!(
            serde_json::to_value(acquired).unwrap(),
            json!({"state":"unavailable","reason":"unsupported-kernel-release"})
        );
    }
    assert!(matches!(
        kernel_release(&[0xff]),
        Acquired::Unavailable { .. }
    ));
    assert!(matches!(
        kernel_release("1".repeat(33).as_bytes()),
        Acquired::Unavailable { .. }
    ));
    assert!(matches!(
        kernel_release("1".repeat(32).as_bytes()),
        Acquired::Observed { .. }
    ));
}

#[test]
fn parallelism_hint_is_positive_exact_and_distinct_from_hardware_cpu_count() {
    for count in [1, 9_007_199_254_740_992, u64::MAX] {
        let acquired = parallelism_hint(Ok(count));
        let encoded = serde_json::to_value(&acquired).unwrap();
        assert_eq!(encoded["value"]["count"], count.to_string());
        assert_eq!(encoded["value"]["method"], "rust-std-available-parallelism");
        assert_eq!(
            serde_json::from_value::<Acquired<ParallelismHint>>(encoded).unwrap(),
            acquired
        );
    }
    for count in [
        Ok(0),
        Err(AcquisitionReason::ParallelismQueryFailed),
        Err(AcquisitionReason::QuantityOverflow),
    ] {
        assert!(matches!(
            parallelism_hint(count),
            Acquired::Unavailable { .. }
        ));
    }
    let mut invalid = serde_json::to_value(parallelism_hint(Ok(1))).unwrap();
    invalid["value"]["count"] = json!("0");
    assert!(serde_json::from_value::<Acquired<ParallelismHint>>(invalid).is_err());
}

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
    let acquired = ObservedEnvironment::acquire(&clock).unwrap();
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
    let acquired = ObservedEnvironment::acquire(&clock).unwrap();
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
        assert!(
            serde_json::from_value::<EnvironmentInputs>(changed).is_err(),
            "{key}"
        );
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
