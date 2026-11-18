// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Filesystem-free Draft 7 generation for the common record representations.
//!
//! A generated schema is representation data. It admits no record on its own:
//! integrity, binding, and inventory remain separate checks.

use schemars::{generate::SchemaSettings, JsonSchema, Schema};
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

/// Require each member of a record object whose value may be null.
///
/// A record writes such a member even when it has no value, as `null`, and
/// its decoder rejects a record that leaves the member out (it reads the
/// member with `deserialize_with = "Option::deserialize"`). A derived schema
/// lists an `Option` member as optional, so without this transform it would
/// admit records the decoder rejects. The added members follow name order, so
/// the schema does not depend on the order of the member map.
pub fn require_nullable_members(schema: &mut Schema) {
    let mut nullable = schema
        .get("properties")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
        .filter(|(_, member)| admits_null(member))
        .map(|(name, _)| name.clone())
        .collect::<Vec<_>>();
    nullable.sort_unstable();
    for name in nullable {
        let required = schema
            .ensure_object()
            .entry("required")
            .or_insert_with(|| Value::Array(Vec::new()));
        if let Value::Array(names) = required {
            if !names.iter().any(|known| known == name.as_str()) {
                names.push(Value::String(name));
            }
        }
    }
}

/// Whether a member's schema admits null. An `Option` adds `"null"` to an
/// inline type, or a null branch beside a referenced one.
fn admits_null(member: &Value) -> bool {
    let typed = match member.get("type") {
        Some(Value::String(kind)) => kind == "null",
        Some(Value::Array(kinds)) => kinds.iter().any(|kind| kind == "null"),
        _ => false,
    };
    typed
        || member
            .get("anyOf")
            .and_then(Value::as_array)
            .is_some_and(|branches| branches.iter().any(admits_null))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};
    use serde_json::json;

    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    #[allow(dead_code)]
    struct Inner {
        count: u32,
    }

    /// A member in each shape a record member takes. The two that may be null
    /// are declared out of name order.
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    #[schemars(transform = require_nullable_members)]
    #[allow(dead_code)]
    struct Fixture {
        count: u32,
        #[serde(deserialize_with = "Option::deserialize")]
        referenced: Option<Inner>,
        #[serde(deserialize_with = "Option::deserialize")]
        inline: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[schemars(with = "u32")]
        omitted: Option<u32>,
    }

    /// Every member may be null, so the derived schema requires none.
    #[derive(Serialize, Deserialize, JsonSchema)]
    #[serde(deny_unknown_fields)]
    #[schemars(transform = require_nullable_members)]
    #[allow(dead_code)]
    struct OnlyNullable {
        #[serde(deserialize_with = "Option::deserialize")]
        only: Option<String>,
    }

    #[test]
    fn a_member_that_may_be_null_is_required_as_its_decoder_requires_it() {
        let schema = draft7_schema::<Fixture>().unwrap();
        assert_eq!(schema["required"], json!(["count", "inline", "referenced"]));
        let validator = jsonschema::draft7::new(&schema).unwrap();
        let record = json!({"count": 1, "referenced": {"count": 2}, "inline": null, "omitted": 3});
        assert!(validator.is_valid(&record));
        assert!(validator.is_valid(&json!({"count": 1, "referenced": null, "inline": "a"})));
        for member in ["count", "referenced", "inline", "omitted"] {
            let mut left_out = record.clone();
            left_out.as_object_mut().unwrap().remove(member);
            assert_eq!(
                validator.is_valid(&left_out),
                serde_json::from_value::<Fixture>(left_out).is_ok(),
                "{member}"
            );
        }

        let schema = draft7_schema::<OnlyNullable>().unwrap();
        assert_eq!(schema["required"], json!(["only"]));
    }

    #[test]
    fn a_member_the_contract_already_requires_is_listed_once() {
        // For a serializer, a member it always writes is already required.
        let schema = SchemaSettings::draft07()
            .for_serialize()
            .into_generator()
            .into_root_schema_for::<Fixture>();
        assert_eq!(
            schema.get("required"),
            Some(&json!(["count", "referenced", "inline"]))
        );
    }
}
