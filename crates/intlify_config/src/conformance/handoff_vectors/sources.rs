// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Handoff Vectors of an Intent's source locale, for 016, and of the
//! message locale fallback sequence, for 020 (D15:1369-1375, 1494-1522).
//!
//! 016 resolves every Intent to exactly one source locale. An
//! application-owned Intent that omits its source locale inherits the
//! project's `defaultSourceLocale`, and blocks at 016's source-authoring
//! stage when the profile records no default; an explicit application
//! source locale stays authoritative over the default; and a library Intent
//! keeps the source locale it published, never the application's default,
//! failing library admission without that evidence (D15:1369-1375). The
//! source-locale vectors hand over `projectLocales`, with or without a
//! default, beside one Intent's owner and its explicit or published source
//! locale, and expect the resolved source locale and where it came from, or
//! the block.
//!
//! 020 expands one requested locale's fallback for one checked Intent: the
//! requested locale first, then the configured sequence in its order, each
//! literal as it is and the Intent source-locale candidate as that Intent's
//! source locale. A sequence is complete: a literal's own sequence is never
//! spliced in, and nothing implicit is added (D15:1503-1522). The fallback
//! vectors hand over `messageFallback` and `projectLocales` beside a
//! requested locale and an Intent's source locale, and expect the complete
//! candidate order, each candidate with its kind. Each Intent source locale
//! differs from the requested locale and from every literal, since 020's
//! Linker decides how such a candidate is probed (D15:1518).
//!
//! 016 and 020 own the form of the inputs and the relationships (D15:3330),
//! which these spell in 015's words until they define it.

use serde_json::{json, Value};

/// The decisions every source-locale vector rests on: the optional project
/// source default, inherited only by application Intents that omit their
/// source locale.
pub(super) const SOURCE_DECISIONS: [&str; 1] = ["015-026"];

/// The decisions every fallback vector rests on: the complete,
/// non-recursive fallback sequences with the Intent source-locale candidate.
pub(super) const FALLBACK_DECISIONS: [&str; 1] = ["015-030"];

/// One Intent as 016 receives it: its owner and the source locale its
/// authoring states, explicitly for an application Intent or as published
/// for a library Intent.
#[derive(Debug, Clone, Copy)]
pub(super) enum Intent {
    Application(Option<&'static str>),
    Library(Option<&'static str>),
}

/// What 016 concludes for an Intent, as the table states it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Resolved {
    Inherited(&'static str),
    Explicit(&'static str),
    Published(&'static str),
    /// The application Intent omits its source locale and the profile
    /// records no default: 016's blocking Finding at that Intent.
    ApplicationMissing,
    /// The library Intent has no published source locale: library
    /// admission fails.
    LibraryMissing,
}

/// One source-locale vector: its name, whether its producer has the
/// storefront's default, the Intent, and what 016 concludes.
pub(super) struct SourceVector {
    pub(super) name: &'static str,
    pub(super) with_default: bool,
    pub(super) intent: Intent,
    pub(super) resolved: Resolved,
}

const fn source(
    name: &'static str,
    with_default: bool,
    intent: Intent,
    resolved: Resolved,
) -> SourceVector {
    SourceVector {
        name,
        with_default,
        intent,
        resolved,
    }
}

/// Each way 016 resolves an Intent's source locale, under the storefront's
/// default `en` and under the fallback storefront, which has none.
pub(super) const SOURCE_VECTORS: [SourceVector; 7] = {
    use Intent::{Application, Library};
    use Resolved::{ApplicationMissing, Explicit, Inherited, LibraryMissing, Published};
    [
        source(
            "default-inherited",
            true,
            Application(None),
            Inherited("en"),
        ),
        source(
            "explicit-over-default",
            true,
            Application(Some("ja")),
            Explicit("ja"),
        ),
        source(
            "library-keeps-its-published-source",
            true,
            Library(Some("de")),
            Published("de"),
        ),
        source(
            "library-without-a-published-source",
            true,
            Library(None),
            LibraryMissing,
        ),
        source(
            "no-default-omitted",
            false,
            Application(None),
            ApplicationMissing,
        ),
        source(
            "no-default-explicit",
            false,
            Application(Some("en")),
            Explicit("en"),
        ),
        source(
            "no-default-library",
            false,
            Library(Some("de")),
            Published("de"),
        ),
    ]
};

/// One fallback vector: its name, the requested locale, the Intent's source
/// locale, and the complete candidate order the table states.
pub(super) struct FallbackVector {
    pub(super) name: &'static str,
    pub(super) requested: &'static str,
    pub(super) intent_source: &'static str,
    pub(super) order: &'static [&'static str],
}

const fn fallback(
    name: &'static str,
    requested: &'static str,
    intent_source: &'static str,
    order: &'static [&'static str],
) -> FallbackVector {
    FallbackVector {
        name,
        requested,
        intent_source,
        order,
    }
}

/// 015's example over Intents of three source locales, and two sequences
/// that stay independent (D15:1505-1518).
pub(super) const FALLBACK_VECTORS: [FallbackVector; 5] = [
    fallback("english-source", "ja-JP", "en", &["ja-JP", "ja", "en"]),
    fallback("french-source", "ja-JP", "fr", &["ja-JP", "ja", "fr"]),
    fallback(
        "german-library-source",
        "ja-JP",
        "de",
        &["ja-JP", "ja", "de"],
    ),
    fallback("no-recursion", "ja", "de", &["ja", "en"]),
    fallback("independent-sequence", "en", "de", &["en", "fr"]),
];

/// The consumer inputs and the relationship of `vector` against the
/// `projectLocales` its producer hands over.
///
/// # Panics
///
/// Panics when the resolution differs from what the table states, which the
/// table rules out.
pub(super) fn source_relationship(vector: &SourceVector, locales: &Value) -> (Value, Value) {
    let default = locales["defaultSourceLocale"].as_str();
    let (inputs, resolved) = match vector.intent {
        Intent::Application(explicit) => {
            let mut intent = json!({ "owner": "application" });
            if let Some(locale) = explicit {
                intent["sourceLocale"] = json!(locale);
            }
            let resolved = match (explicit, default) {
                (Some(locale), _) => ("resolved", Some(locale), "explicit"),
                (None, Some(locale)) => ("resolved", Some(locale), "inherited"),
                (None, None) => ("blocked", None, "application-source-locale-missing"),
            };
            (intent, resolved)
        }
        Intent::Library(published) => {
            let mut intent = json!({ "owner": "library" });
            if let Some(locale) = published {
                intent["publishedSourceLocale"] = json!(locale);
            }
            let resolved = match published {
                Some(locale) => ("resolved", Some(locale), "published"),
                None => ("blocked", None, "library-source-locale-missing"),
            };
            (intent, resolved)
        }
    };
    let relationship = match resolved {
        ("resolved", Some(locale), origin) => {
            json!({ "outcome": "resolved", "sourceLocale": locale, "origin": origin })
        }
        (_, _, reason) => json!({ "outcome": "blocked", "reason": reason }),
    };
    let stated = match vector.resolved {
        Resolved::Inherited(locale) => {
            json!({ "outcome": "resolved", "sourceLocale": locale, "origin": "inherited" })
        }
        Resolved::Explicit(locale) => {
            json!({ "outcome": "resolved", "sourceLocale": locale, "origin": "explicit" })
        }
        Resolved::Published(locale) => {
            json!({ "outcome": "resolved", "sourceLocale": locale, "origin": "published" })
        }
        Resolved::ApplicationMissing => {
            json!({ "outcome": "blocked", "reason": "application-source-locale-missing" })
        }
        Resolved::LibraryMissing => {
            json!({ "outcome": "blocked", "reason": "library-source-locale-missing" })
        }
    };
    assert_eq!(relationship, stated, "{}", vector.name);
    (json!({ "intent": inputs }), relationship)
}

/// The consumer inputs and the relationship of `vector` against the
/// `messageFallback` its producer hands over: the requested locale, then its
/// configured sequence, each literal as it is and the Intent source-locale
/// candidate as the Intent's source locale.
///
/// # Panics
///
/// Panics when the order differs from what the table states, which the
/// table rules out.
pub(super) fn fallback_relationship(vector: &FallbackVector, fallback: &Value) -> (Value, Value) {
    let mut candidates = vec![json!({ "locale": vector.requested, "kind": "requested" })];
    let configured = fallback["mappings"][vector.requested]
        .as_array()
        .cloned()
        .unwrap_or_default();
    for candidate in configured {
        candidates.push(match candidate.as_str() {
            Some(literal) => json!({ "locale": literal, "kind": "literal" }),
            None => json!({ "locale": vector.intent_source, "kind": "intent-source-locale" }),
        });
    }
    let order = candidates
        .iter()
        .map(|candidate| candidate["locale"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(order, vector.order, "{}", vector.name);
    let inputs = json!({
        "requestedLocale": vector.requested,
        "intentSourceLocale": vector.intent_source,
    });
    (inputs, json!({ "candidates": candidates }))
}

#[cfg(test)]
mod tests {
    use super::super::tests::{canonicalizer, written};
    use super::*;

    /// The written manifests whose IDs start with `family`.
    fn manifests(family: &str) -> Vec<Value> {
        let (_, vectors) = written();
        vectors
            .files
            .iter()
            .map(|file| serde_json::from_slice::<Value>(&file.bytes).unwrap())
            .filter(|manifest| manifest["id"].as_str().unwrap().starts_with(family))
            .collect()
    }

    /// 016's resolution of one Intent's source locale, read from 015's rules
    /// as a decision table over the Intent's owner, its own locale, and the
    /// project default (D15:1369-1375).
    fn other_source(locales: &Value, inputs: &Value) -> Value {
        let intent = &inputs["intent"];
        let own = intent
            .get("sourceLocale")
            .or_else(|| intent.get("publishedSourceLocale"));
        let library = intent["owner"] == "library";
        let default = &locales["defaultSourceLocale"];
        let (locale, origin) = match (library, own) {
            (true, Some(locale)) => (locale, "published"),
            (true, None) => {
                return json!({ "outcome": "blocked", "reason": "library-source-locale-missing" })
            }
            (false, Some(locale)) => (locale, "explicit"),
            (false, None) if default.is_string() => (default, "inherited"),
            (false, None) => {
                return json!({ "outcome": "blocked", "reason": "application-source-locale-missing" })
            }
        };
        json!({ "outcome": "resolved", "sourceLocale": locale, "origin": origin })
    }

    #[test]
    fn each_source_locale_is_what_016_s_rules_conclude() {
        let manifests = manifests("015-handoff-source-locale-");
        assert_eq!(manifests.len(), SOURCE_VECTORS.len());
        for manifest in &manifests {
            assert_eq!(
                manifest["expectedRelationship"]["json"],
                other_source(
                    &manifest["producerProfileFacts"]["projectLocales"],
                    &manifest["consumerInputs"]["json"]
                ),
                "{}",
                manifest["id"]
            );
            assert_eq!(manifest["verificationOwner"], "016");
        }
        // The default is an explicit absence without one, never a locale.
        let defaults = manifests
            .iter()
            .map(|manifest| {
                manifest["producerProfileFacts"]["projectLocales"]["defaultSourceLocale"]
                    .to_string()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            defaults,
            std::collections::BTreeSet::from(["\"en\"".to_owned(), "null".to_owned()])
        );
    }

    #[test]
    fn each_fallback_order_is_the_complete_configured_sequence() {
        let manifests = manifests("015-handoff-fallback-");
        assert_eq!(manifests.len(), FALLBACK_VECTORS.len());
        for manifest in &manifests {
            let inputs = &manifest["consumerInputs"]["json"];
            let requested = inputs["requestedLocale"].as_str().unwrap();
            let source = &inputs["intentSourceLocale"];
            let configured =
                &manifest["producerProfileFacts"]["messageFallback"]["mappings"][requested];
            let mut expected = vec![json!({ "locale": requested, "kind": "requested" })];
            for candidate in configured.as_array().unwrap() {
                expected.push(if candidate.is_string() {
                    json!({ "locale": candidate, "kind": "literal" })
                } else {
                    assert_eq!(candidate["kind"], "intent-source-locale");
                    json!({ "locale": source, "kind": "intent-source-locale" })
                });
            }
            let relationship = &manifest["expectedRelationship"]["json"];
            assert_eq!(
                relationship["candidates"],
                json!(expected),
                "{}",
                manifest["id"]
            );
            // No candidate repeats, so no probe representation is 020's
            // Linker's to choose (D15:1518).
            let locales = expected
                .iter()
                .map(|candidate| candidate["locale"].as_str().unwrap())
                .collect::<std::collections::BTreeSet<_>>();
            assert_eq!(locales.len(), expected.len(), "{}", manifest["id"]);
            assert_eq!(manifest["verificationOwner"], "020");
        }
    }

    /// Each locale a vector submits or expects is a canonical spelling.
    #[test]
    fn each_locale_is_canonical() {
        let canonicalizer = canonicalizer();
        let mut locales = Vec::new();
        for vector in &SOURCE_VECTORS {
            match vector.intent {
                Intent::Application(Some(locale)) | Intent::Library(Some(locale)) => {
                    locales.push(locale);
                }
                _ => {}
            }
        }
        for vector in &FALLBACK_VECTORS {
            locales.push(vector.requested);
            locales.push(vector.intent_source);
            locales.extend(vector.order);
        }
        for locale in locales {
            let canonical = canonicalizer.canonicalize(locale).unwrap();
            assert_eq!(canonical.into_locale().as_str(), locale);
        }
    }
}
