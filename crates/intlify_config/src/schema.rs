// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Filesystem-free Draft 7 generation and deterministic artifact formatting.
//!
//! Owners normalize their own field semantics before formatting. This module
//! never removes nullable fields or adds product-specific schema defaults.

use schemars::{generate::SchemaSettings, JsonSchema};
use serde_json::Value;

/// Generate the complete configuration-version-0 structural schema.
///
/// Uses 015's authoring model and 017's formal reference encodings. This helper
/// exposes schema data, not a partial Profile or a resolver construction API.
pub fn project_profile_config_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<
        crate::model::IntlifyConfig<
            crate::references::PolicyReference,
            crate::references::TargetProfileReference,
        >,
    >()
}

/// Workspace-only schema data for the adopted minimum measurement Run Plan.
/// Does not expose plan issuance, a partial resolver, or measurement samples.
#[cfg(feature = "benchmark")]
pub fn measurement_run_plan_schema() -> Result<Value, serde_json::Error> {
    crate::benchmark::run_plan_schema()
}

/// Complete input representation of the adopted minimum Measurement Case ID.
#[cfg(feature = "benchmark")]
pub fn measurement_case_identity_schema() -> Result<Value, serde_json::Error> {
    crate::benchmark::measurement_case_schema()
}

/// Generate an owner model's Draft 7 schema without generator-only root `$id`.
pub fn draft7_schema<T: JsonSchema>() -> Result<Value, serde_json::Error> {
    let root = SchemaSettings::draft07()
        .into_generator()
        .into_root_schema_for::<T>();
    let mut value = serde_json::to_value(root)?;
    if let Some(object) = value.as_object_mut() {
        object.remove("$id");
    }
    Ok(value)
}

/// Sort object members by unsigned UTF-8 bytes and produce stable JSON bytes.
///
/// Array order and all field semantics are retained. String arrays use the
/// repository formatter's compact style when they fit its 100-column width,
/// and output has one final newline.
pub fn format_schema(mut value: Value) -> Result<String, serde_json::Error> {
    sort_object_members(&mut value);
    let output = serde_json::to_string_pretty(&value)?;
    let mut output = compact_string_arrays(&output);
    output.push('\n');
    Ok(output)
}

fn sort_object_members(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for value in object.values_mut() {
                sort_object_members(value);
            }
            let mut members = std::mem::take(object).into_iter().collect::<Vec<_>>();
            members.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
            object.extend(members);
        }
        Value::Array(values) => {
            for value in values {
                sort_object_members(value);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

fn compact_string_arrays(output: &str) -> String {
    let lines = output.lines().collect::<Vec<_>>();
    let mut compacted = Vec::with_capacity(lines.len());
    let mut index = 0;

    while let Some(line) = lines.get(index) {
        let trimmed = line.trim_start();
        if trimmed.starts_with('"') && trimmed.ends_with(": [") {
            if let Some((next_index, compacted_line)) = compact_string_array_lines(&lines, index) {
                compacted.push(compacted_line);
                index = next_index;
                continue;
            }
        }

        compacted.push((*line).to_owned());
        index += 1;
    }

    compacted.join("\n")
}

fn compact_string_array_lines(lines: &[&str], start: usize) -> Option<(usize, String)> {
    let line = lines[start];
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let property = trimmed.strip_suffix(" [")?;
    let mut values = Vec::new();
    let mut cursor = start + 1;

    while let Some(line) = lines.get(cursor) {
        let trimmed = line.trim();
        let trailing_comma = trimmed.ends_with(',');
        let value = trimmed.strip_suffix(',').unwrap_or(trimmed);

        if value == "]" {
            if values.is_empty() {
                return None;
            }
            let joined = values
                .iter()
                .map(|value| {
                    serde_json::to_string(value).expect("string enum value is serializable")
                })
                .collect::<Vec<_>>()
                .join(", ");
            let comma = if trailing_comma { "," } else { "" };
            let compacted = format!("{indent}{property} [{joined}]{comma}");
            // Keep serde's multiline representation for long required/enum
            // lists, so regeneration and the repository formatter agree.
            return (compacted.chars().count() <= 100).then_some((cursor + 1, compacted));
        }

        match serde_json::from_str::<String>(value) {
            Ok(value) => values.push(value),
            Err(_) => return None,
        }
        cursor += 1;
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[derive(JsonSchema)]
    #[allow(dead_code)]
    struct Fixture {
        nullable: Option<String>,
        count: u32,
    }

    #[test]
    fn draft7_generation_does_not_change_nullable_fields() {
        let schema = draft7_schema::<Fixture>().unwrap();
        assert_eq!(schema["$schema"], "http://json-schema.org/draft-07/schema#");
        assert!(schema.get("$id").is_none());
        assert_eq!(
            schema["properties"]["nullable"]["type"],
            json!(["string", "null"])
        );
    }

    #[test]
    fn formatting_is_deterministic_and_semantics_preserving() {
        let first: Value = serde_json::from_str(
            r#"{"z":{"β":null,"A":false},"a":[{"z":2,"a":1}],"type":["string","null"]}"#,
        )
        .unwrap();
        let second: Value = serde_json::from_str(
            r#"{"type":["string","null"],"a":[{"a":1,"z":2}],"z":{"A":false,"β":null}}"#,
        )
        .unwrap();
        let output = format_schema(first.clone()).unwrap();
        assert_eq!(output, format_schema(second).unwrap());
        assert_eq!(serde_json::from_str::<Value>(&output).unwrap(), first);
        assert!(output.ends_with('\n'));
        assert!(!output.ends_with("\n\n"));
        assert!(output.contains(r#""type": ["string", "null"]"#));
    }

    #[test]
    fn arrays_keep_order_escaping_and_non_string_elements() {
        let value = json!({"enum": ["z", "a", "a\"b", "a\\b", "\n"], "items": [1, null, true]});
        let output = format_schema(value.clone()).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&output).unwrap(), value);
        assert_eq!(
            output,
            format_schema(serde_json::from_str(&output).unwrap()).unwrap()
        );
    }

    #[test]
    fn long_string_arrays_keep_the_multiline_form_without_changing_values() {
        let value = json!({"required": ["a".repeat(60), "b".repeat(60)]});
        let output = format_schema(value.clone()).unwrap();
        assert!(output.contains("\"required\": [\n"));
        assert_eq!(serde_json::from_str::<Value>(&output).unwrap(), value);
        assert_eq!(
            output,
            format_schema(serde_json::from_str(&output).unwrap()).unwrap()
        );
    }
}
