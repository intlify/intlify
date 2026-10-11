// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Draft 7 generation for the representations this crate owns.
//!
//! Design 017 asks the adopting implementation to materialize closed schemas
//! for the conformance value and the manifest. They are committed and checked,
//! not inferred at decode time, and nothing here fetches or executes a schema.
//! A schema states the closed shape; the canonical order 017 fixes is checked
//! by the reader, because Draft 7 cannot express it.

use schemars::{generate::SchemaSettings, JsonSchema};
use serde_json::Value;

use crate::conformance::LocaleConformanceData;

/// One committed schema: its generator and where it lives in this crate.
pub struct CommittedSchema {
    /// The path of the committed artifact, relative to the crate root.
    pub path: &'static str,
    /// Generate the schema the committed artifact must equal.
    pub generate: fn() -> Result<Value, serde_json::Error>,
}

/// Every schema this crate commits, in the order they are checked.
pub const COMMITTED_SCHEMAS: [CommittedSchema; 1] = [CommittedSchema {
    path: "schema/locale-conformance-data-v0.schema.json",
    generate: conformance_schema,
}];

/// Generate the complete closed schema of the conformance value.
pub fn conformance_schema() -> Result<Value, serde_json::Error> {
    draft7_schema::<LocaleConformanceData>()
}

/// Generate one model's Draft 7 schema without the generator-only root `$id`.
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

/// Produce stable bytes for the committed artifact.
///
/// Members are sorted by unsigned UTF-8 bytes so regeneration is deterministic.
/// Freshness is checked by comparing decoded values rather than bytes, because
/// the repository formatter also owns the committed file's layout.
pub fn format_schema(mut value: Value) -> Result<String, serde_json::Error> {
    sort_object_members(&mut value);
    let mut output = serde_json::to_string_pretty(&value)?;
    output.push('\n');
    Ok(output)
}

fn sort_object_members(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for member in object.values_mut() {
                sort_object_members(member);
            }
            let mut members = std::mem::take(object).into_iter().collect::<Vec<_>>();
            members.sort_by(|(left, _), (right, _)| left.as_bytes().cmp(right.as_bytes()));
            object.extend(members);
        }
        Value::Array(values) => {
            for member in values {
                sort_object_members(member);
            }
        }
        Value::Null | Value::Bool(_) | Value::Number(_) | Value::String(_) => {}
    }
}

#[cfg(test)]
mod tests;
