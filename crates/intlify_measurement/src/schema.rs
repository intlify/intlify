// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Filesystem-free Draft 7 generation for the common record representations.
//!
//! A generated schema is representation data. It admits no record on its own:
//! integrity, binding, and inventory remain separate checks.

use schemars::{generate::SchemaSettings, JsonSchema};
use serde_json::Value;

/// Generate a record's Draft 7 schema under the exact name it is known by.
///
/// A record is a type alias over the shared envelope, and an alias carries no
/// schema name of its own, so several different records would otherwise all be
/// titled after that one envelope type.
pub fn draft7_record_schema<T: JsonSchema>(title: &str) -> Result<Value, serde_json::Error> {
    let mut value = draft7_schema::<T>()?;
    if let Some(object) = value.as_object_mut() {
        object.insert("title".to_owned(), Value::String(title.to_owned()));
    }
    Ok(value)
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
