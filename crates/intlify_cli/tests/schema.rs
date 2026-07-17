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

    let resources = &json["properties"]["resources"];
    assert_eq!(resources["$ref"], "#/definitions/ResourcesConfig");
    assert!(resources.get("anyOf").is_none());

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
