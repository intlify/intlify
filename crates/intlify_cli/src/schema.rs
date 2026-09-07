// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Deterministic generation and freshness checking for the CLI config schema.
//!
//! This module derives the Draft 7 artifact from the runtime config types,
//! normalizes generator-only metadata and formatting, and writes or compares the
//! committed schema bytes. Runtime configuration validation remains in
//! `config` and its section owners.

use std::fs;
use std::io;
use std::path::Path;

use intlify_config::schema::{draft7_schema, format_schema};
use serde_json::Value;

use crate::config::ProjectConfigFile;

pub const OUTPUT_SCHEMA_VERSION: &str = "0";
pub const CONFIG_SCHEMA_ARTIFACT: &str = "packages/cli/schema/config.schema.json";

#[derive(Debug)]
pub enum ConfigSchemaError {
    Generate(serde_json::Error),
    Read(io::Error),
    Write(io::Error),
    Stale { expected: String, actual: String },
}

impl std::fmt::Display for ConfigSchemaError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Generate(error) => write!(formatter, "failed to generate config schema: {error}"),
            Self::Read(error) => write!(formatter, "failed to read config schema: {error}"),
            Self::Write(error) => write!(formatter, "failed to write config schema: {error}"),
            Self::Stale { .. } => write!(formatter, "committed config schema is stale"),
        }
    }
}

impl std::error::Error for ConfigSchemaError {}

pub fn generate_config_schema() -> Result<String, ConfigSchemaError> {
    let mut value = draft7_schema::<ProjectConfigFile>().map_err(ConfigSchemaError::Generate)?;
    remove_nullable_schema_metadata(&mut value);
    format_schema(value).map_err(ConfigSchemaError::Generate)
}

pub fn write_config_schema(path: &Path) -> Result<(), ConfigSchemaError> {
    let schema = generate_config_schema()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(ConfigSchemaError::Write)?;
    }
    fs::write(path, schema).map_err(ConfigSchemaError::Write)
}

pub fn check_config_schema(path: &Path) -> Result<(), ConfigSchemaError> {
    let expected = generate_config_schema()?;
    let actual = fs::read_to_string(path)
        .map_err(ConfigSchemaError::Read)
        .map(|content| content.replace("\r\n", "\n"))?;

    if actual == expected {
        Ok(())
    } else {
        Err(ConfigSchemaError::Stale { expected, actual })
    }
}

pub fn check_config_schema_contents(actual: &str) -> Result<(), ConfigSchemaError> {
    let expected = generate_config_schema()?;
    let actual = actual.replace("\r\n", "\n");

    if actual == expected {
        Ok(())
    } else {
        Err(ConfigSchemaError::Stale { expected, actual })
    }
}

fn remove_nullable_schema_metadata(value: &mut Value) {
    let Some(properties) = value
        .as_object_mut()
        .and_then(|object| object.get_mut("properties"))
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    let Some(schema_property) = properties.get_mut("$schema").and_then(Value::as_object_mut) else {
        return;
    };

    schema_property.insert("type".to_owned(), Value::String("string".to_owned()));
    remove_property_null_variant(properties, "fmt");
    remove_property_null_variant(properties, "lint");
    remove_property_null_variant(properties, "resources");
    remove_property_null_variant(properties, "messages");
}

fn remove_property_null_variant(properties: &mut serde_json::Map<String, Value>, property: &str) {
    let Some(property_schema) = properties.get_mut(property).and_then(Value::as_object_mut) else {
        return;
    };
    let Some(any_of) = property_schema
        .remove("anyOf")
        .and_then(|value| value.as_array().cloned())
    else {
        return;
    };
    let Some(non_null_schema) = any_of
        .into_iter()
        .find(|schema| schema.get("type").and_then(Value::as_str) != Some("null"))
    else {
        return;
    };
    let Some(non_null_schema) = non_null_schema.as_object() else {
        return;
    };

    for (key, value) in non_null_schema {
        property_schema.insert(key.clone(), value.clone());
    }
}
