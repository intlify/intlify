// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015-008/009/038 structural fixtures; semantic checks belong to later steps.
//!
//! The generic reference arguments are finite test encodings, not a production
//! configuration schema or a Profile Specification conformance claim.

use std::sync::OnceLock;

use serde_json::{json, Value};

use crate::fixtures::{complete_config, minimal_config, FixtureConfig};
use crate::model::{Presence, RequiredNullable, CONFIGURATION_SCHEMA_VERSION};

fn schema_validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema = crate::schema::draft7_schema::<FixtureConfig>().unwrap();
        jsonschema::draft7::new(&schema).expect("valid test-owned Draft 7 schema")
    })
}

// Every fixed 015-owned object's complete field inventory is represented here.
// The fixture reference rows explicitly belong to tests rather than to 017.
#[test]
fn field_inventory_matches_required_optional_and_wrong_type_behavior() {
    let inventory: &[(&str, &[&str], &[&str])] = &[
        ("", &["schemaVersion", "profiles"], &["$schema"]),
        (
            "/profiles/app",
            &[
                "projectId",
                "selectionScope",
                "requestedLocales",
                "defaultRequestedLocale",
                "policies",
                "targetProfiles",
                "deploymentGroups",
            ],
            &[
                "defaultSourceLocale",
                "localeNegotiation",
                "messageFallback",
                "coverage",
                "delivery",
            ],
        ),
        ("/profiles/app/localeNegotiation", &[], &["aliases"]),
        ("/profiles/app/coverage", &[], &["defaultMode", "rules"]),
        (
            "/profiles/app/coverage/rules/2",
            &["mode"],
            &["requestedLocales", "intentSurfaceClasses"],
        ),
        (
            "/profiles/app/policies",
            &[
                "resourceLimits",
                "trust",
                "sourceAdmission",
                "approval",
                "selection",
                "providerRouting",
                "glossarySet",
            ],
            &[],
        ),
        (
            "/profiles/app/targetProfiles/browser",
            &["profile", "requestedLocales"],
            &["defaultRequestedLocale"],
        ),
        (
            "/profiles/app/deploymentGroups/web",
            &["members"],
            &["hydrationRelations"],
        ),
        (
            "/profiles/app/deploymentGroups/web/hydrationRelations/0",
            &["server", "client"],
            &[],
        ),
        ("/profiles/app/messageFallback/ja/1", &["kind"], &[]),
        ("/profiles/app/delivery", &[], &["placement"]),
        ("/profiles/app/policies/trust", &["$testPolicy"], &[]),
        (
            "/profiles/app/targetProfiles/browser/profile",
            &["$testTarget"],
            &[],
        ),
    ];

    for &(pointer, required, optional) in inventory {
        let full = complete_config();
        let actual: std::collections::BTreeSet<_> = full
            .pointer(pointer)
            .unwrap()
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let expected: std::collections::BTreeSet<_> =
            required.iter().chain(optional).copied().collect();
        assert_eq!(
            actual, expected,
            "incomplete fixture inventory at {pointer}"
        );

        for (fields, should_require) in [(required, true), (optional, false)] {
            for &field in fields {
                let mut missing = full.clone();
                missing
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(field);
                if should_require {
                    rejected(missing);
                } else {
                    admitted(missing);
                }

                let mut wrong_type = full.clone();
                wrong_type.pointer_mut(pointer).unwrap()[field] = json!(false);
                rejected(wrong_type);
            }
        }
    }
}

#[test]
fn an_invalid_sibling_never_constructs_a_partial_root() {
    let mut value = minimal_config();
    value["profiles"]["other"] = value["profiles"]["app"].clone();
    value["profiles"]["other"]
        .as_object_mut()
        .unwrap()
        .remove("policies");
    rejected(value);
}

#[test]
fn schema_and_runtime_identity_patterns_agree_on_control_character_boundaries() {
    for suffix in ["\n", "\r", "\r\n", "\t", "\0", " ", "\u{2028}"] {
        let mut value = minimal_config();
        value["profiles"]["app"]["projectId"] = json!(format!("app{suffix}"));
        rejected(value);
    }
}

fn admitted(value: Value) -> FixtureConfig {
    assert!(
        schema_validator().is_valid(&value),
        "schema rejected positive structural fixture"
    );
    serde_json::from_value(value).expect("complete structural fixture")
}

fn rejected(value: Value) {
    assert!(
        !schema_validator().is_valid(&value),
        "schema accepted negative structural fixture"
    );
    assert!(serde_json::from_value::<FixtureConfig>(value).is_err());
}

#[test]
fn generated_fixture_schema_is_deterministic_and_keeps_its_test_identity() {
    let first = crate::schema::draft7_schema::<FixtureConfig>().unwrap();
    let second = crate::schema::draft7_schema::<FixtureConfig>().unwrap();
    assert_eq!(first["$schema"], "http://json-schema.org/draft-07/schema#");
    assert!(first.get("$id").is_none());
    assert!(first.get("$defs").is_none());
    assert_eq!(first["additionalProperties"], false);
    assert_eq!(first["required"], json!(["schemaVersion", "profiles"]));
    let bytes = crate::schema::format_schema(first).unwrap();
    assert_eq!(bytes, crate::schema::format_schema(second).unwrap());
    assert!(bytes.contains("$testPolicy"));
    assert!(bytes.contains("$testTarget"));
    assert!(!bytes.contains("LocalizationProjectProfile"));
}

#[test]
fn complete_and_minimum_shapes_are_admitted_without_semantic_defaults() {
    let minimum = admitted(minimal_config());
    assert_eq!(
        minimum.schema_version.as_str(),
        CONFIGURATION_SCHEMA_VERSION
    );
    let declaration = minimum.profiles.values().next().unwrap();
    assert!(matches!(
        declaration.default_source_locale,
        Presence::Absent
    ));
    assert!(matches!(
        declaration.policies.provider_routing,
        RequiredNullable::Null
    ));
    assert_eq!(serde_json::to_value(minimum).unwrap(), minimal_config());
    assert_eq!(
        serde_json::to_value(admitted(complete_config())).unwrap(),
        complete_config()
    );
}

#[test]
fn root_version_and_shape_are_closed() {
    for invalid in [
        Value::Null,
        json!([]),
        json!("config"),
        json!(false),
        json!(1),
    ] {
        rejected(invalid);
    }
    for version in [json!(0), json!("1"), json!("00"), Value::Null] {
        let mut value = minimal_config();
        value["schemaVersion"] = version;
        rejected(value);
    }
    for key in ["schemaVersion", "profiles"] {
        let mut value = minimal_config();
        value.as_object_mut().unwrap().remove(key);
        rejected(value);
    }
    for profiles in [json!({}), json!([]), Value::Null] {
        let mut value = minimal_config();
        value["profiles"] = profiles;
        rejected(value);
    }
    for legacy in [
        "fmt",
        "lint",
        "resources",
        "messages",
        "profile",
        "selector",
    ] {
        let mut value = minimal_config();
        value[legacy] = json!({});
        rejected(value);
    }
}

#[test]
fn each_required_declaration_member_rejects_omission() {
    for key in [
        "projectId",
        "selectionScope",
        "requestedLocales",
        "defaultRequestedLocale",
        "policies",
        "targetProfiles",
        "deploymentGroups",
    ] {
        let mut value = minimal_config();
        value["profiles"]["app"]
            .as_object_mut()
            .unwrap()
            .remove(key);
        rejected(value);
    }
}

#[test]
fn every_optional_member_rejects_null() {
    for pointer in [
        "/$schema",
        "/profiles/app/defaultSourceLocale",
        "/profiles/app/localeNegotiation",
        "/profiles/app/localeNegotiation/aliases",
        "/profiles/app/messageFallback",
        "/profiles/app/coverage",
        "/profiles/app/coverage/defaultMode",
        "/profiles/app/coverage/rules",
        "/profiles/app/coverage/rules/0/requestedLocales",
        "/profiles/app/coverage/rules/1/intentSurfaceClasses",
        "/profiles/app/targetProfiles/browser/defaultRequestedLocale",
        "/profiles/app/deploymentGroups/web/hydrationRelations",
        "/profiles/app/delivery",
        "/profiles/app/delivery/placement",
    ] {
        let mut value = complete_config();
        *value.pointer_mut(pointer).unwrap() = Value::Null;
        rejected(value);
    }
}

#[test]
fn all_policy_members_are_required_and_only_two_are_nullable() {
    for name in [
        "resourceLimits",
        "trust",
        "sourceAdmission",
        "approval",
        "selection",
        "providerRouting",
        "glossarySet",
    ] {
        let mut value = minimal_config();
        value["profiles"]["app"]["policies"]
            .as_object_mut()
            .unwrap()
            .remove(name);
        rejected(value);
        let mut value = complete_config();
        value["profiles"]["app"]["policies"][name] = Value::Null;
        if matches!(name, "providerRouting" | "glossarySet") {
            admitted(value);
        } else {
            rejected(value);
        }
    }
}

#[test]
fn fixed_objects_reject_unknown_members_at_every_level() {
    for pointer in [
        "",
        "/profiles/app",
        "/profiles/app/localeNegotiation",
        "/profiles/app/coverage",
        "/profiles/app/coverage/rules/0",
        "/profiles/app/policies",
        "/profiles/app/targetProfiles/browser",
        "/profiles/app/deploymentGroups/web",
        "/profiles/app/deploymentGroups/web/hydrationRelations/0",
        "/profiles/app/delivery",
        "/profiles/app/messageFallback/ja/1",
        "/profiles/app/policies/trust",
        "/profiles/app/targetProfiles/browser/profile",
    ] {
        let mut value = complete_config();
        value
            .pointer_mut(pointer)
            .unwrap()
            .as_object_mut()
            .unwrap()
            .insert("unknown".into(), json!(true));
        rejected(value);
    }
}

#[test]
fn required_collections_and_present_selectors_are_nonempty() {
    for pointer in [
        "/profiles/app/requestedLocales",
        "/profiles/app/targetProfiles/browser/requestedLocales",
        "/profiles/app/deploymentGroups/web/members",
        "/profiles/app/messageFallback/ja",
        "/profiles/app/coverage/rules/0/requestedLocales",
        "/profiles/app/coverage/rules/1/intentSurfaceClasses",
    ] {
        let mut value = complete_config();
        *value.pointer_mut(pointer).unwrap() = json!([]);
        rejected(value);
    }
    for pointer in [
        "/profiles",
        "/profiles/app/targetProfiles",
        "/profiles/app/deploymentGroups",
    ] {
        let mut value = complete_config();
        *value.pointer_mut(pointer).unwrap() = json!({});
        rejected(value);
    }
}

#[test]
fn empty_optional_objects_keep_authoring_absence_separate_from_semantic_defaults() {
    for key in [
        "localeNegotiation",
        "messageFallback",
        "coverage",
        "delivery",
    ] {
        let mut value = minimal_config();
        value["profiles"]["app"][key] = json!({});
        assert_eq!(
            serde_json::to_value(admitted(value.clone())).unwrap(),
            value
        );
    }
    let mut value = minimal_config();
    value["profiles"]["app"]["localeNegotiation"] = json!({"aliases": {}});
    value["profiles"]["app"]["coverage"] = json!({"rules": []});
    value["profiles"]["app"]["deploymentGroups"]["web"]["hydrationRelations"] = json!([]);
    assert_eq!(
        serde_json::to_value(admitted(value.clone())).unwrap(),
        value
    );
}

#[test]
fn coverage_requires_a_mode_and_at_least_one_constrained_dimension() {
    for rule in [
        json!({}),
        json!({"mode": "direct-required"}),
        json!({"requestedLocales": ["en"]}),
        json!({"requestedLocales": ["en"], "mode": "source-equal"}),
    ] {
        let mut value = minimal_config();
        value["profiles"]["app"]["coverage"] = json!({"rules": [rule]});
        rejected(value);
    }
}

#[test]
fn fixed_tags_and_modes_have_no_compatibility_aliases() {
    for (pointer, replacement) in [
        ("/profiles/app/delivery/placement", json!("hoist")),
        ("/profiles/app/coverage/defaultMode", json!("ignore")),
        ("/profiles/app/messageFallback/ja/1/kind", json!("source")),
        (
            "/profiles/app/messageFallback/ja/1",
            json!({"kind": "intent-source-locale", "locale": "en"}),
        ),
    ] {
        let mut value = complete_config();
        *value.pointer_mut(pointer).unwrap() = replacement;
        rejected(value);
    }
}

#[test]
fn identical_profiles_remain_independent_named_declarations() {
    let mut value = minimal_config();
    value["profiles"]["other-app"] = value["profiles"]["app"].clone();
    let config = admitted(value);
    assert_eq!(config.profiles.len(), 2);
}

#[test]
fn semantic_duplicates_and_references_are_not_silently_normalized() {
    let mut value = complete_config();
    value["profiles"]["app"]["requestedLocales"] = json!(["EN-us", "en-US", "en-US"]);
    value["profiles"]["app"]["defaultRequestedLocale"] = json!("not-in-set");
    value["profiles"]["app"]["deploymentGroups"]["web"]["members"] = json!(["browser", "browser"]);
    assert_eq!(
        serde_json::to_value(admitted(value.clone())).unwrap(),
        value
    );
}

#[test]
fn id_domains_share_exact_syntax_without_normalization() {
    for invalid in [
        "", "APP", "app/", " app", "app ", ".app", "app-", "_app", "日本", "a\nb",
    ] {
        for pointer in [
            "/profiles/app/projectId",
            "/profiles/app/selectionScope",
            "/profiles/app/deploymentGroups/web/members/0",
        ] {
            let mut value = minimal_config();
            *value.pointer_mut(pointer).unwrap() = json!(invalid);
            rejected(value);
        }
        for pointer in [
            "/profiles",
            "/profiles/app/targetProfiles",
            "/profiles/app/deploymentGroups",
        ] {
            let mut value = minimal_config();
            let map = value.pointer_mut(pointer).unwrap().as_object_mut().unwrap();
            let key = map.keys().next().unwrap().clone();
            let member = map.remove(&key).unwrap();
            map.insert(invalid.to_owned(), member);
            rejected(value);
        }
    }
    for valid in ["a", "1", "a.b", "a_b", "a-b", "a.._-b"] {
        let mut value = minimal_config();
        value["profiles"]["app"]["projectId"] = json!(valid);
        admitted(value);
    }
}
