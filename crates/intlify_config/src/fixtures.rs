// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite test-owned reference encodings. These are not 017 artifacts.
//!
//! The fixture discriminants intentionally advertise their non-product scope;
//! neither type is compiled into the ordinary library. The non-default benchmark
//! feature uses the same bounded fixtures, not a second test reference encoding.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::model::IntlifyConfig;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixturePolicyReference {
    #[serde(rename = "$testPolicy")]
    token: PolicyToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum PolicyToken {
    ResourceLimits,
    Trust,
    SourceAdmission,
    Approval,
    Selection,
    ProviderRouting,
    GlossarySet,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureTargetReference {
    #[serde(rename = "$testTarget")]
    token: TargetToken,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
enum TargetToken {
    Browser,
    Server,
}

pub(crate) type FixtureConfig = IntlifyConfig<FixturePolicyReference, FixtureTargetReference>;

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
                    "resourceLimits": {"$testPolicy": "resource-limits"},
                    "trust": {"$testPolicy": "trust"},
                    "sourceAdmission": {"$testPolicy": "source-admission"},
                    "approval": {"$testPolicy": "approval"},
                    "selection": {"$testPolicy": "selection"},
                    "providerRouting": null,
                    "glossarySet": null
                },
                "targetProfiles": {
                    "browser": {
                        "profile": {"$testTarget": "browser"},
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
    app["policies"]["providerRouting"] = json!({"$testPolicy": "provider-routing"});
    app["policies"]["glossarySet"] = json!({"$testPolicy": "glossary-set"});
    app["targetProfiles"]["browser"]["defaultRequestedLocale"] = json!("en");
    app["targetProfiles"]["server"] =
        json!({"profile": {"$testTarget": "server"}, "requestedLocales": ["en"]});
    app["deploymentGroups"]["web"] = json!({
        "members": ["browser", "server"],
        "hydrationRelations": [{"server": "server", "client": "browser"}]
    });
    app["delivery"] = json!({"placement": "duplicate"});
    value
}
