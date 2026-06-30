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
    assert_eq!(json["required"][0], "fmt");
    assert_eq!(json["required"][1], "lint");
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
