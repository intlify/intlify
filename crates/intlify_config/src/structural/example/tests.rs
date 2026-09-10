// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use super::*;
use crate::profile_fixtures::minimal_config;

const SAMPLE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/examples/resolve_config/intlify.config.json"
));

fn run(config: &Value, profile: Option<&str>) -> Value {
    resolve(&serde_json::to_vec(config).unwrap(), profile).unwrap()
}

fn has_code(result: &Value, code: &str) -> bool {
    result["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .any(|d| d["code"] == code)
}

#[test]
fn editable_sample_resolves_with_corrections_and_no_invented_source_default() {
    let result = resolve(SAMPLE, None).unwrap();
    assert_eq!(result["status"], "resolved");
    assert_eq!(result["selectedProfile"], "app");
    assert_eq!(
        result["locales"],
        json!({
            "defaultSourceLocale": null,
            "requestedLocales": ["en-US", "ja"],
            "defaultRequestedLocale": "en-US"
        })
    );
    assert_eq!(result["corrections"].as_array().unwrap().len(), 2);
    assert_eq!(result["diagnostics"], json!([]));
    assert!(result["notice"]
        .as_str()
        .unwrap()
        .contains("not a LocalizationProjectProfile"));
}

#[test]
fn source_locale_is_canonicalized_only_when_declared() {
    let mut config = minimal_config();
    config["profiles"]["app"]["defaultSourceLocale"] = json!("iw-IL");
    let result = run(&config, None);
    assert_eq!(result["locales"]["defaultSourceLocale"], "he-IL");
    assert_eq!(
        result["corrections"],
        json!([{
            "pointer": "/profiles/app/defaultSourceLocale", "replacement": "he-IL"
        }])
    );
}

#[test]
fn duplicate_locales_report_all_occurrences_without_publishing_a_core() {
    let mut config = minimal_config();
    config["profiles"]["app"]["requestedLocales"] = json!(["en", "EN", "en"]);
    let result = run(&config, None);
    assert_eq!(result["status"], "not-resolved");
    assert!(result["locales"].is_null());
    let issue = &result["diagnostics"][0];
    assert_eq!(issue["code"], "CanonicalLocaleDuplicate");
    assert_eq!(
        issue["relatedPointers"],
        json!([
            "/profiles/app/requestedLocales/0",
            "/profiles/app/requestedLocales/1",
            "/profiles/app/requestedLocales/2"
        ])
    );
    assert!(issue["position"]["line"].as_u64().unwrap() > 0);
    assert_eq!(result["corrections"].as_array().unwrap().len(), 1);
}

#[test]
fn default_must_belong_to_requested_locales() {
    let mut config = minimal_config();
    config["profiles"]["app"]["defaultRequestedLocale"] = json!("ja");
    let result = run(&config, None);
    assert!(has_code(&result, "DefaultNotRequested"));
    assert!(result["locales"].is_null());
}

#[test]
fn unsupported_fixture_input_is_not_misrepresented_as_invalid() {
    for (input, code) in [
        ("pt-BR", "UnsupportedLocaleInput"),
        ("en_US", "InvalidLocaleIdentifier"),
    ] {
        let mut config = minimal_config();
        config["profiles"]["app"]["requestedLocales"] = json!([input]);
        config["profiles"]["app"]["defaultRequestedLocale"] = json!(input);
        let result = run(&config, None);
        assert!(has_code(&result, code), "{result}");
        assert!(result["locales"].is_null());
    }
}

#[test]
fn strict_input_errors_have_display_diagnostics() {
    for (source, code) in [
        (&b"\xff"[..], "InvalidUtf8"),
        (&b"{"[..], "InvalidJson"),
        (
            &br#"{"schemaVersion":"0","schemaVersion":"0"}"#[..],
            "DuplicateMember",
        ),
    ] {
        let result = resolve(source, None).unwrap();
        assert_eq!(result["stage"], "materialize");
        assert!(has_code(&result, code), "{result}");
        assert!(result["locales"].is_null());
    }
}

#[test]
fn duplicate_member_location_counts_utf8_bytes_and_crlf() {
    let source = "{\r\n  \"é\":0,\"é\":1\r\n}";
    let result = resolve(source.as_bytes(), None).unwrap();
    let issue = &result["diagnostics"][0];
    assert_eq!(issue["code"], "DuplicateMember");
    assert_eq!(issue["position"]["line"], 2);
    assert_eq!(issue["position"]["byteColumn"], 10);
    assert_eq!(issue["relatedPosition"]["byteColumn"], 3);
}

#[test]
fn malformed_structure_does_not_reach_selection_or_echo_rejected_values() {
    let mut config = minimal_config();
    config["profiles"]["app"]["policies"]["trust"]["kind"] = json!("private-secret-value");
    let result = run(&config, Some("app"));
    assert_eq!(result["stage"], "structure");
    assert!(result["selectedProfile"].is_null());
    assert!(!result["diagnostics"].as_array().unwrap().is_empty());
    assert!(!result.to_string().contains("private-secret-value"));
}

#[test]
fn required_nullable_fields_cannot_be_omitted_and_unknown_fields_are_rejected() {
    let mut config = minimal_config();
    config["profiles"]["app"]["policies"]
        .as_object_mut()
        .unwrap()
        .remove("glossarySet");
    let result = run(&config, None);
    assert!(has_code(&result, "RequiredFieldMissing"));
    assert!(result["diagnostics"].to_string().contains("glossarySet"));

    let mut config = minimal_config();
    config["private-secret-key"] = json!("private-secret-value");
    let result = run(&config, None);
    assert!(has_code(&result, "UnknownField"));
    assert!(!result.to_string().contains("private-secret"));
}

#[test]
fn multiple_profiles_require_exact_selection_and_a_complete_root() {
    let mut config = minimal_config();
    config["profiles"]["other"] = config["profiles"]["app"].clone();
    assert!(has_code(&run(&config, None), "ProfileRequired"));
    assert!(has_code(&run(&config, Some("missing")), "UnknownProfile"));
    assert!(has_code(
        &run(&config, Some("APP")),
        "InvalidProfileSelector"
    ));
    assert_eq!(run(&config, Some("other"))["selectedProfile"], "other");

    config["profiles"]["other"]["requestedLocales"] = json!(false);
    let result = run(&config, Some("app"));
    assert_eq!(result["stage"], "structure");
    assert!(result["selectedProfile"].is_null());
    assert!(result["locales"].is_null());
}

#[test]
fn oversized_input_is_rejected_before_parsing_a_valid_prefix() {
    let mut source = SAMPLE.to_vec();
    source.resize(usize::try_from(MAX_FILE_BYTES + 1).unwrap(), b' ');
    let result = resolve(&source, None).unwrap();
    assert_eq!(result["stage"], "materialize");
    assert!(has_code(&result, "InputLimit"));
    assert!(result["locales"].is_null());
}

#[test]
fn a_failed_run_does_not_change_a_later_resolution() {
    let first = resolve(SAMPLE, None).unwrap();
    let _ = resolve(b"invalid", None).unwrap();
    assert_eq!(first, resolve(SAMPLE, None).unwrap());
}
