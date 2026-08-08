// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::path::{Path, PathBuf};

use serde_json::Value;

#[test]
fn generated_config_schema_matches_design_contract() {
    let schema = intlify_cli::schema::generate_config_schema().expect("schema should generate");
    let json = serde_json::from_str::<Value>(&schema).expect("schema should be valid JSON");

    assert_eq!(json["$schema"], "http://json-schema.org/draft-07/schema#");
    assert!(json.get("$id").is_none());
    assert!(json.get("$defs").is_none());
    assert!(json.get("definitions").is_some());
    assert!(json.get("required").is_none());
    assert!(json["definitions"].get("FormatterConfig").is_some());
    assert!(json["definitions"].get("FormatterMode").is_some());
    assert!(json["definitions"].get("ResourcesConfig").is_some());
    assert!(json["definitions"].get("CatalogConfig").is_some());
    assert!(json["definitions"].get("MessagesConfig").is_some());
    assert!(json["definitions"].get("MessageSelectorConfig").is_some());

    let resources = &json["properties"]["resources"];
    assert_eq!(resources["$ref"], "#/definitions/ResourcesConfig");
    assert!(resources.get("anyOf").is_none());
    let messages = &json["properties"]["messages"];
    assert_eq!(messages["$ref"], "#/definitions/MessagesConfig");
    assert!(messages.get("anyOf").is_none());

    let catalogs = &json["definitions"]["ResourcesConfig"]["properties"]["catalogs"];
    assert_eq!(catalogs["type"], "array");
    assert!(catalogs.get("anyOf").is_none());
    assert!(json["definitions"]["ResourcesConfig"]
        .get("required")
        .is_none());

    let catalog = &json["definitions"]["CatalogConfig"];
    assert_eq!(catalog["required"], serde_json::json!(["include"]));
    assert_eq!(catalog["properties"]["include"]["minItems"], 1);
    assert_eq!(catalog["properties"]["exclude"]["type"], "array");
    assert!(catalog["properties"]["exclude"].get("anyOf").is_none());
    assert_eq!(catalog["properties"]["format"]["type"], "string");
    assert_eq!(
        catalog["properties"]["format"]["enum"],
        serde_json::json!(["json"])
    );
    assert!(catalog["properties"]["format"].get("anyOf").is_none());

    assert_eq!(catalog["properties"]["scope"]["type"], "string");
    assert_eq!(catalog["properties"]["scope"]["minLength"], 1);
    assert_eq!(catalog["properties"]["scope"]["maxLength"], 255);
    assert!(catalog["properties"]["scope"].get("anyOf").is_none());
    assert!(catalog["properties"]["locale"].get("oneOf").is_some());
    assert!(catalog["properties"]["locale"].get("anyOf").is_none());
    assert_eq!(
        catalog["properties"]["locale"]["oneOf"][1]["properties"]["value"]["maxLength"],
        255
    );
    assert_eq!(
        catalog["dependencies"]["scope"],
        serde_json::json!(["locale"])
    );
    assert_eq!(
        catalog["dependencies"]["locale"],
        serde_json::json!(["scope"])
    );

    let messages = &json["definitions"]["MessagesConfig"];
    assert_eq!(messages["required"], serde_json::json!(["locales"]));
    assert_eq!(messages["properties"]["locales"]["minItems"], 1);
    assert_eq!(messages["properties"]["locales"]["maxItems"], 1024);
    assert_eq!(messages["properties"]["locales"]["items"]["minLength"], 1);
    assert_eq!(messages["properties"]["roots"]["maxItems"], 4096);
    let fallback = &messages["properties"]["fallback"];
    assert_eq!(fallback["type"], "object");
    assert_eq!(fallback["maxProperties"], 1024);
    assert_eq!(fallback["propertyNames"]["minLength"], 1);
    assert_eq!(fallback["propertyNames"]["maxLength"], 255);
    assert_eq!(fallback["additionalProperties"]["minItems"], 1);
    assert_eq!(fallback["additionalProperties"]["maxItems"], 64);
    assert_eq!(fallback["additionalProperties"]["uniqueItems"], true);
    assert_eq!(fallback["additionalProperties"]["items"]["minLength"], 1);
    assert_eq!(fallback["additionalProperties"]["items"]["maxLength"], 255);
    let coverage_baseline = &messages["properties"]["coverageBaseline"];
    assert_eq!(coverage_baseline["type"], "object");
    assert_eq!(coverage_baseline["minProperties"], 1);
    assert_eq!(coverage_baseline["maxProperties"], 4096);
    assert_eq!(coverage_baseline["propertyNames"]["minLength"], 1);
    assert_eq!(coverage_baseline["propertyNames"]["maxLength"], 255);
    assert_eq!(coverage_baseline["additionalProperties"]["minLength"], 1);
    assert_eq!(coverage_baseline["additionalProperties"]["maxLength"], 255);
    let delivery = &messages["properties"]["delivery"];
    assert_eq!(delivery["required"], serde_json::json!(["targets"]));
    assert_eq!(delivery["properties"]["targets"]["minItems"], 1);
    assert_eq!(delivery["properties"]["targets"]["maxItems"], 64);
    assert_eq!(
        delivery["properties"]["targets"]["items"]["$ref"],
        "#/definitions/MessageDeliveryTargetConfig"
    );
    let delivery_target = &json["definitions"]["MessageDeliveryTargetConfig"];
    assert_eq!(
        delivery_target["required"],
        serde_json::json!(["name", "exporter", "out"])
    );
    assert_eq!(delivery_target["properties"]["name"]["minLength"], 1);
    assert_eq!(delivery_target["properties"]["name"]["maxLength"], 255);
    assert_eq!(
        delivery_target["properties"]["name"]["pattern"],
        "^[a-z0-9][a-z0-9._-]{0,254}$"
    );
    assert_eq!(
        delivery_target["properties"]["exporter"]["enum"],
        serde_json::json!(["esm"])
    );
    assert_eq!(delivery_target["properties"]["out"]["minLength"], 1);
    assert_eq!(delivery_target["properties"]["out"]["maxLength"], 4_159);
    assert_eq!(
        delivery_target["properties"]["eagerLocales"]["maxItems"],
        1_024
    );
    assert_eq!(
        delivery_target["properties"]["eagerLocales"]["uniqueItems"],
        true
    );
    assert_eq!(
        delivery_target["properties"]["placement"]["enum"],
        serde_json::json!(["duplicate"])
    );

    let producers = &json["definitions"]["MessageProducersConfig"];
    let js = &producers["properties"]["js"];
    assert!(js.get("anyOf").is_none());
    assert_eq!(js["properties"]["include"]["minItems"], 1);
    assert_eq!(js["properties"]["recognizers"]["minProperties"], 1);
    assert_eq!(producers["properties"]["artifacts"]["minItems"], 1);

    let root = &json["definitions"]["MessageRootConfig"];
    assert_eq!(root["properties"]["reason"]["type"], "string");
    assert!(root["properties"]["reason"].get("anyOf").is_none());

    let selector_kinds = json["definitions"]["MessageSelectorConfig"]["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .map(|selector| selector["properties"]["kind"]["const"].as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        selector_kinds,
        ["exact", "prefix", "pattern", "all-in-scope"]
    );
    assert!(schema.ends_with('\n'));
}

#[test]
fn committed_config_schema_is_current() {
    intlify_cli::schema::check_config_schema(&workspace_schema_path())
        .expect("committed schema should match generated schema");
}

#[test]
fn schema_check_detects_stale_content() {
    let error =
        intlify_cli::schema::check_config_schema_contents("{}\n").expect_err("stale schema fails");

    assert!(matches!(
        error,
        intlify_cli::schema::ConfigSchemaError::Stale { .. }
    ));
}

fn workspace_schema_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(intlify_cli::schema::CONFIG_SCHEMA_ARTIFACT)
}
