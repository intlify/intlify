// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::{Arc, OnceLock};

use serde_json::{json, Value};

use crate::fixtures::{
    complete_config, minimal_config, FixtureConfig, FixturePolicyReference, FixtureTargetReference,
};
use crate::input_limits::Bound;
use crate::materialize::{materialize_file, MaterializedDocument};
use crate::model::{NonEmptyVec, Presence};

use super::{AuthoringSchema, StructuralAnalysis, StructuralFailure, StructuralLimits};

pub(crate) type FixtureAnalysis =
    StructuralAnalysis<FixturePolicyReference, FixtureTargetReference>;
type FixtureSchema = AuthoringSchema<FixturePolicyReference, FixtureTargetReference>;

pub(super) fn limits() -> StructuralLimits {
    // Explicit finite test capacity, never an implicit production default.
    StructuralLimits {
        max_profiles: Bound::new(100).unwrap(),
        max_profile_id_bytes: Bound::new(256).unwrap(),
        max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
    }
}

fn document(value: &Value) -> Arc<MaterializedDocument> {
    Arc::new(
        materialize_file(
            Arc::from(serde_json::to_vec(value).unwrap()),
            crate::materialize_tests::limits(),
        )
        .unwrap(),
    )
}

pub(crate) fn analyze_fixture(value: &Value) -> FixtureAnalysis {
    analyze_with_limits(value, limits())
}

pub(super) fn analyze_with_limits(value: &Value, capacity: StructuralLimits) -> FixtureAnalysis {
    // Test setup only: share an immutable schema, never invocation state.
    static SCHEMA: OnceLock<FixtureSchema> = OnceLock::new();
    SCHEMA
        .get_or_init(|| FixtureSchema::for_model().unwrap())
        .analyze(document(value), capacity)
        .unwrap()
}

#[test]
fn complete_root_has_no_raw_deserialize_route() {
    // If Deserialize is accidentally added to the root, both implementations
    // become applicable and this assertion no longer compiles.
    trait AmbiguousIfDeserialize<Marker> {
        fn assert_unambiguous() {}
    }
    impl<T> AmbiguousIfDeserialize<()> for T {}
    impl<T: serde::de::DeserializeOwned> AmbiguousIfDeserialize<u8> for T {}
    let _ = <FixtureConfig as AmbiguousIfDeserialize<_>>::assert_unambiguous;
}

#[test]
fn complete_owned_root_requires_all_admission_prerequisites() {
    for value in [minimal_config(), complete_config()] {
        let analysis = analyze_fixture(&value);
        assert!(analysis.version_selected());
        assert!(analysis.is_complete());
        assert!(analysis.issues().is_empty());
        assert_eq!(analysis.schema_issue_count(), 0);
        let config = analysis.construct().unwrap().unwrap();
        drop(analysis);
        assert_eq!(serde_json::to_value(config).unwrap(), value);
    }
}

#[test]
fn version_failure_suppresses_schema_body_work_without_fallback() {
    for (value, reason) in [
        (json!([]), StructuralFailure::RootTypeInvalid),
        (
            json!({"profiles":false}),
            StructuralFailure::SchemaVersionMissing,
        ),
        (
            json!({"schemaVersion":0, "profiles":false}),
            StructuralFailure::SchemaVersionTypeInvalid,
        ),
        (
            json!({"schemaVersion":"future-private-version", "profiles":false}),
            StructuralFailure::SchemaVersionUnsupported,
        ),
    ] {
        let analysis = analyze_fixture(&value);
        assert!(!analysis.version_selected());
        assert_eq!(analysis.structural_units(), None);
        assert_eq!(analysis.issues().len(), 1);
        assert_eq!(analysis.issues()[0].reason, reason);
        assert!(!format!("{:?}", analysis.issues()).contains("future-private-version"));
        assert!(analysis.construct().unwrap().is_none());
        assert!(analysis.profile("app").unwrap().is_none());
    }
}

#[test]
fn schema_metadata_never_selects_or_fetches_an_authority() {
    let mut value = minimal_config();
    value["$schema"] = json!("https://invalid.example/private-other-schema.json");
    let analysis = analyze_fixture(&value);
    assert!(analysis.is_complete());
    assert!(matches!(
        analysis.construct().unwrap().unwrap().schema(),
        Presence::Present(_)
    ));
    value.as_object_mut().unwrap().remove("schemaVersion");
    let analysis = analyze_fixture(&value);
    assert_eq!(
        analysis.issues()[0].reason,
        StructuralFailure::SchemaVersionMissing
    );
    assert!(analysis.structural_units().is_none());
}

#[test]
fn invalid_metadata_or_sibling_withholds_root_but_retains_independent_typed_fields() {
    let mut value = complete_config();
    value["$schema"] = json!(false);
    value["profiles"]["bad"] = value["profiles"]["app"].clone();
    value["profiles"]["bad"]["coverage"] = json!(false);
    let analysis = analyze_fixture(&value);
    assert!(!analysis.is_complete());
    assert!(analysis.construct().unwrap().is_none());
    assert!(analysis.profile("app").unwrap().is_some());
    assert!(analysis.profile("bad").unwrap().is_none());
    let source: Option<String> = analysis
        .profile_field("bad", &["defaultSourceLocale"])
        .unwrap();
    assert_eq!(source.as_deref(), Some("en"));
    let requested: Option<NonEmptyVec<String>> = analysis
        .profile_field("bad", &["requestedLocales"])
        .unwrap();
    assert_eq!(requested.unwrap().as_slice(), ["en"]);
    let resource: Option<FixturePolicyReference> = analysis
        .profile_field("bad", &["policies", "resourceLimits"])
        .unwrap();
    assert!(resource.is_some());
}

#[test]
fn a_passing_alternative_does_not_admit_an_invalid_enclosing_field() {
    let mut value = complete_config();
    value["profiles"]["app"]["coverage"]["rules"][0]["unknown"] = json!(true);
    let analysis = analyze_fixture(&value);
    assert!(!analysis.is_complete());
    assert!(analysis
        .profile_field::<crate::model::Coverage>("app", &["coverage"])
        .unwrap()
        .is_none());
    assert!(analysis
        .profile_field::<Vec<crate::model::CoverageRule>>("app", &["coverage", "rules"])
        .unwrap()
        .is_none());
    assert!(analysis.construct().unwrap().is_none());
}

#[test]
fn profile_count_and_id_bounds_accept_exact_and_report_all_independent_overruns() {
    let mut value = minimal_config();
    value["profiles"]["bpp"] = value["profiles"]["app"].clone();
    let schema = FixtureSchema::for_model().unwrap();
    let doc = document(&value);
    let mut capacity = limits();
    capacity.max_profiles = Bound::new(2).unwrap();
    capacity.max_profile_id_bytes = Bound::new(3).unwrap();
    assert!(schema
        .analyze(Arc::clone(&doc), capacity)
        .unwrap()
        .is_complete());
    capacity.max_profiles = Bound::new(1).unwrap();
    capacity.max_profile_id_bytes = Bound::new(2).unwrap();
    let analysis = schema.analyze(doc, capacity).unwrap();
    assert!(!analysis.is_complete());
    assert_eq!(analysis.issues().len(), 3);
    assert_eq!(
        analysis.issues()[0].reason,
        StructuralFailure::ProfilesLimit {
            limit: capacity.max_profiles,
            actual: 2
        }
    );
    for issue in &analysis.issues()[1..] {
        assert_eq!(
            issue.reason,
            StructuralFailure::ProfileIdLimit {
                limit: capacity.max_profile_id_bytes,
                actual: 3
            }
        );
    }
    assert!(analysis.profile("app").unwrap().is_none());
    assert!(analysis.construct().unwrap().is_none());
}

#[test]
fn id_overrun_prevents_key_validation_and_descendant_work_but_not_other_profiles() {
    let mut value = minimal_config();
    value["profiles"]["PRIVATE-KEY"] = json!({"unknown":"private-value"});
    let mut capacity = limits();
    capacity.max_profile_id_bytes = Bound::new(3).unwrap();
    let analysis = FixtureSchema::for_model()
        .unwrap()
        .analyze(document(&value), capacity)
        .unwrap();
    assert_eq!(analysis.issues().len(), 1);
    assert!(matches!(
        analysis.issues()[0].reason,
        StructuralFailure::ProfileIdLimit { actual: 11, .. }
    ));
    assert_eq!(analysis.schema_issue_count(), 0);
    assert!(analysis.profile("app").unwrap().is_some());
    assert!(analysis.construct().unwrap().is_none());
    assert!(!format!("{:?}", analysis.issues()).contains("PRIVATE-KEY"));
}

#[test]
fn raw_id_bytes_are_counted_before_syntax_admission_and_escape_spelling_is_not_the_unit() {
    let mut value = minimal_config();
    value["profiles"]["日本"] = value["profiles"]["app"].clone();
    let mut capacity = limits();
    capacity.max_profile_id_bytes = Bound::new(5).unwrap();
    let schema = FixtureSchema::for_model().unwrap();
    let analysis = schema.analyze(document(&value), capacity).unwrap();
    assert_eq!(
        analysis.issues()[0].reason,
        StructuralFailure::ProfileIdLimit {
            limit: capacity.max_profile_id_bytes,
            actual: 6
        }
    );
    let raw = serde_json::to_string(&value)
        .unwrap()
        .replace("日本", "\\u65e5\\u672c");
    let doc = Arc::new(
        materialize_file(
            Arc::from(raw.into_bytes()),
            crate::materialize_tests::limits(),
        )
        .unwrap(),
    );
    let escaped = schema.analyze(doc, capacity).unwrap();
    assert_eq!(analysis.issues()[0].reason, escaped.issues()[0].reason);
    assert!(escaped.construct().unwrap().is_none());
}

#[test]
fn work_overrun_withholds_even_a_structurally_valid_configuration() {
    let schema = FixtureSchema::for_model().unwrap();
    let doc = document(&complete_config());
    let expected = schema
        .analyze(Arc::clone(&doc), limits())
        .unwrap()
        .structural_units()
        .unwrap();
    let mut capacity = limits();
    capacity.max_structural_analysis_units = Bound::new(expected).unwrap();
    assert!(schema
        .analyze(Arc::clone(&doc), capacity)
        .unwrap()
        .construct()
        .unwrap()
        .is_some());
    capacity.max_structural_analysis_units = Bound::new(expected - 1).unwrap();
    let rejected = schema.analyze(doc, capacity).unwrap();
    assert_eq!(
        rejected.issues()[0].reason,
        StructuralFailure::StructuralWorkLimit {
            limit: capacity.max_structural_analysis_units,
            actual: expected
        }
    );
    assert!(rejected.construct().unwrap().is_none());
    assert!(rejected.profile("app").unwrap().is_none());
}

#[test]
fn malformed_profiles_shape_never_creates_a_profile_or_implicit_selection() {
    for profiles in [json!(null), json!([]), json!(false), json!({})] {
        let analysis = analyze_fixture(&json!({"schemaVersion":"0", "profiles":profiles}));
        assert!(analysis.version_selected());
        assert!(!analysis.is_complete());
        assert!(analysis.construct().unwrap().is_none());
        assert!(analysis.profile("app").unwrap().is_none());
    }
}

#[test]
fn result_owns_input_and_binding_and_repeated_invocations_do_not_share_state() {
    let schema = FixtureSchema::for_model().unwrap();
    let doc = document(&minimal_config());
    let first = schema.analyze(Arc::clone(&doc), limits()).unwrap();
    drop(doc);
    assert!(!schema
        .analyze(document(&json!({})), limits())
        .unwrap()
        .is_complete());
    let mut lower = limits();
    lower.max_profile_id_bytes = Bound::new(1).unwrap();
    assert!(!schema
        .analyze(document(&minimal_config()), lower)
        .unwrap()
        .is_complete());
    let repeated = schema
        .analyze(document(&minimal_config()), limits())
        .unwrap();
    drop(schema);
    assert_eq!(first.structural_units(), repeated.structural_units());
    assert_eq!(first.construct().unwrap(), repeated.construct().unwrap());
}
