// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The formal 017 reference encoding in the complete 015 structural path.
//! Bodies, construction authority, and locale data remain outside this schema.

use std::sync::{Arc, OnceLock};

use serde_json::{json, Value};

use crate::input_limits::Bound;
use crate::materialize::{materialize_file, MaterializedDocument};
use crate::model::IntlifyConfig;
use crate::profile_fixtures::{complete_config as config, minimal_config, reference};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::{AuthoringSchema, StructuralAnalysis, StructuralLimits};

type Config = IntlifyConfig<PolicyReference, TargetProfileReference>;
type Analysis = StructuralAnalysis<PolicyReference, TargetProfileReference>;

const ARTIFACT: &str = include_str!("../schema/project-profile-config-v0.schema.json");
const FIELDS: [&str; 5] = [
    "kind",
    "identity",
    "revision",
    "specificationRevision",
    "semanticDigest",
];
const POLICIES: [(&str, &str); 7] = [
    ("resourceLimits", "resource-limit-policy"),
    ("trust", "trust-policy"),
    ("sourceAdmission", "source-admission-policy"),
    ("approval", "approval-policy"),
    ("selection", "selection-policy"),
    ("providerRouting", "provider-routing-policy"),
    ("glossarySet", "glossary-set"),
];

fn schema() -> &'static AuthoringSchema<PolicyReference, TargetProfileReference> {
    static SCHEMA: OnceLock<AuthoringSchema<PolicyReference, TargetProfileReference>> =
        OnceLock::new();
    SCHEMA.get_or_init(|| AuthoringSchema::for_model().unwrap())
}

fn validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        jsonschema::draft7::new(&serde_json::from_str::<Value>(ARTIFACT).unwrap()).unwrap()
    })
}

fn limits() -> StructuralLimits {
    StructuralLimits {
        max_profiles: Bound::new(8).unwrap(),
        max_profile_id_bytes: Bound::new(64).unwrap(),
        max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
    }
}

fn document(bytes: Vec<u8>) -> Arc<MaterializedDocument> {
    Arc::new(materialize_file(Arc::from(bytes), crate::materialize_tests::limits()).unwrap())
}

fn analyze(value: &Value) -> Analysis {
    schema()
        .analyze(document(serde_json::to_vec(value).unwrap()), limits())
        .unwrap()
}

fn agrees(value: Value, accepted: bool) -> Option<Config> {
    assert_eq!(validator().is_valid(&value), accepted, "schema: {value}");
    let analysis = analyze(&value);
    assert_eq!(analysis.is_complete(), accepted, "analysis: {value}");
    let result = analysis.construct().unwrap();
    assert_eq!(result.is_some(), accepted);
    drop(analysis);
    if let Some(config) = &result {
        assert_eq!(serde_json::to_value(config).unwrap(), value);
    }
    assert_eq!(
        crate::model::test_root_shape_accepts::<PolicyReference, TargetProfileReference>(value),
        accepted,
        "typed shape"
    );
    result
}

#[test]
fn formal_schema_is_closed_fresh_and_deterministic() {
    let generated = crate::schema::project_profile_config_schema().unwrap();
    assert_eq!(generated, schema().schema_body().clone());
    let text = crate::schema::format_schema(generated.clone()).unwrap();
    assert_eq!(text, ARTIFACT, "regenerate the project-profile schema");
    assert_eq!(
        text,
        crate::schema::format_schema(crate::schema::project_profile_config_schema().unwrap())
            .unwrap()
    );
    assert_eq!(
        generated["$schema"],
        "http://json-schema.org/draft-07/schema#"
    );
    assert!(generated.get("$id").is_none());
    assert_eq!(generated["additionalProperties"], false);
    assert!(!text.contains("$testPolicy"));
    assert!(!text.contains("$testTarget"));
    assert!(!text.contains("Fixture"));
    assert!(!text.contains("LocalizationProjectProfile"));
    for name in ["PolicyReference", "TargetProfileReference"] {
        let definition = &generated["definitions"][name];
        assert_eq!(definition["additionalProperties"], false);
        let actual: std::collections::BTreeSet<_> = definition["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| field.as_str().unwrap())
            .collect();
        assert_eq!(actual, FIELDS.into_iter().collect());
        assert_eq!(
            definition["properties"].as_object().unwrap().len(),
            FIELDS.len()
        );
    }
}

#[test]
fn all_declared_kinds_and_complete_profiles_use_the_same_structural_path() {
    for (slot, kind) in POLICIES {
        assert_eq!(
            config()["profiles"]["app"]["policies"][slot],
            reference(kind)
        );
    }
    agrees(config(), true);
    agrees(minimal_config(), true);
    let mut value = config();
    value["profiles"]["app"]["policies"]["providerRouting"] = Value::Null;
    value["profiles"]["app"]["policies"]["glossarySet"] = Value::Null;
    agrees(value, true);
}

#[test]
fn formal_root_has_no_deserialize_bypass() {
    trait AmbiguousIfDeserialize<Marker> {
        fn assert_unambiguous() {}
    }
    impl<T> AmbiguousIfDeserialize<()> for T {}
    impl<T: serde::de::DeserializeOwned> AmbiguousIfDeserialize<u8> for T {}
    let _ = <Config as AmbiguousIfDeserialize<_>>::assert_unambiguous;
}

#[test]
fn the_formal_schema_preserves_every_owned_field_and_closed_object() {
    for &(path, required, optional) in crate::model_tests::OWNED_FIELD_INVENTORY {
        let base = config();
        let actual: std::collections::BTreeSet<_> = base
            .pointer(path)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(actual, required.iter().chain(optional).copied().collect());
        for (fields, must_be_present) in [(required, true), (optional, false)] {
            for field in fields {
                let mut missing = base.clone();
                missing
                    .pointer_mut(path)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(*field);
                agrees(missing, !must_be_present);
                let mut wrong_type = base.clone();
                wrong_type.pointer_mut(path).unwrap()[*field] = json!(false);
                agrees(wrong_type, false);
            }
        }
        let mut unknown = base;
        unknown.pointer_mut(path).unwrap()["unknown"] = json!(true);
        agrees(unknown, false);
    }
}

#[test]
fn nullable_slots_empty_collections_and_fixed_variants_keep_015_rules() {
    for (path, invalid) in [
        ("/profiles", json!({})),
        ("/profiles/app/requestedLocales", json!([])),
        ("/profiles/app/targetProfiles", json!({})),
        ("/profiles/app/deploymentGroups", json!({})),
        ("/profiles/app/deploymentGroups/web/members", json!([])),
        (
            "/profiles/app/targetProfiles/browser/requestedLocales",
            json!([]),
        ),
        ("/profiles/app/coverage/rules/0/requestedLocales", json!([])),
        (
            "/profiles/app/coverage/rules/1/intentSurfaceClasses",
            json!([]),
        ),
        ("/profiles/app/coverage/rules/0/mode", json!("unknown")),
        ("/profiles/app/delivery/placement", json!("unknown")),
        ("/profiles/app/messageFallback/ja", json!([])),
        ("/profiles/app/messageFallback/ja/1/kind", json!("unknown")),
        ("/profiles/app/defaultSourceLocale", Value::Null),
        ("/profiles/app/defaultRequestedLocale", Value::Null),
        ("/profiles/app/policies/resourceLimits", Value::Null),
        ("/profiles/app/policies/trust", Value::Null),
    ] {
        let mut value = config();
        *value.pointer_mut(path).unwrap() = invalid;
        agrees(value, false);
    }
    for slot in ["providerRouting", "glossarySet"] {
        let mut value = config();
        value["profiles"]["app"]["policies"][slot] = Value::Null;
        agrees(value.clone(), true);
        value["profiles"]["app"]["policies"]
            .as_object_mut()
            .unwrap()
            .remove(slot);
        agrees(value, false);
    }
    for version in [json!(0), json!("1"), Value::Null] {
        let mut value = config();
        value["schemaVersion"] = version;
        agrees(value, false);
    }
}

#[test]
fn reference_fields_are_required_closed_and_typed() {
    for path in [
        "/profiles/app/policies/trust",
        "/profiles/app/targetProfiles/browser/profile",
    ] {
        for field in FIELDS {
            let mut missing = config();
            missing
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(field);
            agrees(missing, false);
            for invalid in [Value::Null, json!(false), json!(0), json!({}), json!([])] {
                let mut wrong_type = config();
                wrong_type.pointer_mut(path).unwrap()[field] = invalid;
                agrees(wrong_type, false);
            }
        }
        let mut extra = config();
        extra.pointer_mut(path).unwrap()["url"] = json!("https://invalid.example/policy");
        agrees(extra, false);
    }
}

#[test]
fn exact_tokens_have_no_normalization_or_selector_coercion() {
    for field in ["identity", "revision", "specificationRevision"] {
        for invalid in [
            "",
            "UPPER",
            " leading",
            "trailing ",
            "a/b",
            ".a",
            "a-",
            "日本語",
            "^1",
            ">=1",
            "*",
        ] {
            let mut value = config();
            value["profiles"]["app"]["policies"]["trust"][field] = json!(invalid);
            agrees(value, false);
        }
        for ending in ["\n", "\r", "\r\n", "\t", "\0", "\u{2028}", "\u{2029}"] {
            let mut value = config();
            value["profiles"]["app"]["policies"]["trust"][field] = json!(format!("a{ending}"));
            agrees(value, false);
        }
        for valid in ["0", "1", "r1", "1.0.0", "release-1", "a_b.c-9"] {
            let mut value = config();
            value["profiles"]["app"]["policies"]["trust"][field] = json!(valid);
            agrees(value, true);
        }
    }
}

#[test]
fn semantic_digest_is_an_exact_full_sha256_pin_without_hashing_the_body() {
    let valid = format!("sha256:{}", "0123456789abcdef".repeat(4));
    for invalid in [
        String::new(),
        "sha256:".into(),
        format!("sha256:{}", "a".repeat(63)),
        format!("sha256:{}", "a".repeat(65)),
        format!("sha256:{}", "A".repeat(64)),
        format!("sha256:{}", "g".repeat(64)),
        format!("sha256:{}", "é".repeat(32)),
        valid.replace("sha256:", "SHA256:"),
        valid.replace("sha256:", "blake3:"),
        format!("{valid}\n"),
        format!("{valid}\r\n"),
        format!(" {valid}"),
    ] {
        let mut value = config();
        value["profiles"]["app"]["policies"]["trust"]["semanticDigest"] = json!(invalid);
        agrees(value, false);
    }
    let mut value = config();
    value["profiles"]["app"]["policies"]["trust"]["semanticDigest"] = json!(valid);
    agrees(value, true);
}

#[test]
fn schema_admission_does_not_claim_kind_role_or_semantic_version_support() {
    let mut value = config();
    value["profiles"]["app"]["policies"]["trust"] = reference("selection-policy");
    value["profiles"]["app"]["policies"]["approval"]["specificationRevision"] = json!("999");
    agrees(value, true);
    for (path, invalid_kind) in [
        ("/profiles/app/policies/trust", "target-profile"),
        ("/profiles/app/policies/trust", "unknown-policy"),
        (
            "/profiles/app/targetProfiles/browser/profile",
            "trust-policy",
        ),
    ] {
        let mut value = config();
        value.pointer_mut(path).unwrap()["kind"] = json!(invalid_kind);
        agrees(value, false);
    }
}

#[test]
fn exact_equality_retains_each_field_even_when_content_is_equal() {
    let base = reference("trust-policy");
    let admitted: PolicyReference = serde_json::from_value(base.clone()).unwrap();
    for (field, different) in [
        ("kind", json!("selection-policy")),
        ("identity", json!("other-identity")),
        ("revision", json!("2")),
        ("specificationRevision", json!("1")),
        (
            "semanticDigest",
            json!(format!("sha256:{}", "2".repeat(64))),
        ),
    ] {
        let mut changed = base.clone();
        changed[field] = different;
        assert_ne!(
            admitted,
            serde_json::from_value::<PolicyReference>(changed).unwrap()
        );
    }
}

#[test]
fn invalid_reference_withholds_root_and_preserves_independent_fields() {
    let mut value = config();
    value["profiles"]["bad"] = value["profiles"]["app"].clone();
    value["profiles"]["bad"]["policies"]["trust"]["semanticDigest"] = json!("private-invalid");
    let analysis = analyze(&value);
    assert!(analysis.construct().unwrap().is_none());
    assert!(analysis.profile("app").unwrap().is_some());
    assert!(analysis.profile("bad").unwrap().is_none());
    assert!(analysis
        .profile_field::<PolicyReference>("bad", &["policies", "trust"])
        .unwrap()
        .is_none());
    assert_eq!(
        analysis
            .profile_field::<String>("bad", &["defaultRequestedLocale"])
            .unwrap()
            .as_deref(),
        Some("en")
    );
}

#[test]
fn raw_entry_rejects_duplicate_reference_members_before_object_materialization() {
    let text = serde_json::to_string(&config()).unwrap();
    for field in FIELDS {
        let key = format!("\"{field}\":");
        let duplicate = text.replacen(&key, &format!("\"{field}\":null,{key}"), 1);
        assert!(materialize_file(
            Arc::from(duplicate.into_bytes()),
            crate::materialize_tests::limits()
        )
        .is_err());
    }
    let escaped = text.replace("sha256:", "\\u0073ha256:");
    let analysis = schema()
        .analyze(document(escaped.into_bytes()), limits())
        .unwrap();
    assert_eq!(
        serde_json::to_value(analysis.construct().unwrap().unwrap()).unwrap(),
        config()
    );
}

#[test]
fn formal_reference_strings_remain_subject_to_entry_capacity() {
    use crate::input_limits::{CountRelation, InputBound};
    use crate::materialize::InputFailure;

    let source: Arc<[u8]> = Arc::from(serde_json::to_vec(&config()).unwrap());
    let mut capacity = crate::materialize_tests::limits();
    // The longest fixture strings are the 7 + 64 byte SHA-256 pins.
    capacity.value.max_single_string_bytes = Bound::new(71).unwrap();
    let doc = materialize_file(Arc::clone(&source), capacity).unwrap();
    assert!(schema()
        .analyze(Arc::new(doc), limits())
        .unwrap()
        .is_complete());
    capacity.value.max_single_string_bytes = Bound::new(70).unwrap();
    let Err(error) = materialize_file(source, capacity) else {
        panic!("first-over input must not materialize")
    };
    let InputFailure::ResourceLimits(violations) = error.reason else {
        panic!("input bound expected")
    };
    assert_eq!(violations.len(), 1);
    assert_eq!(violations[0].bound, InputBound::SingleStringBytes);
    assert_eq!(violations[0].actual, 71);
    assert_eq!(violations[0].relation, CountRelation::Exact);
}

#[test]
fn formal_schema_work_remains_bounded_before_root_construction() {
    let doc = document(serde_json::to_vec(&config()).unwrap());
    let baseline = schema().analyze(Arc::clone(&doc), limits()).unwrap();
    let units = baseline.structural_units().unwrap();
    let mut capacity = limits();
    capacity.max_structural_analysis_units = Bound::new(units).unwrap();
    assert!(schema()
        .analyze(Arc::clone(&doc), capacity)
        .unwrap()
        .construct()
        .unwrap()
        .is_some());
    capacity.max_structural_analysis_units = Bound::new(units - 1).unwrap();
    let failed = schema().analyze(doc, capacity).unwrap();
    assert!(failed.construct().unwrap().is_none());
    assert_eq!(failed.issues().len(), 1);
    assert_eq!(
        failed.issues()[0].reason,
        crate::structural::StructuralFailure::StructuralWorkLimit {
            limit: capacity.max_structural_analysis_units,
            actual: units
        }
    );
}

#[test]
fn formal_references_reach_the_same_private_locale_core_without_body_resolution() {
    use crate::locale::core::{Input, Limits};
    use crate::locale::fixtures::{fixture_binding, FixtureProvider};
    use crate::locale::Canonicalizer;
    use crate::model::Presence;
    use crate::structural::selection::{Selection, SelectorInput};

    let mut value = config();
    value["profiles"]["app"]
        .as_object_mut()
        .unwrap()
        .remove("defaultSourceLocale");
    value["profiles"]["app"]["requestedLocales"] = json!(["EN-us", "ja"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("EN-us");
    let analysis = analyze(&value);
    let configuration = analysis.construct().unwrap().unwrap();
    let Selection::Selected(selected) = analysis
        .select(&SelectorInput::absent(limits().max_profile_id_bytes))
        .unwrap()
    else {
        panic!("expected single admitted profile")
    };
    assert!(matches!(
        configuration
            .profiles()
            .values()
            .next()
            .unwrap()
            .default_source_locale,
        Presence::Absent
    ));
    let provider = Canonicalizer::bind(
        &fixture_binding(),
        Some(FixtureProvider::new()),
        Bound::new(128).unwrap(),
    )
    .unwrap();
    let result = Input::from_selected(&configuration, selected.id())
        .unwrap()
        .resolve(
            &provider,
            Limits {
                max_active_occurrences: Bound::new(16).unwrap(),
                max_requested_locales: Bound::new(8).unwrap(),
            },
        );
    let core = result.value().unwrap();
    assert!(core.source_default().is_none());
    assert_eq!(
        core.requested()
            .iter()
            .map(crate::locale::CanonicalLocale::as_str)
            .collect::<Vec<_>>(),
        ["en-US", "ja"]
    );
    assert_eq!(core.requested_default().as_str(), "en-US");
}
