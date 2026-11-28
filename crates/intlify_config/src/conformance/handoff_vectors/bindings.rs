// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The binding-subset Handoff Vectors: the minimum projection of the
//! Resolved Profile Artifact Bindings each downstream consumer receives
//! beside the checked profile (D15:2714-2728).
//!
//! The sidecar holds exactly the admitted Policy and Target Profile bodies
//! the checked profile references, and a producer or the project graph
//! passes each consumer only the subset its owning specification requires
//! (D15:1761). 015's consumer table fixes that subset (D15:2718-2726):
//!
//! - the Source producer (016): the source-admission, trust, and
//!   resource-limit policies;
//! - the project graph and query service (019): the complete set, for
//!   dependency storage;
//! - the Requirement planner (020): the selected group's Target Profiles and
//!   the resource-limit policy;
//! - Synchronization (022): the Provider-routing policy and the Glossary Set
//!   when the profile references them, and the trust and resource-limit
//!   policies;
//! - Governance and Store (021): the approval, selection, trust,
//!   source-admission, and resource-limit policies;
//! - the Message Linker (020): the selected group's Target Profiles and the
//!   approval, selection, trust, source-admission, and resource-limit
//!   policies;
//! - the Target Exporter (024): the Target Profile of the one target an
//!   export operation handles (D24:144) and the resource-limit policy;
//! - Release Assembly (025): the selected group's Target Profiles and the
//!   approval, selection, trust, and resource-limit policies;
//! - execution integration (027): none.
//!
//! Each vector hands over `deploymentGroups`, `optionalPolicies`,
//! `requiredPolicies`, and `targets`, so its bindings are the complete set.
//! Its consumer inputs name the consumer and, when the subset depends on
//! them, the selected Group ID and the exported target, and its relationship
//! is the subset's keys in the sidecar's order. The consumer rejects a
//! missing, extra, or mismatched binding without rereading or reacquiring
//! anything (D15:2728); its owner defines and verifies that rejection.

use serde_json::{json, Value};

/// The decisions every binding-subset vector rests on: the consumer
/// boundaries, and the sidecar projected to each consumer's minimum subset.
pub(super) const DECISIONS: [&str; 2] = ["015-045", "015-058"];

/// A consumer 015's table names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Consumer {
    SourceProducer,
    ProjectGraph,
    RequirementPlanner,
    Synchronization,
    GovernanceAndStore,
    MessageLinker,
    TargetExporter,
    ReleaseAssembly,
    ExecutionIntegration,
}

/// The Target Profiles a consumer receives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Profiles {
    /// None.
    None,
    /// Every target's.
    All,
    /// The selected group's members'.
    Group,
    /// The one exported target's.
    Target,
}

/// Every Policy kind a profile may reference.
const POLICY_KINDS: [&str; 7] = [
    "resource-limit-policy",
    "trust-policy",
    "source-admission-policy",
    "approval-policy",
    "selection-policy",
    "provider-routing-policy",
    "glossary-set",
];

impl Consumer {
    /// The consumer's name in a vector's inputs.
    pub(super) const fn id(self) -> &'static str {
        match self {
            Self::SourceProducer => "source-producer",
            Self::ProjectGraph => "project-graph",
            Self::RequirementPlanner => "requirement-planner",
            Self::Synchronization => "synchronization",
            Self::GovernanceAndStore => "governance-and-store",
            Self::MessageLinker => "message-linker",
            Self::TargetExporter => "target-exporter",
            Self::ReleaseAssembly => "release-assembly",
            Self::ExecutionIntegration => "execution-integration",
        }
    }

    /// The design that owns the consumer and verifies its vectors.
    pub(super) const fn owner(self) -> &'static str {
        match self {
            Self::SourceProducer => "016",
            Self::ProjectGraph => "019",
            Self::RequirementPlanner | Self::MessageLinker => "020",
            Self::GovernanceAndStore => "021",
            Self::Synchronization => "022",
            Self::TargetExporter => "024",
            Self::ReleaseAssembly => "025",
            Self::ExecutionIntegration => "027",
        }
    }

    /// The Policy kinds it receives where the profile references them.
    const fn policies(self) -> &'static [&'static str] {
        match self {
            Self::SourceProducer => &[
                "source-admission-policy",
                "trust-policy",
                "resource-limit-policy",
            ],
            Self::ProjectGraph => &POLICY_KINDS,
            Self::RequirementPlanner | Self::TargetExporter => &["resource-limit-policy"],
            Self::Synchronization => &[
                "provider-routing-policy",
                "glossary-set",
                "trust-policy",
                "resource-limit-policy",
            ],
            Self::GovernanceAndStore | Self::MessageLinker => &[
                "approval-policy",
                "selection-policy",
                "trust-policy",
                "source-admission-policy",
                "resource-limit-policy",
            ],
            Self::ReleaseAssembly => &[
                "approval-policy",
                "selection-policy",
                "trust-policy",
                "resource-limit-policy",
            ],
            Self::ExecutionIntegration => &[],
        }
    }

    /// The Target Profiles it receives.
    const fn profiles(self) -> Profiles {
        match self {
            Self::SourceProducer
            | Self::Synchronization
            | Self::GovernanceAndStore
            | Self::ExecutionIntegration => Profiles::None,
            Self::ProjectGraph => Profiles::All,
            Self::RequirementPlanner | Self::MessageLinker | Self::ReleaseAssembly => {
                Profiles::Group
            }
            Self::TargetExporter => Profiles::Target,
        }
    }
}

/// One vector: its name, its consumer, whether the storefront with two
/// groups produces it, the selected Group ID and the exported target when
/// the consumer receives them, and the identities of the bindings it
/// receives, as the table states them.
pub(super) struct Vector {
    pub(super) name: &'static str,
    pub(super) consumer: Consumer,
    pub(super) two_groups: bool,
    pub(super) group: Option<&'static str>,
    pub(super) target: Option<&'static str>,
    pub(super) identities: &'static [&'static str],
}

/// Each consumer of the table: those whose subset no transaction input
/// selects over the storefront, which references every Policy kind and
/// whose one group `web` holds `browser` and `server`; those whose subset a
/// selected group selects over the storefront with a second group, `jobs`,
/// which holds `worker`, once for each group.
pub(super) const VECTORS: [Vector; 13] = {
    use Consumer::{
        ExecutionIntegration, GovernanceAndStore, MessageLinker, ProjectGraph, ReleaseAssembly,
        RequirementPlanner, SourceProducer, Synchronization, TargetExporter,
    };
    const fn on_storefront(
        name: &'static str,
        consumer: Consumer,
        identities: &'static [&'static str],
    ) -> Vector {
        Vector {
            name,
            consumer,
            two_groups: false,
            group: None,
            target: None,
            identities,
        }
    }
    const fn on_two_groups(
        name: &'static str,
        consumer: Consumer,
        group: &'static str,
        target: Option<&'static str>,
        identities: &'static [&'static str],
    ) -> Vector {
        Vector {
            name,
            consumer,
            two_groups: true,
            group: Some(group),
            target,
            identities,
        }
    }
    [
        on_storefront(
            "source-producer",
            SourceProducer,
            &["storefront-limits", "storefront-source", "storefront-trust"],
        ),
        on_storefront(
            "project-graph",
            ProjectGraph,
            &[
                "storefront-approval",
                "storefront-browser",
                "storefront-glossary",
                "storefront-limits",
                "storefront-routing",
                "storefront-selection",
                "storefront-server",
                "storefront-source",
                "storefront-trust",
            ],
        ),
        on_storefront(
            "synchronization",
            Synchronization,
            &[
                "storefront-glossary",
                "storefront-limits",
                "storefront-routing",
                "storefront-trust",
            ],
        ),
        on_storefront(
            "governance-and-store",
            GovernanceAndStore,
            &[
                "storefront-approval",
                "storefront-limits",
                "storefront-selection",
                "storefront-source",
                "storefront-trust",
            ],
        ),
        on_storefront("execution-integration", ExecutionIntegration, &[]),
        on_two_groups(
            "requirement-planner-web",
            RequirementPlanner,
            "web",
            None,
            &[
                "storefront-browser",
                "storefront-limits",
                "storefront-server",
            ],
        ),
        on_two_groups(
            "requirement-planner-jobs",
            RequirementPlanner,
            "jobs",
            None,
            &["storefront-limits", "storefront-worker"],
        ),
        on_two_groups(
            "message-linker-web",
            MessageLinker,
            "web",
            None,
            &[
                "storefront-approval",
                "storefront-browser",
                "storefront-limits",
                "storefront-selection",
                "storefront-server",
                "storefront-source",
                "storefront-trust",
            ],
        ),
        on_two_groups(
            "message-linker-jobs",
            MessageLinker,
            "jobs",
            None,
            &[
                "storefront-approval",
                "storefront-limits",
                "storefront-selection",
                "storefront-source",
                "storefront-trust",
                "storefront-worker",
            ],
        ),
        on_two_groups(
            "target-exporter-server",
            TargetExporter,
            "web",
            Some("server"),
            &["storefront-limits", "storefront-server"],
        ),
        on_two_groups(
            "target-exporter-worker",
            TargetExporter,
            "jobs",
            Some("worker"),
            &["storefront-limits", "storefront-worker"],
        ),
        on_two_groups(
            "release-assembly-web",
            ReleaseAssembly,
            "web",
            None,
            &[
                "storefront-approval",
                "storefront-browser",
                "storefront-limits",
                "storefront-selection",
                "storefront-server",
                "storefront-trust",
            ],
        ),
        on_two_groups(
            "release-assembly-jobs",
            ReleaseAssembly,
            "jobs",
            None,
            &[
                "storefront-approval",
                "storefront-limits",
                "storefront-selection",
                "storefront-trust",
                "storefront-worker",
            ],
        ),
    ]
};

/// The consumer inputs and the relationship of `vector` against the facts
/// and the complete bindings its producer hands over, checked against the
/// identities the table states.
///
/// # Panics
///
/// Panics when the subset differs from the table's, which the table rules
/// out.
pub(super) fn relationship(vector: &Vector, facts: &Value, bindings: &Value) -> (Value, Value) {
    let mut inputs = json!({ "consumer": vector.consumer.id() });
    if let Some(group) = vector.group {
        inputs["selectedGroupId"] = json!(group);
    }
    if let Some(target) = vector.target {
        inputs["targetId"] = json!(target);
    }
    let targets = &facts["targets"];
    let profiles: Vec<&Value> = match vector.consumer.profiles() {
        Profiles::None => Vec::new(),
        Profiles::All => targets
            .as_object()
            .expect("a checked profile maps its targets")
            .values()
            .map(|target| &target["profile"])
            .collect(),
        Profiles::Group => {
            let group = vector
                .group
                .expect("the consumer receives a selected group");
            facts["deploymentGroups"][group]["members"]
                .as_array()
                .expect("a selected group lists its members")
                .iter()
                .map(|member| {
                    &targets[member.as_str().expect("a member is a Target ID")]["profile"]
                })
                .collect()
        }
        Profiles::Target => {
            let target = vector.target.expect("the consumer receives its target");
            vec![&targets[target]["profile"]]
        }
    };
    let keys: Vec<Value> = bindings
        .as_array()
        .expect("the bindings are rows")
        .iter()
        .map(|row| &row["key"])
        .filter(|key| {
            if key["kind"] == "target-profile" {
                profiles.contains(key)
            } else {
                vector
                    .consumer
                    .policies()
                    .iter()
                    .any(|kind| key["kind"] == *kind)
            }
        })
        .cloned()
        .collect();
    let mut identities: Vec<&str> = keys
        .iter()
        .map(|key| key["identity"].as_str().expect("a key has an identity"))
        .collect();
    identities.sort_unstable();
    assert_eq!(identities, vector.identities, "{}", vector.name);
    (inputs, json!({ "bindingKeys": keys }))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::tests::written;
    use super::*;

    /// The written binding-subset vectors.
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
                    .starts_with("015-handoff-binding-subset-")
            })
            .collect()
    }

    /// 015's consumer table read again from the vector alone, by the
    /// profile members that reference each body: the references the
    /// consumer receives, then the sidecar's rows holding them, in its
    /// order.
    fn other_subset(facts: &Value, bindings: &Value, inputs: &Value) -> Value {
        let required = |names: &[&str]| -> Vec<Value> {
            names
                .iter()
                .map(|name| facts["requiredPolicies"][*name].clone())
                .collect()
        };
        let present = |names: &[&str]| -> Vec<Value> {
            names
                .iter()
                .map(|name| facts["optionalPolicies"][*name].clone())
                .filter(|reference| !reference.is_null())
                .collect()
        };
        let profile =
            |target: &Value| facts["targets"][target.as_str().unwrap()]["profile"].clone();
        let group = || -> Vec<Value> {
            facts["deploymentGroups"][inputs["selectedGroupId"].as_str().unwrap()]["members"]
                .as_array()
                .unwrap()
                .iter()
                .map(profile)
                .collect()
        };
        let all = bindings
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["key"].clone())
            .collect::<Vec<_>>();
        let received: Vec<Value> = match inputs["consumer"].as_str().unwrap() {
            "source-producer" => required(&["sourceAdmission", "trust", "resourceLimits"]),
            "project-graph" => all.clone(),
            "requirement-planner" => [group(), required(&["resourceLimits"])].concat(),
            "synchronization" => [
                present(&["providerRouting", "glossarySet"]),
                required(&["trust", "resourceLimits"]),
            ]
            .concat(),
            "governance-and-store" => required(&[
                "approval",
                "selection",
                "trust",
                "sourceAdmission",
                "resourceLimits",
            ]),
            "message-linker" => [
                group(),
                required(&[
                    "approval",
                    "selection",
                    "trust",
                    "sourceAdmission",
                    "resourceLimits",
                ]),
            ]
            .concat(),
            "target-exporter" => {
                vec![
                    profile(&inputs["targetId"]),
                    facts["requiredPolicies"]["resourceLimits"].clone(),
                ]
            }
            "release-assembly" => [
                group(),
                required(&["approval", "selection", "trust", "resourceLimits"]),
            ]
            .concat(),
            "execution-integration" => Vec::new(),
            other => panic!("{other} is no consumer of 015's table"),
        };
        let keys = all
            .into_iter()
            .filter(|key| received.contains(key))
            .collect::<Vec<_>>();
        // The sidecar holds every reference the consumer receives.
        assert_eq!(keys.len(), received.len());
        json!({ "bindingKeys": keys })
    }

    #[test]
    fn each_subset_is_what_015_s_consumer_table_gives() {
        let manifests = manifests();
        assert_eq!(manifests.len(), VECTORS.len());
        for manifest in &manifests {
            assert_eq!(
                manifest["expectedRelationship"]["json"],
                other_subset(
                    &manifest["producerProfileFacts"],
                    &manifest["producerArtifactBindings"],
                    &manifest["consumerInputs"]["json"]
                ),
                "{}",
                manifest["id"]
            );
        }
    }

    #[test]
    fn every_consumer_of_the_table_has_its_vectors_and_owner() {
        let manifests = manifests();
        let consumers = manifests
            .iter()
            .map(|manifest| {
                let inputs = &manifest["consumerInputs"]["json"];
                let consumer = VECTORS
                    .iter()
                    .map(|vector| vector.consumer)
                    .find(|consumer| inputs["consumer"] == consumer.id())
                    .unwrap();
                assert_eq!(manifest["verificationOwner"], consumer.owner());
                // Only a consumer whose subset a group or a target selects
                // receives them, and an exported target is a member of its
                // group.
                let facts = &manifest["producerProfileFacts"];
                let selects = consumer.profiles() == Profiles::Group
                    || consumer.profiles() == Profiles::Target;
                assert_eq!(inputs.get("selectedGroupId").is_some(), selects);
                assert_eq!(
                    inputs.get("targetId").is_some(),
                    consumer.profiles() == Profiles::Target
                );
                if let Some(target) = inputs.get("targetId") {
                    let members = &facts["deploymentGroups"]
                        [inputs["selectedGroupId"].as_str().unwrap()]["members"];
                    assert!(members.as_array().unwrap().contains(target));
                }
                assert_eq!(
                    facts.as_object().unwrap().keys().collect::<Vec<_>>(),
                    [
                        "deploymentGroups",
                        "optionalPolicies",
                        "requiredPolicies",
                        "targets"
                    ]
                );
                consumer.id()
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(consumers.len(), 9);
        // Every group of the two-group profile is selected once for each
        // consumer a group selects.
        for consumer in [
            "requirement-planner",
            "message-linker",
            "target-exporter",
            "release-assembly",
        ] {
            let groups = manifests
                .iter()
                .filter(|manifest| manifest["consumerInputs"]["json"]["consumer"] == consumer)
                .map(|manifest| {
                    manifest["consumerInputs"]["json"]["selectedGroupId"]
                        .as_str()
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(groups.len(), 2, "{consumer}");
            assert_eq!(
                groups.into_iter().collect::<BTreeSet<_>>(),
                ["jobs", "web"].into(),
                "{consumer}"
            );
        }
    }

    #[test]
    fn every_subset_keeps_the_sidecar_s_order_and_the_resource_limits() {
        for manifest in manifests() {
            let all = manifest["producerArtifactBindings"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["key"].clone())
                .collect::<Vec<_>>();
            let keys = manifest["expectedRelationship"]["json"]["bindingKeys"]
                .as_array()
                .unwrap()
                .clone();
            // The keys are a subsequence of the complete set, so each is
            // a referenced body and the order is the sidecar's.
            let mut rest = all.iter();
            assert!(
                keys.iter().all(|key| rest.any(|row| row == key)),
                "{}",
                manifest["id"]
            );
            // Every consumer but execution integration receives the
            // resource-limit policy; execution integration receives none.
            let limits = &manifest["producerProfileFacts"]["requiredPolicies"]["resourceLimits"];
            let execution =
                manifest["consumerInputs"]["json"]["consumer"] == "execution-integration";
            assert_eq!(keys.contains(limits), !execution, "{}", manifest["id"]);
            assert_eq!(keys.is_empty(), execution, "{}", manifest["id"]);
        }
    }

    #[test]
    fn the_policy_kinds_are_every_kind_a_binding_of_the_storefront_has() {
        let (_, vectors) = written();
        let graph = vectors
            .files
            .iter()
            .map(|file| serde_json::from_slice::<Value>(&file.bytes).unwrap())
            .find(|manifest| manifest["id"] == "015-handoff-binding-subset-project-graph")
            .unwrap();
        let kinds = graph["producerArtifactBindings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["key"]["kind"].as_str().unwrap())
            .filter(|kind| *kind != "target-profile")
            .collect::<BTreeSet<_>>();
        assert_eq!(kinds, POLICY_KINDS.into_iter().collect());
    }
}
