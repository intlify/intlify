// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The coverage Handoff Vectors for 020: what a requirement for one checked
//! Intent at one requested locale on one target concludes from the
//! profile's coverage decision table, its fallback policy, the Intent's
//! source locale, and the eligibility of the definitions a Store offers
//! (D15:1552-1629).
//!
//! A target that excludes the requested locale creates no requirement and
//! no coverage debt for the pair (015-COV-247). Otherwise the requirement
//! records the cell's effective mode and whether it is source-equal, its
//! requested locale equal to the Intent's source locale (D15:1559, 1616):
//!
//! - source-equal: the checked source fulfills it and creates no Provider
//!   work (D15:1627);
//! - `direct-required`: only an eligible direct definition fulfills it; an
//!   eligible fallback cannot make a missing, stale, invalid, or unapproved
//!   direct definition admissible, and the direct state is the blocking
//!   cause (D15:1623);
//! - `fallback-allowed`: an eligible direct definition fulfills it;
//!   otherwise the first eligible candidate of the fallback order does, with
//!   a visible coverage debt naming the direct state and the selected
//!   definition locale; with none it blocks (D15:1625).
//!
//! A requirement that is not source-equal keeps its Provider demand, the
//! direct localization, whatever fulfills it (D15:1625). Each vector hands
//! over `coverage`, `messageFallback`, `projectLocales`, and `targets`, so its
//! bindings are the Target Profiles. 020 owns the form of the inputs and the
//! relationship (D15:3330), which these spell in 015's words until it
//! defines it; the Release and the Provider work themselves are 025's and
//! 022's.

use serde_json::{json, Map, Value};

/// The decisions every coverage vector rests on: the coverage decision
/// table and its modes, and the fallback policy whose order a
/// `fallback-allowed` requirement selects from.
pub(super) const DECISIONS: [&str; 2] = ["015-030", "015-031"];

/// A definition's state as a Store offers it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum State {
    Eligible,
    Missing,
    Stale,
    Invalid,
    Unapproved,
}

impl State {
    const fn id(self) -> &'static str {
        match self {
            Self::Eligible => "eligible",
            Self::Missing => "missing",
            Self::Stale => "stale",
            Self::Invalid => "invalid",
            Self::Unapproved => "unapproved",
        }
    }
}

/// What a requirement concludes, as the table states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Concluded {
    /// The target excludes the requested locale.
    NoRequirement,
    /// The checked source fulfills it.
    Source,
    /// The eligible direct definition fulfills it.
    Direct,
    /// This fallback definition locale fulfills it, with a coverage debt.
    Fallback(&'static str),
    /// It blocks, the direct state its cause.
    Blocked,
}

/// One vector: its name, whether the fallback storefront produces it, the
/// target, the requested locale, the surface class, the Intent's source
/// locale, the direct definition's state when the requirement has one, the
/// fallback definitions' states, and the conclusion.
pub(super) struct Vector {
    pub(super) name: &'static str,
    pub(super) fallback_storefront: bool,
    pub(super) target: &'static str,
    pub(super) requested: &'static str,
    pub(super) surface: &'static str,
    pub(super) source: &'static str,
    pub(super) direct: Option<State>,
    pub(super) fallbacks: &'static [(&'static str, State)],
    pub(super) concluded: Concluded,
}

/// Each conclusion 015's coverage rules give: over the storefront, whose
/// `ja` is `fallback-allowed` under the order `ja -> [en, Intent source
/// locale]` and whose `en` is `direct-required`, and over the fallback
/// storefront, whose `en` is `direct-required` under `en -> [fr]` and whose
/// `ja-JP` is in no target's subset.
pub(super) const VECTORS: [Vector; 12] = {
    use Concluded::{Blocked, Direct, Fallback, NoRequirement, Source};
    use State::{Eligible, Invalid, Missing, Stale, Unapproved};
    [
        Vector {
            name: "fallback-allowed-direct",
            fallback_storefront: false,
            target: "browser",
            requested: "ja",
            surface: "nav",
            source: "en",
            direct: Some(Eligible),
            fallbacks: &[("en", Eligible)],
            concluded: Direct,
        },
        Vector {
            name: "fallback-allowed-first-candidate",
            fallback_storefront: false,
            target: "browser",
            requested: "ja",
            surface: "nav",
            source: "de",
            direct: Some(Missing),
            fallbacks: &[("en", Eligible), ("de", Eligible)],
            concluded: Fallback("en"),
        },
        Vector {
            name: "fallback-allowed-later-candidate",
            fallback_storefront: false,
            target: "server",
            requested: "ja",
            surface: "checkout",
            source: "de",
            direct: Some(Stale),
            fallbacks: &[("en", Unapproved), ("de", Eligible)],
            concluded: Fallback("de"),
        },
        Vector {
            name: "fallback-allowed-no-eligible-candidate",
            fallback_storefront: false,
            target: "browser",
            requested: "ja",
            surface: "nav",
            source: "de",
            direct: Some(Missing),
            fallbacks: &[("en", Missing), ("de", Invalid)],
            concluded: Blocked,
        },
        Vector {
            name: "fallback-allowed-source-equal",
            fallback_storefront: false,
            target: "browser",
            requested: "ja",
            surface: "nav",
            source: "ja",
            direct: None,
            fallbacks: &[],
            concluded: Source,
        },
        Vector {
            name: "direct-required-direct",
            fallback_storefront: false,
            target: "browser",
            requested: "en",
            surface: "checkout",
            source: "ja",
            direct: Some(Eligible),
            fallbacks: &[],
            concluded: Direct,
        },
        Vector {
            name: "direct-required-source-equal",
            fallback_storefront: false,
            target: "server",
            requested: "en",
            surface: "nav",
            source: "en",
            direct: None,
            fallbacks: &[],
            concluded: Source,
        },
        Vector {
            name: "direct-required-missing-despite-a-fallback",
            fallback_storefront: true,
            target: "browser",
            requested: "en",
            surface: "nav",
            source: "ja",
            direct: Some(Missing),
            fallbacks: &[("fr", Eligible)],
            concluded: Blocked,
        },
        Vector {
            name: "direct-required-stale",
            fallback_storefront: true,
            target: "browser",
            requested: "en",
            surface: "nav",
            source: "ja",
            direct: Some(Stale),
            fallbacks: &[("fr", Eligible)],
            concluded: Blocked,
        },
        Vector {
            name: "direct-required-invalid",
            fallback_storefront: true,
            target: "browser",
            requested: "en",
            surface: "nav",
            source: "ja",
            direct: Some(Invalid),
            fallbacks: &[("fr", Eligible)],
            concluded: Blocked,
        },
        Vector {
            name: "direct-required-unapproved",
            fallback_storefront: true,
            target: "browser",
            requested: "en",
            surface: "nav",
            source: "ja",
            direct: Some(Unapproved),
            fallbacks: &[("fr", Eligible)],
            concluded: Blocked,
        },
        Vector {
            name: "excluded-locale",
            fallback_storefront: true,
            target: "browser",
            requested: "ja-JP",
            surface: "nav",
            source: "en",
            direct: None,
            fallbacks: &[],
            concluded: NoRequirement,
        },
    ]
};

/// The fallback order of `requested` for an Intent whose source locale is
/// `source`: the configured sequence, a literal as it is and the Intent
/// source-locale candidate as `source` (D15:1503).
fn order(fallback: &Value, requested: &str, source: &str) -> Vec<String> {
    fallback["mappings"][requested]
        .as_array()
        .into_iter()
        .flatten()
        .map(|candidate| candidate.as_str().unwrap_or(source).to_owned())
        .collect()
}

/// The consumer inputs and the relationship of `vector` against the facts
/// its producer hands over, checked against what the table states.
///
/// # Panics
///
/// Panics when the conclusion differs from the table's, which the table
/// rules out.
pub(super) fn relationship(vector: &Vector, facts: &Value) -> (Value, Value) {
    let mut inputs = json!({
        "targetId": vector.target,
        "requestedLocale": vector.requested,
        "intentSurfaceClass": vector.surface,
        "intentSourceLocale": vector.source,
    });
    if let Some(state) = vector.direct {
        inputs["direct"] = json!(state.id());
    }
    if !vector.fallbacks.is_empty() {
        let offered = vector
            .fallbacks
            .iter()
            .map(|(locale, state)| ((*locale).to_owned(), json!(state.id())))
            .collect::<Map<_, _>>();
        inputs["fallbackDefinitions"] = Value::Object(offered);
    }
    let subset = facts["targets"][vector.target]["requestedLocales"]
        .as_array()
        .expect("a checked target lists its supported subset");
    if !subset.iter().any(|locale| locale == vector.requested) {
        assert_eq!(
            vector.concluded,
            Concluded::NoRequirement,
            "{}",
            vector.name
        );
        return (inputs, json!({ "requirement": "absent" }));
    }
    let mode = facts["coverage"]["decisions"][vector.requested][vector.surface].clone();
    let source_equal = vector.requested == vector.source;
    let mut relationship = json!({
        "requirement": "present",
        "mode": mode,
        "sourceEqual": source_equal,
        "providerDemand": !source_equal,
    });
    let direct = vector.direct.unwrap_or(State::Missing);
    let concluded = if source_equal {
        relationship["outcome"] = json!("source");
        relationship["definitionLocale"] = json!(vector.source);
        Concluded::Source
    } else if direct == State::Eligible {
        relationship["outcome"] = json!("direct");
        relationship["definitionLocale"] = json!(vector.requested);
        Concluded::Direct
    } else {
        let eligible = |locale: &str| {
            vector
                .fallbacks
                .iter()
                .any(|(offered, state)| *offered == locale && *state == State::Eligible)
        };
        let selected = (mode == "fallback-allowed")
            .then(|| {
                order(&facts["messageFallback"], vector.requested, vector.source)
                    .into_iter()
                    .find(|locale| eligible(locale))
            })
            .flatten();
        if let Some(locale) = selected {
            relationship["outcome"] = json!("fallback");
            relationship["definitionLocale"] = json!(locale);
            relationship["coverageDebt"] =
                json!({ "cause": direct.id(), "definitionLocale": locale });
            let named = vector
                .fallbacks
                .iter()
                .find(|(offered, _)| *offered == locale);
            Concluded::Fallback(named.map_or("", |(offered, _)| offered))
        } else {
            relationship["outcome"] = json!("blocked");
            relationship["blockingCause"] = json!(direct.id());
            Concluded::Blocked
        }
    };
    assert_eq!(concluded, vector.concluded, "{}", vector.name);
    (inputs, relationship)
}

#[cfg(test)]
mod tests {
    use super::super::tests::{canonicalizer, written};
    use super::*;

    /// The written coverage vectors.
    fn manifests() -> Vec<Value> {
        let (_, vectors) = written();
        vectors
            .files
            .iter()
            .map(|file| serde_json::from_slice::<Value>(&file.bytes).unwrap())
            .filter(|manifest| {
                manifest["id"]
                    .as_str()
                    .unwrap()
                    .starts_with("015-handoff-coverage-")
            })
            .collect()
    }

    /// 015's coverage rules read again from the vector alone: no
    /// requirement off the target's subset; otherwise the cell's mode and
    /// the source-equal state, then the source, an eligible direct
    /// definition, the first eligible candidate of the fallback order when
    /// the mode allows it, or the block, the direct state its cause.
    fn other_coverage(facts: &Value, inputs: &Value) -> Value {
        let requested = inputs["requestedLocale"].as_str().unwrap();
        let source = inputs["intentSourceLocale"].as_str().unwrap();
        let target = &facts["targets"][inputs["targetId"].as_str().unwrap()];
        if !target["requestedLocales"]
            .as_array()
            .unwrap()
            .contains(&json!(requested))
        {
            return json!({ "requirement": "absent" });
        }
        let mode = &facts["coverage"]["decisions"][requested]
            [inputs["intentSurfaceClass"].as_str().unwrap()];
        let equal = requested == source;
        let head = json!({
            "requirement": "present",
            "mode": mode,
            "sourceEqual": equal,
            "providerDemand": !equal,
        });
        let with = |members: Value| {
            let mut value = head.clone();
            for (name, member) in members.as_object().unwrap() {
                value[name] = member.clone();
            }
            value
        };
        if equal {
            return with(json!({ "outcome": "source", "definitionLocale": source }));
        }
        let direct = inputs["direct"].as_str().unwrap();
        if direct == "eligible" {
            return with(json!({ "outcome": "direct", "definitionLocale": requested }));
        }
        if mode == "fallback-allowed" {
            let offered = &inputs["fallbackDefinitions"];
            let configured = facts["messageFallback"]["mappings"][requested]
                .as_array()
                .cloned()
                .unwrap_or_default();
            for candidate in configured {
                let locale = candidate.as_str().unwrap_or(source);
                if offered[locale] == "eligible" {
                    return with(json!({
                        "outcome": "fallback",
                        "definitionLocale": locale,
                        "coverageDebt": { "cause": direct, "definitionLocale": locale },
                    }));
                }
            }
        }
        with(json!({ "outcome": "blocked", "blockingCause": direct }))
    }

    #[test]
    fn each_requirement_is_what_015_s_coverage_rules_conclude() {
        let manifests = manifests();
        assert_eq!(manifests.len(), VECTORS.len());
        for manifest in &manifests {
            assert_eq!(
                manifest["expectedRelationship"]["json"],
                other_coverage(
                    &manifest["producerProfileFacts"],
                    &manifest["consumerInputs"]["json"]
                ),
                "{}",
                manifest["id"]
            );
        }
    }

    #[test]
    fn a_requirement_keeps_its_demand_and_never_falls_back_when_direct_is_required() {
        for manifest in manifests() {
            let relationship = &manifest["expectedRelationship"]["json"];
            if relationship["requirement"] == "absent" {
                continue;
            }
            // Only a source-equal requirement drops its Provider demand.
            assert_eq!(
                relationship["providerDemand"],
                json!(relationship["sourceEqual"] == false),
                "{}",
                manifest["id"]
            );
            // A fallback always carries its debt, and only where allowed.
            let fallback = relationship["outcome"] == "fallback";
            assert_eq!(relationship.get("coverageDebt").is_some(), fallback);
            if fallback {
                assert_eq!(
                    relationship["mode"], "fallback-allowed",
                    "{}",
                    manifest["id"]
                );
            }
        }
        // Every direct state but eligible blocks a direct-required
        // requirement despite an eligible fallback.
        let causes = manifests()
            .iter()
            .filter(|manifest| {
                let relationship = &manifest["expectedRelationship"]["json"];
                relationship["mode"] == "direct-required" && relationship["outcome"] == "blocked"
            })
            .map(|manifest| {
                manifest["expectedRelationship"]["json"]["blockingCause"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            causes,
            ["invalid", "missing", "stale", "unapproved"]
                .map(str::to_owned)
                .into()
        );
    }

    #[test]
    fn each_vector_hands_over_the_coverage_inputs_to_020() {
        let canonicalizer = canonicalizer();
        for manifest in manifests() {
            let facts = manifest["producerProfileFacts"].as_object().unwrap();
            let groups = facts.keys().map(String::as_str).collect::<Vec<_>>();
            assert_eq!(
                groups,
                ["coverage", "messageFallback", "projectLocales", "targets"]
            );
            assert_eq!(manifest["verificationOwner"], "020");
        }
        for vector in &VECTORS {
            let mut locales = vec![vector.requested, vector.source];
            locales.extend(vector.fallbacks.iter().map(|(locale, _)| *locale));
            for locale in locales {
                let canonical = canonicalizer.canonicalize(locale).unwrap();
                assert_eq!(canonical.into_locale().as_str(), locale, "{}", vector.name);
            }
        }
    }
}
