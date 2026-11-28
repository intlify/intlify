// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The negotiation Handoff Vectors for 027: what the portable lookup that
//! Locale Negotiation Specification revision `"0"` selects concludes from
//! one target's supported subset and effective default, the project's
//! aliases, and an application's ordered preferences, or what a direct
//! selection concludes (D15:1416-1484).
//!
//! The producer is the negotiation storefront. Its browser and server
//! support `de`, `en`, `fr-FR`, `ja`, `zh`, and `zh-Hant`, `en` by default,
//! its worker `en` and `ja`, `ja` by default, and its aliases are
//! `en-AU -> fr-FR`, `fr -> fr-FR`, and `fr-FR -> en`. Each vector hands over
//! the canonicalization specification, the negotiation profile, and the
//! targets, so its bindings are the three Target Profiles. Its
//! `consumerInputs` hold the Target ID and the preferences, each a canonical
//! spelling but the one invalid preference, or the directly selected locale.
//! Its `expectedRelationship` holds the selected locale, or the effective
//! default when nothing matches, with the lookup's trace: each preference
//! the lookup examined, in order, with each candidate it probed and what the
//! probe found, an exact member, an applicable or inapplicable alias, or no
//! match. An invalid preference fails the whole invocation, and a direct
//! selection outside the subset fails without negotiating or defaulting.
//! 027 owns the form of both (D15:3330), which these spell in 015's words
//! until 027 defines it.

use serde_json::{json, Map, Value};

/// The decisions every negotiation vector rests on: the negotiation
/// specification and profile, the shared hydration inputs, the portable
/// lookup, and 027's ownership of the invocation.
pub(super) const DECISIONS: [&str; 4] = ["015-029", "015-055", "015-063", "015-207"];

/// What an application submits to one invocation.
#[derive(Debug, Clone, Copy)]
pub(super) enum Request {
    /// An ordered preference sequence.
    Preferences(&'static [&'static str]),
    /// A direct selection.
    Direct(&'static str),
}

/// What an invocation concludes, as the table states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Concluded {
    /// This locale, an exact member at the matching candidate, or the
    /// directly selected member.
    Exact(&'static str),
    /// This locale, an alias's destination at the matching candidate.
    Alias(&'static str),
    /// The effective default: no preference matches.
    Default,
    /// A failure: the preference at this position is invalid.
    Invalid(usize),
    /// A failure: the direct selection is no member of the subset.
    Unsupported,
}

/// One vector: its name, the Target ID, the request, and the conclusion.
pub(super) struct Vector {
    pub(super) name: &'static str,
    pub(super) target: &'static str,
    pub(super) request: Request,
    pub(super) concluded: Concluded,
}

const fn vector(
    name: &'static str,
    target: &'static str,
    request: Request,
    concluded: Concluded,
) -> Vector {
    Vector {
        name,
        target,
        request,
        concluded,
    }
}

/// Each behavior of 015's portable lookup and direct selection, on the
/// browser, the server, or the worker.
pub(super) const VECTORS: [Vector; 20] = {
    use Concluded::{Alias, Default, Exact, Invalid, Unsupported};
    use Request::{Direct, Preferences};
    [
        vector(
            "browser-exact-before-alias",
            "browser",
            Preferences(&["fr-FR"]),
            Exact("fr-FR"),
        ),
        vector(
            "browser-preference-order",
            "browser",
            Preferences(&["ja", "de"]),
            Exact("ja"),
        ),
        vector(
            "browser-alias",
            "browser",
            Preferences(&["fr"]),
            Alias("fr-FR"),
        ),
        vector(
            "server-alias",
            "server",
            Preferences(&["fr"]),
            Alias("fr-FR"),
        ),
        vector(
            "worker-alias-inapplicable",
            "worker",
            Preferences(&["fr"]),
            Default,
        ),
        vector(
            "worker-alias-of-an-unsupported-locale",
            "worker",
            Preferences(&["fr-FR"]),
            Alias("en"),
        ),
        vector(
            "browser-region-alias",
            "browser",
            Preferences(&["en-AU"]),
            Alias("fr-FR"),
        ),
        vector(
            "worker-truncation-after-an-inapplicable-alias",
            "worker",
            Preferences(&["en-AU"]),
            Exact("en"),
        ),
        vector(
            "browser-unicode-extension",
            "browser",
            Preferences(&["zh-Hant-TW-u-ca-chinese"]),
            Exact("zh-Hant"),
        ),
        vector(
            "browser-transformed-extension",
            "browser",
            Preferences(&["en-t-ja-jp-u-nu-latn"]),
            Exact("en"),
        ),
        vector(
            "browser-variants",
            "browser",
            Preferences(&["de-CH-1996-fonipa"]),
            Exact("de"),
        ),
        vector("browser-und", "browser", Preferences(&["und"]), Default),
        vector(
            "worker-whole-extension",
            "worker",
            Preferences(&["de-DE-u-ca-gregory-nu-latn"]),
            Default,
        ),
        vector(
            "browser-no-likely-subtag",
            "browser",
            Preferences(&["zh-TW"]),
            Exact("zh"),
        ),
        vector(
            "worker-repeated-preference",
            "worker",
            Preferences(&["zh-TW", "zh-TW", "ja"]),
            Exact("ja"),
        ),
        vector(
            "browser-invalid-preference",
            "browser",
            Preferences(&["en-x-brand", "ja"]),
            Invalid(0),
        ),
        vector("browser-empty", "browser", Preferences(&[]), Default),
        vector(
            "worker-exhausted",
            "worker",
            Preferences(&["de", "zh"]),
            Default,
        ),
        vector(
            "browser-direct",
            "browser",
            Direct("zh-Hant"),
            Exact("zh-Hant"),
        ),
        vector(
            "worker-direct-unsupported",
            "worker",
            Direct("zh-Hant"),
            Unsupported,
        ),
    ]
};

/// The structural parts of a canonical locale: its language, script,
/// region, variants, and extensions, each extension its singleton and its
/// subtags.
struct Parts<'a> {
    language: &'a str,
    script: Option<&'a str>,
    region: Option<&'a str>,
    variants: Vec<&'a str>,
    extensions: Vec<Vec<&'a str>>,
}

impl<'a> Parts<'a> {
    fn of(locale: &'a str) -> Self {
        let mut subtags = locale.split('-');
        let mut parts = Self {
            language: subtags.next().unwrap_or_default(),
            script: None,
            region: None,
            variants: Vec::new(),
            extensions: Vec::new(),
        };
        for subtag in subtags {
            let alphabetic = subtag.bytes().all(|byte| byte.is_ascii_alphabetic());
            let numeric = subtag.bytes().all(|byte| byte.is_ascii_digit());
            let before_variants = parts.region.is_none() && parts.variants.is_empty();
            if subtag.len() == 1 {
                parts.extensions.push(vec![subtag]);
            } else if let Some(extension) = parts.extensions.last_mut() {
                extension.push(subtag);
            } else if subtag.len() == 4 && alphabetic && parts.script.is_none() && before_variants {
                parts.script = Some(subtag);
            } else if ((subtag.len() == 2 && alphabetic) || (subtag.len() == 3 && numeric))
                && before_variants
            {
                parts.region = Some(subtag);
            } else {
                parts.variants.push(subtag);
            }
        }
        parts
    }

    fn text(&self) -> String {
        let mut subtags = vec![self.language];
        subtags.extend(self.script);
        subtags.extend(self.region);
        subtags.extend(&self.variants);
        subtags.extend(self.extensions.iter().flatten());
        subtags.join("-")
    }
}

/// The portable lookup candidate sequence of the canonical locale `locale`
/// (D15:1432-1441): the complete locale, then without its rightmost
/// complete extension while one remains, its rightmost variant while one
/// remains, its region, and its script, down to its language.
pub(super) fn candidates(locale: &str) -> Vec<String> {
    let mut parts = Parts::of(locale);
    let mut sequence = vec![parts.text()];
    while parts.extensions.pop().is_some() {
        sequence.push(parts.text());
    }
    while parts.variants.pop().is_some() {
        sequence.push(parts.text());
    }
    if parts.region.take().is_some() {
        sequence.push(parts.text());
    }
    if parts.script.take().is_some() {
        sequence.push(parts.text());
    }
    sequence
}

/// What the portable lookup concludes for `preferences` on a target whose
/// supported subset is `supported` and whose effective default is
/// `default`, under the alias map `aliases` (D15:1466-1474): at each
/// candidate, an exact member, else a direct alias whose destination is a
/// member, else the next candidate of the same preference.
pub(super) fn lookup(
    preferences: &[&str],
    supported: &[Value],
    default: &Value,
    aliases: &Map<String, Value>,
) -> Value {
    let member = |locale: &str| supported.iter().any(|held| held == locale);
    let mut trace = Vec::new();
    for preference in preferences {
        let mut probes = Vec::new();
        for candidate in candidates(preference) {
            let found = if member(&candidate) {
                Some((
                    json!({ "candidate": candidate, "result": "exact" }),
                    json!(candidate),
                ))
            } else {
                match aliases.get(&candidate) {
                    Some(destination) if destination.as_str().is_some_and(member) => Some((
                        json!({ "candidate": candidate, "result": "alias", "destination": destination }),
                        destination.clone(),
                    )),
                    Some(destination) => {
                        probes.push(json!({
                            "candidate": candidate,
                            "result": "alias-inapplicable",
                            "destination": destination,
                        }));
                        None
                    }
                    None => {
                        probes.push(json!({ "candidate": candidate, "result": "no-match" }));
                        None
                    }
                }
            };
            if let Some((probe, locale)) = found {
                probes.push(probe);
                trace.push(json!({ "preference": preference, "probes": probes }));
                return json!({ "outcome": "selected", "locale": locale, "trace": trace });
            }
        }
        trace.push(json!({ "preference": preference, "probes": probes }));
    }
    json!({ "outcome": "default", "locale": default, "trace": trace })
}

/// The consumer inputs and the relationship of `vector` against the
/// `targets` and `negotiation` facts its producer hands over, the latter
/// checked against what the table states.
///
/// # Panics
///
/// Panics when the lookup concludes otherwise than the table, which the
/// table rules out.
pub(super) fn relationship(
    vector: &Vector,
    targets: &Value,
    negotiation: &Value,
) -> (Value, Value) {
    let target = &targets[vector.target];
    let supported = target["requestedLocales"]
        .as_array()
        .expect("a checked target lists its supported subset");
    let aliases = negotiation["aliases"]
        .as_object()
        .expect("a checked negotiation profile maps its aliases");
    match vector.request {
        Request::Direct(locale) => {
            let inputs = json!({ "targetId": vector.target, "direct": locale });
            let relationship = if supported.iter().any(|held| held == locale) {
                assert_eq!(
                    vector.concluded,
                    Concluded::Exact(locale),
                    "{}",
                    vector.name
                );
                json!({ "outcome": "selected", "locale": locale })
            } else {
                assert_eq!(vector.concluded, Concluded::Unsupported, "{}", vector.name);
                json!({ "outcome": "failure", "reason": "unsupported-direct-selection" })
            };
            (inputs, relationship)
        }
        Request::Preferences(preferences) => {
            let inputs = json!({ "targetId": vector.target, "preferences": preferences });
            if let Concluded::Invalid(at) = vector.concluded {
                let failure = json!({ "outcome": "failure", "reason": "invalid-preference", "preference": at });
                return (inputs, failure);
            }
            let relationship = lookup(
                preferences,
                supported,
                &target["effectiveDefaultRequestedLocale"],
                aliases,
            );
            assert_eq!(
                conclusion(&relationship),
                vector.concluded,
                "{}",
                vector.name
            );
            (inputs, relationship)
        }
    }
}

/// What a lookup's `relationship` concludes: the locale its last probe
/// selects, exactly or through an alias, or the effective default.
fn conclusion(relationship: &Value) -> Concluded {
    let probe = relationship["trace"]
        .as_array()
        .and_then(|trace| trace.last())
        .and_then(|examined| examined["probes"].as_array())
        .and_then(|probes| probes.last());
    let locale = relationship["locale"].as_str().unwrap_or_default();
    let named = VECTORS.iter().find_map(|vector| match vector.concluded {
        Concluded::Exact(named) | Concluded::Alias(named) if named == locale => Some(named),
        _ => None,
    });
    match (
        relationship["outcome"].as_str(),
        probe.map(|probe| &probe["result"]),
        named,
    ) {
        (Some("selected"), Some(result), Some(named)) if result == "exact" => {
            Concluded::Exact(named)
        }
        (Some("selected"), Some(result), Some(named)) if result == "alias" => {
            Concluded::Alias(named)
        }
        (Some("default"), ..) => Concluded::Default,
        _ => panic!("a lookup selects through its last probe or falls back to the default"),
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::canonicalizer;
    use super::*;
    use crate::conformance::resolver_cases::NEGOTIATION;

    /// The written negotiation vectors.
    fn manifests() -> Vec<Value> {
        let (_, vectors) = super::super::tests::written();
        vectors
            .files
            .iter()
            .map(|file| serde_json::from_slice::<Value>(&file.bytes).unwrap())
            .filter(|manifest| manifest["producerCaseId"] == NEGOTIATION)
            .collect()
    }

    /// The candidates of a canonical locale, found another way: drop
    /// everything from the last singleton while one remains, then the last
    /// subtag until the language is left. Canonical order puts the script,
    /// the region, and the variants after the language in that order, so
    /// dropping the last subtag removes them in the order 015 states.
    fn other_candidates(locale: &str) -> Vec<String> {
        let mut current = locale.split('-').collect::<Vec<_>>();
        let mut sequence = vec![current.join("-")];
        while current.len() > 1 {
            let cut = current
                .iter()
                .rposition(|subtag| subtag.len() == 1)
                .unwrap_or(current.len() - 1);
            current.truncate(cut);
            sequence.push(current.join("-"));
        }
        sequence
    }

    /// The lookup, written from 015's three checks at each candidate
    /// (D15:1466-1472) over `other_candidates`.
    fn other_lookup(facts: &Value, inputs: &Value) -> Value {
        let target = &facts["targets"][inputs["targetId"].as_str().unwrap()];
        let held = |locale: &str| {
            target["requestedLocales"]
                .as_array()
                .unwrap()
                .iter()
                .any(|member| member == locale)
        };
        if let Some(direct) = inputs["direct"].as_str() {
            return if held(direct) {
                json!({ "outcome": "selected", "locale": direct })
            } else {
                json!({ "outcome": "failure", "reason": "unsupported-direct-selection" })
            };
        }
        let aliases = &facts["localeNegotiation"]["aliases"];
        let mut trace = Vec::new();
        for preference in inputs["preferences"].as_array().unwrap() {
            let preference = preference.as_str().unwrap();
            let mut probes = Vec::new();
            let mut selected = None;
            for candidate in other_candidates(preference) {
                let destination = aliases[candidate.as_str()].as_str();
                let (probe, found) = match destination {
                    _ if held(&candidate) => ("exact", Some(candidate.clone())),
                    Some(to) if held(to) => ("alias", Some(to.to_owned())),
                    Some(_) => ("alias-inapplicable", None),
                    None => ("no-match", None),
                };
                let mut entry = json!({ "candidate": candidate, "result": probe });
                if probe != "exact" && probe != "no-match" {
                    entry["destination"] = json!(destination);
                }
                probes.push(entry);
                if found.is_some() {
                    selected = found;
                    break;
                }
            }
            trace.push(json!({ "preference": preference, "probes": probes }));
            if let Some(locale) = selected {
                return json!({ "outcome": "selected", "locale": locale, "trace": trace });
            }
        }
        json!({
            "outcome": "default",
            "locale": target["effectiveDefaultRequestedLocale"],
            "trace": trace,
        })
    }

    #[test]
    fn the_candidates_are_015_s_examples() {
        let cases: [(&str, &[&str]); 7] = [
            (
                "zh-Hant-TW-u-ca-chinese",
                &["zh-Hant-TW-u-ca-chinese", "zh-Hant-TW", "zh-Hant", "zh"],
            ),
            (
                "en-t-ja-jp-u-nu-latn",
                &["en-t-ja-jp-u-nu-latn", "en-t-ja-jp", "en"],
            ),
            (
                "de-DE-u-ca-gregory-nu-latn",
                &["de-DE-u-ca-gregory-nu-latn", "de-DE", "de"],
            ),
            (
                "de-CH-1996-fonipa",
                &["de-CH-1996-fonipa", "de-CH-1996", "de-CH", "de"],
            ),
            ("sr-Latn-RS", &["sr-Latn-RS", "sr-Latn", "sr"]),
            ("es-419", &["es-419", "es"]),
            ("und", &["und"]),
        ];
        for (locale, expected) in cases {
            assert_eq!(candidates(locale), expected, "{locale}");
            assert_eq!(other_candidates(locale), expected, "{locale}");
        }
    }

    #[test]
    fn each_relationship_is_what_an_independent_lookup_concludes() {
        let manifests = manifests();
        assert_eq!(manifests.len(), VECTORS.len());
        for manifest in &manifests {
            let inputs = &manifest["consumerInputs"]["json"];
            let relationship = &manifest["expectedRelationship"]["json"];
            if relationship["reason"] == "invalid-preference" {
                continue;
            }
            assert_eq!(
                *relationship,
                other_lookup(&manifest["producerProfileFacts"], inputs),
                "{}",
                manifest["id"]
            );
        }
    }

    /// Each preference and direct selection is a canonical spelling, so its
    /// canonicalization leaves it as it is, but the one invalid preference,
    /// which is outside the admitted domain (D15:1288, 1430).
    #[test]
    fn each_preference_is_canonical_and_the_invalid_one_is_refused() {
        let canonicalizer = canonicalizer();
        for vector in &VECTORS {
            let submitted = match vector.request {
                Request::Preferences(preferences) => preferences.to_vec(),
                Request::Direct(locale) => vec![locale],
            };
            for (at, locale) in submitted.iter().enumerate() {
                let canonical = canonicalizer.canonicalize(locale);
                if vector.concluded == Concluded::Invalid(at) {
                    assert!(canonical.is_err(), "{}: {locale}", vector.name);
                } else {
                    let canonical = canonical.unwrap();
                    assert_eq!(canonical.into_locale().as_str(), *locale, "{}", vector.name);
                }
            }
        }
    }

    #[test]
    fn each_vector_hands_over_the_negotiation_inputs_to_027() {
        for manifest in manifests() {
            let facts = manifest["producerProfileFacts"].as_object().unwrap();
            let groups = facts.keys().map(String::as_str).collect::<Vec<_>>();
            assert_eq!(
                groups,
                ["localeCanonicalization", "localeNegotiation", "targets"]
            );
            assert_eq!(manifest["verificationOwner"], "027");
            let target = manifest["consumerInputs"]["json"]["targetId"]
                .as_str()
                .unwrap();
            assert!(facts["targets"].get(target).is_some(), "{target}");
        }
        // The same preferences negotiate differently from two subsets, and
        // alike on both hydration endpoints.
        let manifests = manifests();
        let locale = |name: &str| {
            manifests
                .iter()
                .find(|manifest| manifest["id"] == format!("015-handoff-negotiation-{name}"))
                .unwrap()["expectedRelationship"]["json"]["locale"]
                .clone()
        };
        assert_eq!(locale("browser-alias"), "fr-FR");
        assert_eq!(locale("server-alias"), "fr-FR");
        assert_eq!(locale("worker-alias-inapplicable"), "ja");
    }
}
