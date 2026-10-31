// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The authority-envelope schema package: the closed Draft 7 schemas a
//! component's declared tuple and a container's artifact are checked against.
//!
//! The schemas constrain only the form of each value: an identity or revision
//! is a token, a digest is a `sha256:` presentation, and a container has
//! exactly its members. They never say which identity, revision, or dialect a
//! value must name; those comparisons are rows of the construction-failure
//! precedence of their own.
//!
//! A container's member order is part of its artifact, which a JSON Schema
//! cannot express; construction checks it beside the container's schema.
//!
//! The Capability Specification body's schema is not here yet: it closes the
//! body's groups, bounds, units, order, and minimum vector, and comes with the
//! Capability Specification itself.

use serde_json::{json, Map, Value};

use intlify_shared_json::token::ID_PATTERN;

use super::admission::{EnvelopeSchema, DRAFT_7};

/// A `sha256:` presentation of a complete digest.
pub(crate) const DIGEST_PATTERN: &str = "^sha256:[0-9a-f]{64}$";

fn token() -> Value {
    json!({ "type": "string", "pattern": ID_PATTERN })
}

fn digest() -> Value {
    json!({ "type": "string", "pattern": DIGEST_PATTERN })
}

/// A closed object with exactly these members, each required.
fn closed(members: &[(&str, Value)]) -> Value {
    let mut properties = Map::new();
    for (name, schema) in members {
        properties.insert((*name).to_owned(), schema.clone());
    }
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": members.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        "properties": properties,
    })
}

fn tuple() -> Value {
    closed(&[
        ("identity", token()),
        ("revision", token()),
        ("digest", digest()),
    ])
}

/// Place a schema at the root of its own document.
fn document(mut schema: Value) -> Value {
    let mut root = Map::new();
    root.insert("$schema".to_owned(), json!(DRAFT_7));
    if let Value::Object(members) = schema.take() {
        root.extend(members);
    }
    Value::Object(root)
}

/// The schema of one envelope.
pub(crate) fn schema(envelope: EnvelopeSchema) -> Value {
    match envelope {
        EnvelopeSchema::ComponentTuple => document(tuple()),
        EnvelopeSchema::ProfileSpecificationTuple => document(closed(&[
            ("identity", token()),
            ("revision", token()),
            ("digest", digest()),
            ("semanticDigest", digest()),
        ])),
        EnvelopeSchema::ConfigurationSchemaAuthoritySet => {
            let member = closed(&[
                ("identity", token()),
                ("revision", token()),
                ("dialect", json!({ "type": "string", "minLength": 1 })),
                ("digest", digest()),
            ]);
            document(closed(&[
                (
                    "profileSpecification",
                    closed(&[
                        ("identity", token()),
                        ("revision", token()),
                        ("semanticDigest", digest()),
                    ]),
                ),
                (
                    "members",
                    json!({
                        "type": "object",
                        "minProperties": 1,
                        "propertyNames": { "pattern": ID_PATTERN },
                        "additionalProperties": member,
                    }),
                ),
            ]))
        }
        EnvelopeSchema::RegistryPackage => {
            let roles = [
                "safe-origin-kind-registry-schema",
                "check-registry-schema",
                "finding-registry-schema",
                "safe-origin-kind-registry",
                "check-registry",
                "finding-registry",
            ];
            let members = roles.map(|role| (role, tuple()));
            document(closed(&[("members", closed(&members))]))
        }
    }
}

/// The package's body: every envelope schema under its name.
pub(crate) fn package() -> Value {
    let mut schemas = Map::new();
    for envelope in [
        EnvelopeSchema::ComponentTuple,
        EnvelopeSchema::ProfileSpecificationTuple,
        EnvelopeSchema::ConfigurationSchemaAuthoritySet,
        EnvelopeSchema::RegistryPackage,
    ] {
        let name = serde_json::to_value(envelope)
            .ok()
            .and_then(|name| name.as_str().map(str::to_owned))
            .expect("an envelope schema is named by a string");
        schemas.insert(name, schema(envelope));
    }
    json!({ "schemas": schemas })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn validator(envelope: EnvelopeSchema) -> jsonschema::Validator {
        jsonschema::draft7::new(&schema(envelope)).unwrap()
    }

    const DIGEST: &str = "sha256:0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn every_envelope_schema_is_a_valid_draft_7_schema() {
        let meta = jsonschema::draft7::meta::validator();
        for envelope in [
            EnvelopeSchema::ComponentTuple,
            EnvelopeSchema::ProfileSpecificationTuple,
            EnvelopeSchema::ConfigurationSchemaAuthoritySet,
            EnvelopeSchema::RegistryPackage,
        ] {
            let schema = schema(envelope);
            assert_eq!(schema["$schema"], DRAFT_7, "{envelope:?}");
            assert!(meta.is_valid(&schema), "{envelope:?}");
        }
    }

    #[test]
    fn a_tuple_is_closed_and_checks_form_only() {
        let tuple = validator(EnvelopeSchema::ComponentTuple);
        let valid =
            json!({ "identity": "intlify-project-profile", "revision": "0", "digest": DIGEST });
        assert!(tuple.is_valid(&valid));
        // Another identity is still the right form: which identity is admitted
        // is a precedence row of its own.
        assert!(
            tuple.is_valid(&json!({ "identity": "another", "revision": "9", "digest": DIGEST }))
        );
        for invalid in [
            json!({ "identity": "intlify-project-profile", "revision": "0" }),
            json!({ "identity": "Upper", "revision": "0", "digest": DIGEST }),
            json!({ "identity": "intlify-project-profile", "revision": "0", "digest": "sha256:00" }),
            json!({ "identity": "intlify-project-profile", "revision": "0", "digest": DIGEST, "extra": true }),
        ] {
            assert!(!tuple.is_valid(&invalid), "{invalid}");
        }
        let profile = validator(EnvelopeSchema::ProfileSpecificationTuple);
        assert!(!profile.is_valid(&valid));
        let mut with_semantic = valid;
        with_semantic["semanticDigest"] = json!(DIGEST);
        assert!(profile.is_valid(&with_semantic));
    }

    #[test]
    fn the_configuration_schema_authority_set_maps_schema_versions_to_members() {
        let set = validator(EnvelopeSchema::ConfigurationSchemaAuthoritySet);
        let member = json!({
            "identity": "intlify-configuration-schema",
            "revision": "0",
            "dialect": DRAFT_7,
            "digest": DIGEST
        });
        let valid = json!({
            "profileSpecification": { "identity": "intlify-project-profile", "revision": "0", "semanticDigest": DIGEST },
            "members": { "0": member }
        });
        assert!(set.is_valid(&valid));
        let mut empty = valid.clone();
        empty["members"] = json!({});
        assert!(!set.is_valid(&empty));
        let mut bad_version = valid.clone();
        bad_version["members"] = json!({ "Zero": member });
        assert!(!set.is_valid(&bad_version));
        let mut missing_dialect = valid;
        missing_dialect["members"]["0"]
            .as_object_mut()
            .unwrap()
            .remove("dialect");
        assert!(!set.is_valid(&missing_dialect));
    }

    #[test]
    fn the_registry_package_holds_exactly_the_six_roles() {
        let package = validator(EnvelopeSchema::RegistryPackage);
        let tuple = json!({ "identity": "intlify-project-profile-check-registry", "revision": "0", "digest": DIGEST });
        let mut members = Map::new();
        for role in [
            "safe-origin-kind-registry-schema",
            "check-registry-schema",
            "finding-registry-schema",
            "safe-origin-kind-registry",
            "check-registry",
            "finding-registry",
        ] {
            members.insert(role.to_owned(), tuple.clone());
        }
        let valid = json!({ "members": members });
        assert!(package.is_valid(&valid));
        let mut missing = valid.clone();
        missing["members"]
            .as_object_mut()
            .unwrap()
            .remove("check-registry");
        assert!(!package.is_valid(&missing));
        let mut unknown = valid;
        unknown["members"]["another-registry"] = tuple;
        assert!(!package.is_valid(&unknown));
    }

    #[test]
    fn the_package_holds_every_envelope_schema_by_name() {
        let package = package();
        let names = package["schemas"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            [
                "component-tuple",
                "profile-specification-tuple",
                "configuration-schema-authority-set",
                "registry-package"
            ]
        );
    }
}
