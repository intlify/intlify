// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Handoff Vectors this crate writes, and what a vector's producer
//! members are (D15:3307-3348).
//!
//! A Handoff Vector fixes what a checked profile hands a downstream owner,
//! the consumer input the owner receives beside it, and the relationship the
//! owner verifies; `intlify_config` never runs one (D15:3309). Its
//! `producerProfileFacts` are semantic groups of the expected profile view of
//! the Resolver Case its `producerCaseId` names, each with its complete
//! value, and its `producerArtifactBindings` are that outcome's binding rows
//! those values reference, in the outcome's order (D15:3330). The harness
//! reproduces both from each expected outcome of that case, which must be
//! checked (D15:3348).
//!
//! Revision `"0"` writes the twelve Group ID selection vectors for 020
//! (D15:2085): each of six selector forms over the storefront, whose one
//! group `web` holds `browser` and `server`, and over the storefront with a
//! second group, `jobs`, which holds `worker`. Each hands over the profile's
//! `deploymentGroups` and `targets`, so its bindings are the Target Profiles
//! `targets` references. Its `consumerInputs` hold the transaction's Group ID
//! selectors and, when it asks for part of a group, the Target IDs it names,
//! and its `expectedRelationship` the selected group with every member
//! planned, or a rejection by the rule 015 states: `selector-required`,
//! `unknown-group-id`, `multiple-selectors`, `target-id-selector`, or
//! `group-subset`. 020 owns the form of both (D15:3330), which these spell in
//! 015's words until 020 defines it.
//!
//! It also writes the twenty negotiation vectors for 027, which hand over a
//! target's supported subset and effective default with the negotiation
//! profile and expect what the portable lookup concludes ([`negotiation`]);
//! the seven source-locale vectors for 016 and the five fallback vectors for
//! 020 ([`sources`]).

use serde_json::{json, Map, Value};

use crate::construction::component::file_digest;
use crate::construction::generate;
use crate::resolver::dependency::SemanticGroup;

use super::case::{ManifestKind, REVISION};
use super::construction_cases::Cases;
use super::index::SuiteFile;
use super::resolver_cases::{expected_path, FALLBACK, NEGOTIATION, STOREFRONT, TWO_GROUPS};

mod negotiation;
mod sources;

/// Where the suite holds the vectors' manifests.
const CASES: &str = "cases/handoff";

/// The semantic groups a Group ID selection vector hands over: every group
/// with its members, and each member's locale facts and Target Profile.
const GROUPS: [SemanticGroup; 2] = [SemanticGroup::DeploymentGroups, SemanticGroup::Targets];

/// The semantic groups a negotiation vector hands over: the specification
/// preferences are canonicalized under, the negotiation profile, and each
/// target's supported subset and effective default (D15:1416-1422).
const NEGOTIATION_GROUPS: [SemanticGroup; 3] = [
    SemanticGroup::LocaleCanonicalization,
    SemanticGroup::LocaleNegotiation,
    SemanticGroup::Targets,
];

/// The semantic group a source-locale vector hands over: the project's
/// locales, its source default among them (D15:1369-1375).
const SOURCE_GROUPS: [SemanticGroup; 1] = [SemanticGroup::ProjectLocales];

/// The semantic groups a fallback vector hands over: the fallback policy and
/// the project's locales, whose requested locales key it (D15:1494).
const FALLBACK_GROUPS: [SemanticGroup; 2] = [
    SemanticGroup::MessageFallback,
    SemanticGroup::ProjectLocales,
];

/// The decisions every Group ID selection vector rests on: one complete
/// group per transaction, 015's selection vectors beside 020's selector
/// admission, and the canonical order of groups and members.
const DECISIONS: [&str; 3] = ["015-034", "015-044", "015-049"];

/// What 020 concludes from a vector's selectors.
#[derive(Debug, Clone, Copy)]
enum Relationship {
    /// This group, with every member planned.
    Selected(&'static str),
    /// No group, by the rule 015 states (D15:2085).
    Rejected(&'static str),
}

/// One vector: its name, the profile, what the transaction submits, and what
/// 020 concludes.
struct Vector {
    name: &'static str,
    two_groups: bool,
    group_ids: &'static [&'static str],
    target_ids: Option<&'static [&'static str]>,
    relationship: Relationship,
}

const fn vector(
    name: &'static str,
    two_groups: bool,
    group_ids: &'static [&'static str],
    target_ids: Option<&'static [&'static str]>,
    relationship: Relationship,
) -> Vector {
    Vector {
        name,
        two_groups,
        group_ids,
        target_ids,
        relationship,
    }
}

/// Each of the six selector forms over a profile with one group and over one
/// with two (D15:2085): omitted, one Group ID, an unknown Group ID, several
/// selectors, a Target ID, and a subset of a group.
const VECTORS: [Vector; 12] = {
    use Relationship::{Rejected, Selected};
    [
        vector("one-group-omitted", false, &[], None, Selected("web")),
        vector("one-group-group-id", false, &["web"], None, Selected("web")),
        vector(
            "one-group-unknown-group-id",
            false,
            &["edge"],
            None,
            Rejected("unknown-group-id"),
        ),
        vector(
            "one-group-repeated-group-id",
            false,
            &["web", "web"],
            None,
            Rejected("multiple-selectors"),
        ),
        vector(
            "one-group-target-id",
            false,
            &["browser"],
            None,
            Rejected("target-id-selector"),
        ),
        vector(
            "one-group-group-subset",
            false,
            &["web"],
            Some(&["browser"]),
            Rejected("group-subset"),
        ),
        vector(
            "two-groups-omitted",
            true,
            &[],
            None,
            Rejected("selector-required"),
        ),
        vector(
            "two-groups-group-id",
            true,
            &["jobs"],
            None,
            Selected("jobs"),
        ),
        vector(
            "two-groups-unknown-group-id",
            true,
            &["edge"],
            None,
            Rejected("unknown-group-id"),
        ),
        vector(
            "two-groups-two-group-ids",
            true,
            &["web", "jobs"],
            None,
            Rejected("multiple-selectors"),
        ),
        vector(
            "two-groups-target-id",
            true,
            &["worker"],
            None,
            Rejected("target-id-selector"),
        ),
        vector(
            "two-groups-group-subset",
            true,
            &["web"],
            Some(&["server"]),
            Rejected("group-subset"),
        ),
    ]
};

/// The members `groups` of the expected outcome `outcome`'s profile view, and
/// the outcome's binding rows whose key one of their values holds, in the
/// outcome's order: a vector's facts and bindings as its producer gives them
/// (D15:3330). `None` unless the outcome is checked and its view holds each
/// group.
pub(crate) fn produced<'a>(
    outcome: &Value,
    groups: impl IntoIterator<Item = &'a str>,
) -> Option<(Value, Value)> {
    if outcome["outcome"] != "checked" {
        return None;
    }
    let mut facts = Map::new();
    for group in groups {
        facts.insert(group.to_owned(), outcome["profile"].get(group)?.clone());
    }
    let facts = Value::Object(facts);
    let bindings = outcome["resolvedProfileArtifactBindings"]
        .as_array()?
        .iter()
        .filter(|row| holds(&facts, &row["key"]))
        .cloned()
        .collect();
    Some((facts, Value::Array(bindings)))
}

/// Whether `value` is `key` or holds it at any depth.
fn holds(value: &Value, key: &Value) -> bool {
    value == key
        || match value {
            Value::Array(items) => items.iter().any(|item| holds(item, key)),
            Value::Object(members) => members.values().any(|member| holds(member, key)),
            _ => false,
        }
}

/// The outcome the file entry of the Resolver Case `id` expects, from the
/// files `resolver` writes.
fn expected(resolver: &Cases, id: &str) -> Value {
    let path = expected_path(id, "file");
    let file = resolver
        .files
        .iter()
        .find(|file| file.path == path)
        .expect("the producer case has a file entry");
    serde_json::from_slice(&file.bytes).expect("an expected outcome is JSON")
}

/// One vector before it is written: its ID, its producer, the facts and
/// bindings the producer gives, the consumer inputs and the relationship,
/// its owner, and the decisions it rests on.
struct Written {
    id: String,
    producer: &'static str,
    produced: (Value, Value),
    inputs: Value,
    relationship: Value,
    owner: &'static str,
    decisions: &'static [&'static str],
}

/// The suite's Handoff Vectors, in ascending unsigned byte order of their
/// IDs, each produced from the expected outcome of the Resolver Case it
/// names among the cases `resolver` writes: the Group ID selection vectors
/// for 020, the negotiation vectors for 027, the source-locale vectors for
/// 016, and the fallback vectors for 020.
///
/// # Errors
///
/// Returns the serialization failure of a manifest, which their values rule
/// out.
pub(crate) fn cases(resolver: &Cases) -> Result<Cases, serde_json::Error> {
    let mut written = Vec::new();
    let groups = GROUPS.map(SemanticGroup::id);
    let one = expected(resolver, STOREFRONT);
    let two = expected(resolver, TWO_GROUPS);
    for vector in &VECTORS {
        let (producer, outcome) = if vector.two_groups {
            (TWO_GROUPS, &two)
        } else {
            (STOREFRONT, &one)
        };
        let (facts, bindings) =
            produced(outcome, groups).expect("a producer case is checked with every group");
        let relationship = match vector.relationship {
            Relationship::Selected(group) => json!({
                "outcome": "selected-group",
                "groupId": group,
                "plannedTargetIds": facts["deploymentGroups"][group]["members"],
            }),
            Relationship::Rejected(rule) => json!({ "outcome": "rejected", "rule": rule }),
        };
        written.push(Written {
            id: format!("015-handoff-group-selection-{}", vector.name),
            producer,
            produced: (facts, bindings),
            inputs: json!({
                "groupIdSelectors": vector.group_ids,
                "targetIdSelectors": vector.target_ids,
            }),
            relationship,
            owner: "020",
            decisions: &DECISIONS,
        });
    }
    let groups = NEGOTIATION_GROUPS.map(SemanticGroup::id);
    let (facts, bindings) = produced(&expected(resolver, NEGOTIATION), groups)
        .expect("the negotiation case is checked with every group");
    for vector in &negotiation::VECTORS {
        let (inputs, relationship) =
            negotiation::relationship(vector, &facts["targets"], &facts["localeNegotiation"]);
        written.push(Written {
            id: format!("015-handoff-negotiation-{}", vector.name),
            producer: NEGOTIATION,
            produced: (facts.clone(), bindings.clone()),
            inputs,
            relationship,
            owner: "027",
            decisions: &negotiation::DECISIONS,
        });
    }
    let storefront = expected(resolver, STOREFRONT);
    let fallback = expected(resolver, FALLBACK);
    let groups = SOURCE_GROUPS.map(SemanticGroup::id);
    for vector in &sources::SOURCE_VECTORS {
        let (producer, outcome) = if vector.with_default {
            (STOREFRONT, &storefront)
        } else {
            (FALLBACK, &fallback)
        };
        let (facts, bindings) =
            produced(outcome, groups).expect("a producer case is checked with every group");
        let (inputs, relationship) = sources::source_relationship(vector, &facts["projectLocales"]);
        written.push(Written {
            id: format!("015-handoff-source-locale-{}", vector.name),
            producer,
            produced: (facts, bindings),
            inputs,
            relationship,
            owner: "016",
            decisions: &sources::SOURCE_DECISIONS,
        });
    }
    let groups = FALLBACK_GROUPS.map(SemanticGroup::id);
    let (facts, bindings) =
        produced(&fallback, groups).expect("the fallback case is checked with every group");
    for vector in &sources::FALLBACK_VECTORS {
        let (inputs, relationship) =
            sources::fallback_relationship(vector, &facts["messageFallback"]);
        written.push(Written {
            id: format!("015-handoff-fallback-{}", vector.name),
            producer: FALLBACK,
            produced: (facts.clone(), bindings.clone()),
            inputs,
            relationship,
            owner: "020",
            decisions: &sources::FALLBACK_DECISIONS,
        });
    }
    let mut files = Vec::new();
    let mut listing = Vec::new();
    for vector in written {
        let (facts, bindings) = vector.produced;
        let manifest = generate::write(&json!({
            "suiteRevision": REVISION,
            "manifestKind": ManifestKind::HandoffVector.name(),
            "id": vector.id,
            "producerCaseId": vector.producer,
            "producerProfileFacts": facts,
            "producerArtifactBindings": bindings,
            "consumerInputs": { "json": vector.inputs },
            "expectedRelationship": { "json": vector.relationship },
            "verificationOwner": vector.owner,
            "traceability": { "decisionIds": vector.decisions, "ruleIds": [], "checkIds": [] },
        }))?;
        let path = format!("{CASES}/{}.json", vector.id);
        listing.push(json!({
            "id": vector.id,
            "path": path,
            "digest": file_digest(&manifest).as_str(),
        }));
        files.push(SuiteFile {
            path,
            bytes: manifest,
        });
    }
    listing.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Cases { files, listing })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::OnceLock;

    use intlify_locale_icu4x::{ConformanceLimits, ReadLimits};

    use super::*;
    use crate::conformance::resolver_cases::{self, data_files};
    use crate::input_limits::Bound;
    use crate::locale::icu4x::Icu4xProvider;
    use crate::locale::{Canonicalizer, VersionedIdentity};

    /// The canonicalizer the committed construction admits, over the
    /// committed ICU4X data.
    pub(super) fn canonicalizer() -> Canonicalizer<Icu4xProvider> {
        const LIMITS: ReadLimits = ReadLimits {
            manifest_bytes: 16_384,
            manifest_depth: 8,
            payloads: 2,
            payload_bytes: 1 << 20,
            marker_bytes: 1 << 20,
            conformance: ConformanceLimits {
                validity_groups: 64,
                codes: 1 << 15,
                extension_keys: 64,
                types: 2_048,
                corrections: 128,
                work: 1 << 16,
            },
        };
        let files = data_files();
        let acquire = |address: &str| {
            files
                .iter()
                .find(|(path, _)| path.ends_with(&format!("/{address}")))
                .map(|(_, bytes)| bytes.to_vec())
        };
        let provider = Icu4xProvider::admit(files[0].1, acquire, &LIMITS).unwrap();
        let supported = crate::construction::resolver::committed().locale_canonicalization();
        let expected = supported
            .expected(&VersionedIdentity {
                identity: "intlify-locale-canonicalization".into(),
                revision: "0".into(),
            })
            .unwrap();
        Canonicalizer::bind(expected, Some(provider), Bound::new(1024).unwrap()).unwrap()
    }

    /// The Resolver Cases and the vectors this crate writes, written once.
    pub(super) fn written() -> &'static (Cases, Cases) {
        static WRITTEN: OnceLock<(Cases, Cases)> = OnceLock::new();
        WRITTEN.get_or_init(|| {
            let resolver = resolver_cases::cases().unwrap();
            let vectors = cases(&resolver).unwrap();
            (resolver, vectors)
        })
    }

    /// The written manifests whose IDs start with `family`.
    fn manifests_of(vectors: &Cases, family: &str) -> Vec<Value> {
        vectors
            .files
            .iter()
            .map(|file| serde_json::from_slice::<Value>(&file.bytes).unwrap())
            .filter(|manifest| manifest["id"].as_str().unwrap().starts_with(family))
            .collect()
    }

    /// The Group ID selection vectors.
    fn manifests(vectors: &Cases) -> Vec<Value> {
        manifests_of(vectors, "015-handoff-group-selection-")
    }

    /// Group selection as 015 states it (D15:2085), from the groups a vector
    /// hands over and its selectors alone: one group permits omission,
    /// several require one exact Group ID, and an unknown Group ID, several
    /// selectors, a Target ID, or a subset of a group is rejected.
    fn selection(facts: &Value, inputs: &Value) -> Value {
        let groups = facts["deploymentGroups"].as_object().unwrap();
        let targets = facts["targets"].as_object().unwrap();
        let selectors: Vec<&str> = inputs["groupIdSelectors"]
            .as_array()
            .unwrap()
            .iter()
            .map(|selector| selector.as_str().unwrap())
            .collect();
        let rejected = |rule: &str| json!({ "outcome": "rejected", "rule": rule });
        let group = match selectors.as_slice() {
            [] if groups.len() == 1 => groups.keys().next().unwrap().as_str(),
            [] => return rejected("selector-required"),
            [selector] if groups.contains_key(*selector) => *selector,
            [selector] if targets.contains_key(*selector) => return rejected("target-id-selector"),
            [_] => return rejected("unknown-group-id"),
            _ => return rejected("multiple-selectors"),
        };
        if !inputs["targetIdSelectors"].is_null() {
            return rejected("group-subset");
        }
        json!({
            "outcome": "selected-group",
            "groupId": group,
            "plannedTargetIds": groups[group]["members"],
        })
    }

    #[test]
    fn each_relationship_is_what_015_s_selection_rules_conclude() {
        let (_, vectors) = written();
        let manifests = manifests(vectors);
        assert_eq!(manifests.len(), 12);
        for manifest in &manifests {
            assert_eq!(
                manifest["expectedRelationship"]["json"],
                selection(
                    &manifest["producerProfileFacts"],
                    &manifest["consumerInputs"]["json"]
                ),
                "{}",
                manifest["id"]
            );
        }
        // Every rejection 015 states, and a selected group, over each profile.
        for profile in ["one-group", "two-groups"] {
            let outcomes = manifests
                .iter()
                .filter(|manifest| manifest["id"].as_str().unwrap().contains(profile))
                .map(|manifest| {
                    let relationship = &manifest["expectedRelationship"]["json"];
                    relationship["rule"]
                        .as_str()
                        .unwrap_or("selected-group")
                        .to_owned()
                })
                .collect::<BTreeSet<_>>();
            let rules = [
                "group-subset",
                "multiple-selectors",
                "selected-group",
                "target-id-selector",
                "unknown-group-id",
            ];
            let mut expected = BTreeSet::from(rules.map(str::to_owned));
            if profile == "two-groups" {
                expected.insert("selector-required".to_owned());
            }
            assert_eq!(outcomes, expected, "{profile}");
        }
    }

    #[test]
    fn each_vector_hands_over_its_producer_s_groups_and_their_bindings() {
        let (resolver, vectors) = written();
        for manifest in manifests_of(vectors, "015-handoff-") {
            let producer = manifest["producerCaseId"].as_str().unwrap();
            for side in ["file", "programmatic"] {
                let path = expected_path(producer, side);
                let file = resolver.files.iter().find(|file| file.path == path);
                let outcome: Value = serde_json::from_slice(&file.unwrap().bytes).unwrap();
                let facts = manifest["producerProfileFacts"].as_object().unwrap();
                let produced = produced(&outcome, facts.keys().map(String::as_str));
                assert_eq!(
                    produced,
                    Some((
                        manifest["producerProfileFacts"].clone(),
                        manifest["producerArtifactBindings"].clone()
                    )),
                    "{}",
                    manifest["id"]
                );
            }
        }
    }

    #[test]
    fn the_two_profiles_hand_over_their_groups_in_canonical_order() {
        let (_, vectors) = written();
        let manifests = manifests(vectors);
        let of = |profile: &str| {
            manifests
                .iter()
                .find(|manifest| manifest["producerCaseId"] == profile)
                .unwrap()
                .clone()
        };
        let members = |manifest: &Value| {
            manifest["producerProfileFacts"]["deploymentGroups"]
                .as_object()
                .unwrap()
                .iter()
                .map(|(group, value)| (group.clone(), value["members"].clone()))
                .collect::<Vec<_>>()
        };
        let one = of(STOREFRONT);
        let two = of(TWO_GROUPS);
        assert_eq!(
            members(&one),
            [("web".into(), json!(["browser", "server"]))]
        );
        assert_eq!(
            members(&two),
            [
                ("jobs".into(), json!(["worker"])),
                ("web".into(), json!(["browser", "server"]))
            ]
        );
        let worker = &two["producerProfileFacts"]["targets"]["worker"];
        assert_eq!(worker["requestedLocales"], json!(["ja"]));
        assert_eq!(worker["effectiveDefaultRequestedLocale"], "ja");
        // Each Target Profile the targets reference, and nothing else.
        let identities = |manifest: &Value| {
            manifest["producerArtifactBindings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    let key = &row["key"];
                    assert_eq!(key["kind"], "target-profile");
                    key["identity"].as_str().unwrap().to_owned()
                })
                .collect::<BTreeSet<_>>()
        };
        assert_eq!(
            identities(&one),
            BTreeSet::from(["storefront-browser".into(), "storefront-server".into()])
        );
        assert_eq!(
            identities(&two),
            BTreeSet::from([
                "storefront-browser".into(),
                "storefront-server".into(),
                "storefront-worker".into()
            ])
        );
        // No selector changes what a profile hands over.
        for manifest in &manifests {
            let profile = of(manifest["producerCaseId"].as_str().unwrap());
            for member in ["producerProfileFacts", "producerArtifactBindings"] {
                assert_eq!(manifest[member], profile[member], "{}", manifest["id"]);
            }
        }
    }

    #[test]
    fn a_producer_hands_over_whole_groups_of_a_checked_outcome_only() {
        let (resolver, _) = written();
        let outcome = expected(resolver, STOREFRONT);
        let kinds = |groups: &[&str]| {
            let (facts, bindings) = produced(&outcome, groups.iter().copied()).unwrap();
            let names = facts
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(names, groups);
            for group in groups {
                assert_eq!(facts[*group], outcome["profile"][*group]);
            }
            bindings
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["key"]["kind"].as_str().unwrap().to_owned())
                .collect::<Vec<_>>()
        };
        assert_eq!(kinds(&["deploymentGroups"]), Vec::<String>::new());
        assert_eq!(kinds(&["targets"]), ["target-profile", "target-profile"]);
        // The required policies' five artifacts, in the outcome's order.
        let policies = kinds(&["requiredPolicies"]);
        let order = outcome["resolvedProfileArtifactBindings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["key"]["kind"].as_str().unwrap().to_owned())
            .filter(|kind| policies.contains(kind))
            .collect::<Vec<_>>();
        assert_eq!(policies.len(), 5);
        assert_eq!(policies, order);
        assert_eq!(produced(&outcome, ["groups"]), None);
        let mut blocked = outcome.clone();
        blocked["outcome"] = json!("blocked");
        assert_eq!(produced(&blocked, ["targets"]), None);
        let mut unbound = outcome;
        unbound["resolvedProfileArtifactBindings"] = Value::Null;
        assert_eq!(produced(&unbound, ["targets"]), None);
    }

    #[test]
    fn a_key_is_held_at_any_depth() {
        let key = json!({ "kind": "target-profile", "identity": "a" });
        assert!(holds(&key, &key));
        assert!(holds(&json!({ "a": { "b": key } }), &key));
        assert!(holds(&json!({ "a": [1, [key]] }), &key));
        assert!(!holds(&json!({ "a": [key["identity"]] }), &key));
        assert!(!holds(&json!([{ "kind": "target-profile" }]), &key));
    }

    #[test]
    fn every_vector_is_one_manifest_below_the_handoff_directory() {
        let (_, vectors) = written();
        let ids = vectors
            .listing
            .iter()
            .map(|entry| entry["id"].as_str().unwrap())
            .collect::<Vec<_>>();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(ids, sorted);
        assert_eq!(
            ids.len(),
            VECTORS.len()
                + negotiation::VECTORS.len()
                + sources::SOURCE_VECTORS.len()
                + sources::FALLBACK_VECTORS.len()
        );
        let paths = vectors
            .files
            .iter()
            .map(|file| file.path.as_str())
            .collect::<BTreeSet<_>>();
        assert_eq!(paths.len(), ids.len());
        for entry in &vectors.listing {
            let id = entry["id"].as_str().unwrap();
            assert!(
                [
                    "015-handoff-group-selection-",
                    "015-handoff-negotiation-",
                    "015-handoff-source-locale-",
                    "015-handoff-fallback-",
                ]
                .iter()
                .any(|family| id.starts_with(family)),
                "{id}"
            );
            assert_eq!(entry["path"], format!("{CASES}/{id}.json"));
            assert!(paths.contains(entry["path"].as_str().unwrap()));
        }
    }
}
