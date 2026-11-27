// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The raw inputs a fixture holds (D15:3243-3248).
//!
//! A raw input that its boundary reads as bytes is lowercase hexadecimal or
//! a `{ path, digest }` reference to a file below the suite root, which
//! belongs to the fixture closure and is verified before any case runs; one
//! that its boundary reads as a decoded JSON value is `{ json }`, inside which
//! nothing is a reference. The runner keeps each verified body by its path,
//! so a fixture's references read the bodies it verified.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::construction::component::{file_digest, strict_json};

use super::index::SuiteFile;

/// The verified bodies, by their suite-root-relative paths.
pub(crate) type Bodies = BTreeMap<String, Vec<u8>>;

/// The bytes of a raw input that its boundary reads as bytes.
///
/// # Panics
///
/// Panics when a reference names a body the runner did not verify, which it
/// verifies for every reference before any case runs.
pub(crate) fn bytes(raw: &Value, bodies: &Bodies) -> Vec<u8> {
    if let Some(hex) = raw.get("bytes").and_then(Value::as_str) {
        return decode(hex);
    }
    let path = raw["path"]
        .as_str()
        .expect("the Case Schema makes it a string");
    bodies[path].clone()
}

/// Every `{ path, digest }` reference `fixture` holds outside a decoded JSON
/// value, in document order.
pub(crate) fn references(fixture: &Value) -> Vec<&Value> {
    fn collect<'a>(value: &'a Value, found: &mut Vec<&'a Value>) {
        match value {
            Value::Object(members) if members.len() == 1 && members.contains_key("json") => {}
            Value::Object(members)
                if members.len() == 2
                    && members.get("path").is_some_and(Value::is_string)
                    && members.get("digest").is_some_and(Value::is_string) =>
            {
                found.push(value);
            }
            Value::Object(members) => members.values().for_each(|member| collect(member, found)),
            Value::Array(items) => items.iter().for_each(|item| collect(item, found)),
            _ => {}
        }
    }
    let mut found = Vec::new();
    collect(fixture, &mut found);
    found
}

/// The bytes of lowercase hexadecimal, two digits a byte, as the Case Schema
/// makes it.
pub(crate) fn decode(hex: &str) -> Vec<u8> {
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let text = std::str::from_utf8(pair).expect("hexadecimal digits are ASCII");
            u8::from_str_radix(text, 16).expect("the Case Schema makes two hexadecimal digits")
        })
        .collect()
}

/// `fixture` of case `id` as it is written below `directory`: each raw input
/// that is strict UTF-8 JSON a file named for where the fixture holds it, in
/// the case's own directory, unless an earlier case wrote the same bytes,
/// whose file it references. A decoded input holds no raw input.
pub(crate) fn written(
    directory: &str,
    id: &str,
    fixture: &Value,
    files: &mut Vec<SuiteFile>,
    kept: &mut BTreeMap<Vec<u8>, String>,
) -> Value {
    fn walk(
        value: &Value,
        at: &str,
        directory: &str,
        id: &str,
        files: &mut Vec<SuiteFile>,
        kept: &mut BTreeMap<Vec<u8>, String>,
    ) -> Value {
        match value {
            Value::Object(members) if members.len() == 1 && members.contains_key("json") => {
                value.clone()
            }
            Value::Object(members) if members.len() == 1 && members.contains_key("bytes") => {
                let bytes = decode(members["bytes"].as_str().expect("raw bytes are hex"));
                if strict_json(&bytes).is_none() {
                    return value.clone();
                }
                let path = kept
                    .entry(bytes.clone())
                    .or_insert_with(|| {
                        let path = format!("{directory}/{id}/{at}.json");
                        files.push(SuiteFile {
                            path: path.clone(),
                            bytes: bytes.clone(),
                        });
                        path
                    })
                    .clone();
                json!({ "path": path, "digest": file_digest(&bytes).as_str() })
            }
            Value::Object(members) => Value::Object(
                members
                    .iter()
                    .map(|(name, member)| {
                        let at = if at.is_empty() {
                            name.clone()
                        } else {
                            format!("{at}.{name}")
                        };
                        (name.clone(), walk(member, &at, directory, id, files, kept))
                    })
                    .collect::<Map<_, _>>(),
            ),
            Value::Array(items) => Value::Array(
                items
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        walk(item, &format!("{at}.{index}"), directory, id, files, kept)
                    })
                    .collect(),
            ),
            other => other.clone(),
        }
    }
    walk(fixture, "", directory, id, files, kept)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::evidence_source::hex;

    fn raw(bytes: &[u8]) -> Value {
        json!({ "bytes": hex(bytes) })
    }

    #[test]
    fn hexadecimal_and_a_reference_give_their_bytes() {
        let bodies = Bodies::from([("a/b.json".to_owned(), b"{}\n".to_vec())]);
        assert_eq!(bytes(&json!({ "bytes": "" }), &bodies), b"");
        assert_eq!(
            bytes(&json!({ "bytes": "00ff7b" }), &bodies),
            [0x00, 0xff, b'{']
        );
        let reference = json!({ "path": "a/b.json", "digest": "sha256:0" });
        assert_eq!(bytes(&reference, &bodies), b"{}\n");
    }

    /// A reference is found wherever it is, but not inside a decoded JSON
    /// value, whose objects are data.
    #[test]
    fn references_are_found_outside_decoded_values() {
        let reference = |path: &str| json!({ "path": path, "digest": "sha256:0" });
        let fixture = json!({
            "a": { "tuple": { "json": { "path": "x", "digest": "y" } }, "bytes": reference("one") },
            "b": { "members": { "0": reference("two"), "1": { "bytes": "00" } } },
            "c": [reference("three"), { "path": "p", "digest": "d", "more": 1 }],
        });
        let paths = references(&fixture)
            .into_iter()
            .map(|reference| reference["path"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(paths, ["one", "two", "three"]);
    }

    /// A raw input that is strict UTF-8 JSON is a file named for where the
    /// fixture holds it, written once for the same bytes; any other stays
    /// hexadecimal, and a decoded value is kept as it is.
    #[test]
    fn a_raw_input_of_strict_json_is_a_file_of_its_own() {
        let mut files = Vec::new();
        let mut kept = BTreeMap::new();
        let fixture = json!({
            "capability": { "json": { "bytes": "7b7d" } },
            "input": { "manifest": raw(b"{}"), "members": [raw(b"{"), raw(b"{}")] },
        });
        let first = written(
            "fixtures/admission",
            "015-a",
            &fixture,
            &mut files,
            &mut kept,
        );
        let reference = json!({
            "path": "fixtures/admission/015-a/input.manifest.json",
            "digest": file_digest(b"{}").as_str(),
        });
        assert_eq!(first["capability"], fixture["capability"]);
        assert_eq!(first["input"]["manifest"], reference);
        assert_eq!(first["input"]["members"], json!([raw(b"{"), reference]));
        assert_eq!(files.len(), 1);
        assert_eq!(
            files[0].path,
            "fixtures/admission/015-a/input.manifest.json"
        );
        assert_eq!(files[0].bytes, b"{}");
        let again = written(
            "fixtures/admission",
            "015-b",
            &json!({ "input": raw(b"{}") }),
            &mut files,
            &mut kept,
        );
        assert_eq!(again["input"], reference);
        assert_eq!(files.len(), 1);
    }
}
