// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde_json::json;

use super::*;

#[test]
fn the_compiled_build_observation_is_owned_and_excludes_runtime_checkout_state() {
    let build = ObservedBuild::acquire().unwrap();
    let value = serde_json::to_value(build.document()).unwrap();
    assert_eq!(value["codec"], "intlify-config-build-observation/0");
    assert_eq!(value["package"]["identity"], "intlify_config");
    assert_eq!(value["package"]["revision"], env!("CARGO_PKG_VERSION"));
    assert_eq!(value["source"]["state"], "observed");
    assert_eq!(value["dependencyLock"]["state"], "observed");
    // Wrappers or an unsupported compiler's metadata remain explicit. They
    // do not force this observational test to claim an unwrapped rustc build.
    match &build.document().compiler {
        Acquisition::Observed { value } => assert_eq!(value.identity, "rustc"),
        Acquisition::Unavailable { reason } => assert!(matches!(
            reason,
            AcquisitionReason::MissingInput
                | AcquisitionReason::CompilerInvocationFailed
                | AcquisitionReason::CompilerOutputUnsupported
                | AcquisitionReason::CompilerOutputLimit
                | AcquisitionReason::CompilerWrappersPresent
        )),
    }
    assert_eq!(value["effectiveConfiguration"]["state"], "unavailable");
    assert_eq!(
        value["effectiveConfiguration"]["reason"],
        "effective-invocation-not-attested"
    );
    assert_eq!(
        value["execution"]["debugAssertions"],
        cfg!(debug_assertions)
    );
    assert!(value["cargoInputs"]["features"]
        .as_array()
        .unwrap()
        .contains(&json!("benchmark")));
    let encoded = serde_json::to_vec(build.document()).unwrap();
    let decoded: BuildObservation = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded, *build.document());
    assert!(build.validate(&decoded).is_empty());
    // No path, username, environment dump, Git command, or runtime source read
    // contributes to this record. It retains the build-time observation.
    let text = String::from_utf8(encoded).unwrap();
    assert!(!text.contains(env!("CARGO_MANIFEST_DIR")));
    assert!(!text.contains("RUSTFLAGS"));
    assert!(!text.contains("HOME"));
    assert_eq!(
        build.checksum(),
        ObservedBuild::acquire().unwrap().checksum()
    );
}

#[test]
fn changed_metadata_is_not_accepted_as_the_executing_build() {
    let build = ObservedBuild::acquire().unwrap();
    let original = serde_json::to_value(build.document()).unwrap();
    for (pointer, replacement) in [
        ("/package/revision", json!("different-build")),
        ("/source/value/digest", json!("0".repeat(64))),
        ("/dependencyLock/value/digest", json!("0".repeat(64))),
        (
            "/compiler",
            json!({"state": "observed", "value": {"identity": "rustc", "release": "0.0.0", "commit": null, "llvm": "0.0.0"}}),
        ),
        ("/execution/debugAssertions", json!(!cfg!(debug_assertions))),
        (
            "/cargoInputs/additionalFlagsPresent",
            json!(!original["cargoInputs"]["additionalFlagsPresent"]
                .as_bool()
                .unwrap()),
        ),
    ] {
        let mut changed = original.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        let decoded: BuildObservation = serde_json::from_value(changed).unwrap();
        assert!(!build.validate(&decoded).is_empty(), "{pointer}");
    }
    for pointer in [
        "/codec",
        "/package",
        "/source",
        "/dependencyLock",
        "/compiler",
        "/cargoInputs",
        "/effectiveConfiguration",
        "/executable",
        "/execution",
    ] {
        let mut changed = original.clone();
        changed
            .as_object_mut()
            .unwrap()
            .remove(pointer.trim_start_matches('/'));
        assert!(serde_json::from_value::<BuildObservation>(changed).is_err());
    }
}
