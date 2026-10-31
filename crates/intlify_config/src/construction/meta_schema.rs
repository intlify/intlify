// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The JSON Schema meta-schema package: the one dialect revision `"0"` admits,
//! with the exact meta-schema every schema artifact is validated against.
//!
//! A schema's `$schema` URI names its dialect, but it never authorizes a
//! retrieval: the meta-schema is this package's copy, never a downloaded one,
//! a host registry's, or a package manager's.

use serde_json::{json, Value};

use super::admission::DRAFT_7;

/// The Draft 7 meta-schema as json-schema.org publishes it.
///
/// It is the copy `referencing` 0.56.0 bundles, byte for byte, and a test pins
/// its digest so a change to it cannot pass as a formatting change.
pub(crate) const DRAFT_7_META_SCHEMA: &str = include_str!("../../data/json-schema-draft-07.json");

/// The digest the meta-schema's exact bytes are pinned to.
pub(crate) const DRAFT_7_META_SCHEMA_DIGEST: &str =
    "sha256:9e786e2198debe6717675ce48e199199f2dba8e3a3e1ffe35cfc0959efd691cb";

/// The package's body.
///
/// # Panics
///
/// Panics when the compiled-in meta-schema is not JSON, which a test rules out.
pub(crate) fn package() -> Value {
    let meta_schema: Value =
        serde_json::from_str(DRAFT_7_META_SCHEMA).expect("the Draft 7 meta-schema is JSON");
    json!({
        "dialects": [
            { "dialect": DRAFT_7, "metaSchema": meta_schema }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use intlify_shared_json::encoding::digest_bytes;
    use intlify_shared_json::token::IntegrityDigest;

    #[test]
    fn the_meta_schema_is_the_pinned_bytes() {
        assert_eq!(
            IntegrityDigest::from_hash(digest_bytes(DRAFT_7_META_SCHEMA.as_bytes())).as_str(),
            DRAFT_7_META_SCHEMA_DIGEST
        );
    }

    #[test]
    fn the_package_names_draft_7_and_its_own_meta_schema() {
        let package = package();
        let dialects = package["dialects"].as_array().unwrap();
        assert_eq!(dialects.len(), 1);
        assert_eq!(dialects[0]["dialect"], DRAFT_7);
        let meta_schema = &dialects[0]["metaSchema"];
        assert_eq!(meta_schema["$schema"], DRAFT_7);
        assert_eq!(meta_schema["$id"], DRAFT_7);
        assert_eq!(meta_schema["title"], "Core schema meta-schema");
        // The meta-schema validates itself, as every Draft 7 schema it admits.
        let validator = jsonschema::draft7::new(meta_schema).unwrap();
        assert!(validator.is_valid(meta_schema));
        // Its value is the one the independent oracle validates with.
        assert!(jsonschema::draft7::meta::validator().is_valid(meta_schema));
    }
}
