// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fs;
use std::io;
use std::path::Path;

use schemars::gen::SchemaSettings;
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
    let mut settings = SchemaSettings::draft07();
    settings.option_nullable = false;
    let generator = settings.into_generator();
    let root_schema = generator.into_root_schema_for::<ProjectConfigFile>();
    let mut value = serde_json::to_value(root_schema).map_err(ConfigSchemaError::Generate)?;
    remove_schema_id(&mut value);
    remove_nullable_schema_metadata(&mut value);

    let mut output = serde_json::to_string_pretty(&value).map_err(ConfigSchemaError::Generate)?;
    // Match the repository JSON formatter so `vp check` and schema verification
    // agree on the committed artifact bytes.
    output = output.replace(
        "\"required\": [\n    \"fmt\",\n    \"lint\"\n  ]",
        "\"required\": [\"fmt\", \"lint\"]",
    );
    output.push('\n');
    Ok(output)
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

fn remove_schema_id(value: &mut Value) {
    if let Some(object) = value.as_object_mut() {
        object.remove("$id");
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
}
