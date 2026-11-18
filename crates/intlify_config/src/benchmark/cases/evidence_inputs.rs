// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite inputs of one file entry's complete invocation for 015's Evidence
//! materialization stages: 015's example as the resolver's tests submit it,
//! profile `app` of the complete fixture configuration with every policy and
//! Target Profile it names submitted once and each reference naming one
//! exactly, and one change of it for each kind of outcome a stage reports:
//! blocking Findings after materialization, and many Findings. Every
//! reference's semantic digest is computed from the envelope it names, never
//! written by hand.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::artifacts::content::sample;
use crate::artifacts::envelope::{envelope_of, ArtifactKind};
use crate::input_limits::Bound;
use crate::profile_fixtures::complete_config;
use crate::resolver::limits::{Group, ResolutionLimits};

use super::policy_inputs::{capacity, reference_value, selected_policy, tuple_of};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum EvidenceRecipe {
    /// 015's example: no Finding.
    Storefront,
    /// The example without its project default and with a target locale that
    /// is no string: blocking Findings after materialization (D15:1907; plan
    /// 025 interpretation #252).
    Blocked,
    /// The example with [`DENSE`] more targets, each requesting a locale
    /// outside the project's that its Target Profile has no direction for:
    /// many Findings, each of a target of its own.
    Dense,
}

/// A configuration and the members submitted with it, in submission order.
pub(in crate::benchmark) struct EvidenceFixture {
    pub(in crate::benchmark) config: Value,
    pub(in crate::benchmark) members: Vec<Value>,
}

/// The number of targets `Dense` adds.
pub(in crate::benchmark) const DENSE: usize = 64;

/// A value no diagnostic count of these inputs reaches.
const ROOM: u64 = 1 << 40;

/// Each policy slot besides `resourceLimits`, the kind its role takes, and
/// the identity of the artifact it names.
const POLICIES: [(&str, ArtifactKind, &str); 6] = [
    ("trust", ArtifactKind::TrustPolicy, "storefront-trust"),
    (
        "sourceAdmission",
        ArtifactKind::SourceAdmissionPolicy,
        "storefront-source",
    ),
    (
        "approval",
        ArtifactKind::ApprovalPolicy,
        "storefront-approval",
    ),
    (
        "selection",
        ArtifactKind::SelectionPolicy,
        "storefront-selection",
    ),
    (
        "providerRouting",
        ArtifactKind::ProviderRoutingPolicy,
        "storefront-routing",
    ),
    (
        "glossarySet",
        ArtifactKind::GlossarySet,
        "storefront-glossary",
    ),
];

/// Each target, the identity of the Target Profile it names, and the
/// hydration role that profile declares, the relation's (024).
const TARGETS: [(&str, &str, &str); 2] = [
    ("browser", "storefront-browser", "browser-hydration-client"),
    ("server", "storefront-server", "ssr-renderer"),
];

impl EvidenceRecipe {
    pub(in crate::benchmark) const ALL: [Self; 3] = [Self::Storefront, Self::Blocked, Self::Dense];

    /// The configuration and members of this recipe, the selected policy
    /// setting `limits` and submitted first.
    pub(in crate::benchmark) fn fixture(self, limits: &ResolutionLimits) -> EvidenceFixture {
        let mut config = complete_config();
        let app = &mut config["profiles"]["app"];
        app["requestedLocales"] = json!(["ja", "en"]);
        app["defaultRequestedLocale"] = json!("en");
        app["localeNegotiation"] = json!({"aliases": {"de": "en"}});
        let policy = selected_policy(limits);
        app["policies"]["resourceLimits"] = reference_value(&tuple_of(&policy));
        let mut members = vec![policy];
        for (slot, kind, identity) in POLICIES {
            let member = envelope_of(kind, identity, "1", sample(kind));
            app["policies"][slot] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        for (target, identity, role) in TARGETS {
            let mut content = sample(ArtifactKind::TargetProfile);
            content["hydration"]["roles"] = json!([role]);
            let member = envelope_of(ArtifactKind::TargetProfile, identity, "1", content);
            app["targetProfiles"][target]["requestedLocales"] = json!(["ja", "en"]);
            app["targetProfiles"][target]["profile"] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        match self {
            Self::Storefront => {}
            Self::Blocked => {
                app.as_object_mut()
                    .expect("the example's declaration is an object")
                    .remove("defaultRequestedLocale");
                app["targetProfiles"]["server"]["requestedLocales"] = json!(["en", 42]);
            }
            Self::Dense => {
                let browser = app["targetProfiles"]["browser"].clone();
                let mut members = app["deploymentGroups"]["web"]["members"]
                    .as_array()
                    .cloned()
                    .expect("the example's group lists its members");
                for target in 0..DENSE {
                    let id = format!("target-{target:02}");
                    let mut declared = browser.clone();
                    declared["requestedLocales"] = json!(["ja", "en", "fr"]);
                    app["targetProfiles"][&id] = declared;
                    members.push(json!(id));
                }
                app["deploymentGroups"]["web"]["members"] = Value::Array(members);
            }
        }
        EvidenceFixture { config, members }
    }
}

/// The bounds both the capability and the selected policy set: the cases'
/// capacity, but every diagnostic bound at a value no count here reaches,
/// since the example's records outgrow the fixture values. The policy's
/// equal the capability's, so it is compatible with it.
pub(in crate::benchmark) fn limits() -> ResolutionLimits {
    let current = capacity().bounds();
    let room = Bound::new(ROOM).expect("a finite positive bound");
    ResolutionLimits::from_bounds(|id| {
        if matches!(id.group, Group::DiagnosticsEntry | Group::DiagnosticsShared) {
            Some(room)
        } else {
            current
                .iter()
                .find(|(bound, _)| *bound == id)
                .map(|&(_, value)| value)
        }
    })
    .expect("every bound of the capacity is set")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each recipe submits the selected policy first, the six other policies,
    /// and the two Target Profiles, each once, and the declaration names each
    /// of them exactly, and nothing else.
    #[test]
    fn each_recipe_names_every_member_it_submits_exactly() {
        let limits = limits();
        for recipe in EvidenceRecipe::ALL {
            let fixture = recipe.fixture(&limits);
            assert_eq!(fixture.members.len(), 9, "{recipe:?}");
            assert_eq!(fixture.members[0], selected_policy(&limits));
            let app = &fixture.config["profiles"]["app"];
            let mut named: Vec<&Value> = app["policies"].as_object().unwrap().values().collect();
            named.extend(
                app["targetProfiles"]
                    .as_object()
                    .unwrap()
                    .values()
                    .map(|target| &target["profile"]),
            );
            let submitted: Vec<Value> = fixture
                .members
                .iter()
                .map(|member| reference_value(&tuple_of(member)))
                .collect();
            assert!(named.iter().all(|reference| submitted.contains(reference)));
            assert!(submitted.iter().all(|member| named.contains(&member)));
        }
    }

    /// Only the recipe's own change differs from the example.
    #[test]
    fn each_recipe_changes_only_what_it_names() {
        let limits = limits();
        let example = EvidenceRecipe::Storefront.fixture(&limits).config;
        let blocked = EvidenceRecipe::Blocked.fixture(&limits).config;
        let dense = EvidenceRecipe::Dense.fixture(&limits).config;
        let mut restored = blocked.clone();
        restored["profiles"]["app"]["defaultRequestedLocale"] = json!("en");
        restored["profiles"]["app"]["targetProfiles"]["server"]["requestedLocales"] =
            json!(["ja", "en"]);
        assert_eq!(restored, example);
        let mut restored = dense.clone();
        let app = &mut restored["profiles"]["app"];
        let targets = app["targetProfiles"].as_object_mut().unwrap();
        let added: Vec<String> = targets
            .keys()
            .filter(|id| id.starts_with("target-"))
            .cloned()
            .collect();
        assert_eq!(added.len(), DENSE);
        for id in &added {
            assert_eq!(targets[id]["requestedLocales"], json!(["ja", "en", "fr"]));
            targets.remove(id);
        }
        app["deploymentGroups"]["web"]["members"] =
            example["profiles"]["app"]["deploymentGroups"]["web"]["members"].clone();
        assert_eq!(restored, example);
    }

    /// The input context refuses a candidate bound to another input: another
    /// recipe kind, another selector even with its own selector input,
    /// another selector input, another recipe's configuration, no vocabulary
    /// assertion, and a vocabulary that is not admitted.
    #[test]
    fn the_input_context_refuses_a_candidate_bound_to_another_input() {
        use std::sync::Arc;

        use crate::artifacts::vocabulary::{
            self, IntentSurfaceClassVocabularyArtifact, VocabularyLimits,
        };
        use crate::benchmark::cases::context::{observe, ContextFailure};
        use crate::benchmark::cases::prepare::{prepare, selector_input};
        use crate::benchmark::cases::{declarations, Recipe, Selector};
        use crate::benchmark::operation::Prepared;

        let room = VocabularyLimits {
            artifact_bytes: u64::MAX,
            depth: u64::MAX,
            members: u64::MAX,
            member_bytes: u64::MAX,
            total_member_bytes: u64::MAX,
        };
        let declaration = declarations()
            .into_iter()
            .find(|case| case.fixture == Recipe::Evidence(EvidenceRecipe::Storefront))
            .unwrap();
        for change in 0..6 {
            let mut candidate = prepare(&declaration).unwrap();
            assert!(observe(&candidate).is_ok());
            let Prepared::SharedEvidence(prepared) = &mut candidate.prepared else {
                unreachable!()
            };
            let bound = prepared.structural.max_profile_id_bytes;
            match change {
                0 => candidate.declaration.fixture = Recipe::Minimal,
                1 => {
                    candidate.declaration.selector = Selector::Absent;
                    prepared.selector = selector_input(Selector::Absent, bound);
                }
                2 => prepared.selector = selector_input(Selector::Unknown, bound),
                3 => {
                    let blocked = EvidenceRecipe::Blocked.fixture(&limits()).config;
                    prepared.source = Arc::from(serde_json::to_vec(&blocked).unwrap());
                }
                4 => prepared.specifications.intent_surface_class_vocabulary = None,
                // Members out of order: read, but not admitted.
                _ => {
                    let artifact = IntentSurfaceClassVocabularyArtifact::sealed(
                        "intlify-benchmark-vocabulary",
                        "0",
                        &["nav", "checkout"],
                    )
                    .unwrap();
                    let bytes = serde_json::to_vec(&artifact).unwrap();
                    prepared.vocabulary = vocabulary::read(Some(&bytes), &room).unwrap();
                }
            }
            assert_eq!(
                observe(&candidate),
                Err(ContextFailure::EvidenceInputMismatch),
                "change {change}"
            );
        }
    }

    /// Every diagnostic bound is at its room, and every other bound is the
    /// capacity's.
    #[test]
    fn only_the_diagnostic_bounds_differ_from_the_capacity() {
        let capacity = capacity().bounds();
        for (id, value) in limits().bounds() {
            if matches!(id.group, Group::DiagnosticsEntry | Group::DiagnosticsShared) {
                assert_eq!(value.get(), ROOM, "{id:?}");
            } else {
                assert!(capacity.contains(&(id, value)), "{id:?}");
            }
        }
    }
}
