// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite inputs using the formal 015/017 configuration representation.
//! Compiled only for tests and the non-default benchmark feature. Reference pins
//! prove structure only: no body is acquired, admitted, or assigned invented trust.

use serde_json::{json, Value};

pub(crate) fn reference(kind: &str) -> Value {
    json!({
        "kind": kind,
        "identity": "fixture-artifact",
        "revision": "1",
        "specificationRevision": "0",
        "semanticDigest": "sha256:1111111111111111111111111111111111111111111111111111111111111111"
    })
}

pub(crate) fn minimal_config() -> Value {
    json!({
        "schemaVersion": "0",
        "profiles": {
            "app": {
                "projectId": "storefront",
                "selectionScope": "storefront.production",
                "requestedLocales": ["en"],
                "defaultRequestedLocale": "en",
                "policies": {
                    "resourceLimits": reference("resource-limit-policy"),
                    "trust": reference("trust-policy"),
                    "sourceAdmission": reference("source-admission-policy"),
                    "approval": reference("approval-policy"),
                    "selection": reference("selection-policy"),
                    "providerRouting": null,
                    "glossarySet": null
                },
                "targetProfiles": {
                    "browser": {
                        "profile": reference("target-profile"),
                        "requestedLocales": ["en"]
                    }
                },
                "deploymentGroups": {"web": {"members": ["browser"]}}
            }
        }
    })
}

pub(crate) fn complete_config() -> Value {
    let mut value = minimal_config();
    value["$schema"] = json!("https://invalid.example/editor-only-schema");
    let app = &mut value["profiles"]["app"];
    app["defaultSourceLocale"] = json!("en");
    app["localeNegotiation"] = json!({"aliases": {"fr": "fr-FR"}});
    app["messageFallback"] = json!({"ja": ["en", {"kind": "intent-source-locale"}]});
    app["coverage"] = json!({
        "defaultMode": "direct-required",
        "rules": [
            {"requestedLocales": ["ja"], "mode": "fallback-allowed"},
            {"intentSurfaceClasses": ["checkout"], "mode": "direct-required"},
            {"requestedLocales": ["ja"], "intentSurfaceClasses": ["checkout"], "mode": "fallback-allowed"}
        ]
    });
    app["policies"]["providerRouting"] = reference("provider-routing-policy");
    app["policies"]["glossarySet"] = reference("glossary-set");
    app["targetProfiles"]["browser"]["defaultRequestedLocale"] = json!("en");
    app["targetProfiles"]["server"] =
        json!({"profile": reference("target-profile"), "requestedLocales": ["en"]});
    app["deploymentGroups"]["web"] = json!({
        "members": ["browser", "server"],
        "hydrationRelations": [{"server": "server", "client": "browser"}]
    });
    app["delivery"] = json!({"placement": "duplicate"});
    value
}
