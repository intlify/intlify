// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite Phase 11 and 12 inputs: 015's storefront example, its two scales,
//! and one change of the example for each decision Phases 11 and 12 reach or
//! wait on. The selected declaration names fixture-owned artifacts, and a
//! reference's semantic digest is computed from the envelope it names, never
//! written by hand. The finite fixture provider has no `ja-JP`, so the
//! example's third locale is its `ja`.

use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::artifacts::envelope::ArtifactKind;
use crate::profile_fixtures::minimal_config;

use super::binding_inputs::{artifact, REQUIRED};
use super::inputs::reverse_objects;
use super::policy_inputs::{capacity, reference_value, selected_policy, tuple_of};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum TargetAndGroupRecipe {
    Minimal,
    Storefront,
    StorefrontReordered,
    ManyTargets,
    ManyGroups,
    LocalesNotSubset,
    ExplicitDefaultNotRequested,
    InheritedDefaultNotRequested,
    TargetLocaleInvalid,
    TargetLocaleRepeated,
    TargetProfileMissing,
    CollectionNotAdmitted,
    MemberUnknown,
    MemberDuplicate,
    TargetUnassigned,
    TargetInTwoGroups,
}

/// A configuration and the members submitted with it, in submission order.
pub(in crate::benchmark) struct TargetAndGroupFixture {
    pub(in crate::benchmark) config: Value,
    pub(in crate::benchmark) members: Vec<Value>,
}

/// The number of targets `ManyTargets` declares in one group, and of groups
/// `ManyGroups` declares with one target each.
pub(super) const SCALE: usize = 64;

/// The example's requested locales, which both of its targets request too.
const STOREFRONT: [&str; 3] = ["en", "fr-FR", "ja"];

/// One declared target: its ID, the identity of the Target Profile it names,
/// its requested locales, and its default override.
struct Target {
    id: String,
    profile: &'static str,
    requested: Vec<&'static str>,
    default: Option<&'static str>,
}

impl Target {
    fn new(id: &str, profile: &'static str, requested: &[&'static str]) -> Self {
        Self {
            id: id.into(),
            profile,
            requested: requested.to_vec(),
            default: None,
        }
    }
}

/// A declaration's locales, targets, and groups, before they are written.
struct Declared {
    requested: Vec<&'static str>,
    targets: Vec<Target>,
    groups: Vec<(String, Vec<String>)>,
    relation: bool,
}

impl Declared {
    /// 015's example: `browser`, with its own default, and `ssr`, inheriting
    /// the project's, in the one group `web` with the hydration relation.
    fn storefront() -> Self {
        let mut browser = Target::new("browser", "storefront-browser", &STOREFRONT);
        browser.default = Some("en");
        Self {
            requested: STOREFRONT.to_vec(),
            targets: vec![browser, Target::new("ssr", "storefront-ssr", &STOREFRONT)],
            groups: vec![("web".into(), vec!["browser".into(), "ssr".into()])],
            relation: true,
        }
    }

    fn target(&mut self, id: &str) -> &mut Target {
        self.targets
            .iter_mut()
            .find(|target| target.id == id)
            .expect("a target of the example")
    }

    fn members(&mut self) -> &mut Vec<String> {
        &mut self.groups[0].1
    }
}

impl TargetAndGroupRecipe {
    pub(super) const ALL: [Self; 16] = [
        Self::Minimal,
        Self::Storefront,
        Self::StorefrontReordered,
        Self::ManyTargets,
        Self::ManyGroups,
        Self::LocalesNotSubset,
        Self::ExplicitDefaultNotRequested,
        Self::InheritedDefaultNotRequested,
        Self::TargetLocaleInvalid,
        Self::TargetLocaleRepeated,
        Self::TargetProfileMissing,
        Self::CollectionNotAdmitted,
        Self::MemberUnknown,
        Self::MemberDuplicate,
        Self::TargetUnassigned,
        Self::TargetInTwoGroups,
    ];

    /// Whether every target resolves and the groups partition the targets.
    pub(in crate::benchmark) const fn completes(self) -> bool {
        matches!(
            self,
            Self::Minimal
                | Self::Storefront
                | Self::StorefrontReordered
                | Self::ManyTargets
                | Self::ManyGroups
        )
    }

    fn declared(self) -> Declared {
        let scaled = |group: &dyn Fn(usize) -> String| {
            let ids = (0..SCALE)
                .map(|index| format!("target-{index:02}"))
                .collect::<Vec<_>>();
            let mut groups: Vec<(String, Vec<String>)> = Vec::new();
            for (index, id) in ids.iter().enumerate() {
                match groups.iter_mut().find(|(name, _)| *name == group(index)) {
                    Some((_, members)) => members.push(id.clone()),
                    None => groups.push((group(index), vec![id.clone()])),
                }
            }
            Declared {
                requested: vec!["en"],
                targets: ids
                    .iter()
                    .map(|id| Target::new(id, "storefront-browser", &["en"]))
                    .collect(),
                groups,
                relation: false,
            }
        };
        let mut declared = Declared::storefront();
        match self {
            Self::Minimal => {
                return Declared {
                    requested: vec!["en"],
                    targets: vec![Target::new("browser", "storefront-browser", &["en"])],
                    groups: vec![("web".into(), vec!["browser".into()])],
                    relation: false,
                }
            }
            Self::ManyTargets => return scaled(&|_| "web".into()),
            Self::ManyGroups => return scaled(&|index| format!("group-{index:02}")),
            // A locale the project does not request.
            Self::LocalesNotSubset => declared.target("ssr").requested.push("de"),
            // The override is no locale the target requests.
            Self::ExplicitDefaultNotRequested => {
                declared.target("browser").requested = vec!["fr-FR", "ja"];
            }
            // The project's default is no locale the target requests.
            Self::InheritedDefaultNotRequested => {
                declared.target("ssr").requested = vec!["fr-FR", "ja"];
            }
            // Phase 7 rejects the spelling, so the target's locales are
            // unavailable.
            Self::TargetLocaleInvalid => declared.target("ssr").requested[2] = "en_US",
            // Phase 7 canonicalizes `EN` to the target's `en`.
            Self::TargetLocaleRepeated => declared.target("ssr").requested.push("EN"),
            Self::MemberUnknown => declared.members().push("tablet".into()),
            Self::MemberDuplicate => declared.members().push("browser".into()),
            Self::TargetUnassigned => declared.members().retain(|id| id != "ssr"),
            Self::TargetInTwoGroups => declared
                .groups
                .push(("edge".into(), vec!["browser".into()])),
            Self::Storefront
            | Self::StorefrontReordered
            | Self::TargetProfileMissing
            | Self::CollectionNotAdmitted => {}
        }
        declared
    }

    /// The configuration and the members, in submission order. The collection
    /// submits the policy the confirmed bounds come from, the four other
    /// required policies, and each named Target Profile once.
    pub(in crate::benchmark) fn fixture(self) -> TargetAndGroupFixture {
        let resource_limits = selected_policy(&capacity());
        let mut config = minimal_config();
        let declaration = &mut config["profiles"]["app"];
        declaration["policies"]["resourceLimits"] = reference_value(&tuple_of(&resource_limits));
        let mut members = vec![resource_limits];
        for (slot, kind, identity) in REQUIRED {
            let member = artifact(kind, identity);
            declaration["policies"][slot] = reference_value(&tuple_of(&member));
            members.push(member);
        }
        let declared = self.declared();
        declaration["requestedLocales"] = json!(declared.requested);
        let mut targets = Map::new();
        let mut profiles = Vec::new();
        for target in &declared.targets {
            let profile = artifact(ArtifactKind::TargetProfile, target.profile);
            let mut value = json!({
                "profile": reference_value(&tuple_of(&profile)),
                "requestedLocales": target.requested,
            });
            if let Some(default) = target.default {
                value["defaultRequestedLocale"] = json!(default);
            }
            targets.insert(target.id.clone(), value);
            if !profiles.contains(&profile) {
                profiles.push(profile);
            }
        }
        declaration["targetProfiles"] = Value::Object(targets);
        let mut groups = Map::new();
        for (id, group) in &declared.groups {
            groups.insert(id.clone(), json!({ "members": group }));
        }
        if declared.relation {
            groups["web"]["hydrationRelations"] = json!([{"server": "ssr", "client": "browser"}]);
        }
        declaration["deploymentGroups"] = Value::Object(groups);
        if self == Self::TargetProfileMissing {
            let ssr = artifact(ArtifactKind::TargetProfile, "storefront-ssr");
            profiles.retain(|profile| *profile != ssr);
        }
        members.extend(profiles);
        // A second copy of the trust policy leaves the collection without a
        // Set.
        if self == Self::CollectionNotAdmitted {
            members.push(members[1].clone());
        }
        if self == Self::StorefrontReordered {
            reverse_objects(&mut config);
            for group in config["profiles"]["app"]["deploymentGroups"]
                .as_object_mut()
                .expect("the declaration's groups")
                .values_mut()
            {
                group["members"]
                    .as_array_mut()
                    .expect("a group's members")
                    .reverse();
            }
            members.reverse();
        }
        TargetAndGroupFixture { config, members }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use TargetAndGroupRecipe as R;

    fn declaration(fixture: &TargetAndGroupFixture) -> &Value {
        &fixture.config["profiles"]["app"]
    }

    fn targets(fixture: &TargetAndGroupFixture) -> &Map<String, Value> {
        declaration(fixture)["targetProfiles"].as_object().unwrap()
    }

    fn groups(fixture: &TargetAndGroupFixture) -> &Map<String, Value> {
        declaration(fixture)["deploymentGroups"]
            .as_object()
            .unwrap()
    }

    /// Every reference names a submitted member by the digest computed from
    /// it, except the one Target Profile `TargetProfileMissing` withholds.
    #[test]
    fn every_reference_names_a_member_by_a_computed_digest() {
        for recipe in R::ALL {
            let fixture = recipe.fixture();
            let submitted = fixture
                .members
                .iter()
                .map(|member| reference_value(&tuple_of(member)))
                .collect::<Vec<_>>();
            let policies = declaration(&fixture)["policies"].as_object().unwrap();
            for reference in policies.values().filter(|reference| !reference.is_null()) {
                assert!(submitted.contains(reference), "{recipe:?}: {reference}");
            }
            for (id, target) in targets(&fixture) {
                assert_eq!(
                    submitted.contains(&target["profile"]),
                    !(recipe == R::TargetProfileMissing && id == "ssr"),
                    "{recipe:?}: {id}"
                );
            }
        }
    }

    /// 015's example, with `ja` for its `ja-JP`: two targets requesting the
    /// project's three locales, one with the default as its override.
    #[test]
    fn the_storefront_is_fifteens_example() {
        let fixture = R::Storefront.fixture();
        let declaration = declaration(&fixture);
        assert_eq!(
            declaration["requestedLocales"],
            json!(["en", "fr-FR", "ja"])
        );
        assert_eq!(declaration["defaultRequestedLocale"], "en");
        let targets = targets(&fixture);
        assert_eq!(targets.keys().collect::<Vec<_>>(), ["browser", "ssr"]);
        assert_eq!(targets["browser"]["defaultRequestedLocale"], "en");
        assert!(targets["ssr"].get("defaultRequestedLocale").is_none());
        for target in targets.values() {
            assert_eq!(target["requestedLocales"], json!(["en", "fr-FR", "ja"]));
        }
        assert_eq!(
            Value::Object(groups(&fixture).clone()),
            json!({"web": {
                "members": ["browser", "ssr"],
                "hydrationRelations": [{"server": "ssr", "client": "browser"}]
            }})
        );
        // The five policies, and each target's own Target Profile.
        assert_eq!(fixture.members.len(), 7);
    }

    /// Each blocked recipe changes one part of the example: a target's
    /// locales, the web group's members, the groups, or the collection.
    #[test]
    fn each_blocked_recipe_changes_only_what_its_name_says() {
        let base = R::Storefront.fixture();
        let part = |fixture: &TargetAndGroupFixture, path: &[&str]| {
            path.iter()
                .fold(declaration(fixture), |value, key| &value[*key])
                .clone()
        };
        for (recipe, path, value) in [
            (
                R::LocalesNotSubset,
                &["targetProfiles", "ssr", "requestedLocales"][..],
                json!(["en", "fr-FR", "ja", "de"]),
            ),
            (
                R::ExplicitDefaultNotRequested,
                &["targetProfiles", "browser", "requestedLocales"],
                json!(["fr-FR", "ja"]),
            ),
            (
                R::InheritedDefaultNotRequested,
                &["targetProfiles", "ssr", "requestedLocales"],
                json!(["fr-FR", "ja"]),
            ),
            (
                R::TargetLocaleInvalid,
                &["targetProfiles", "ssr", "requestedLocales"],
                json!(["en", "fr-FR", "en_US"]),
            ),
            (
                R::TargetLocaleRepeated,
                &["targetProfiles", "ssr", "requestedLocales"],
                json!(["en", "fr-FR", "ja", "EN"]),
            ),
            (
                R::MemberUnknown,
                &["deploymentGroups", "web", "members"],
                json!(["browser", "ssr", "tablet"]),
            ),
            (
                R::MemberDuplicate,
                &["deploymentGroups", "web", "members"],
                json!(["browser", "ssr", "browser"]),
            ),
            (
                R::TargetUnassigned,
                &["deploymentGroups", "web", "members"],
                json!(["browser"]),
            ),
            (
                R::TargetInTwoGroups,
                &["deploymentGroups", "edge"],
                json!({"members": ["browser"]}),
            ),
        ] {
            let fixture = recipe.fixture();
            assert_eq!(fixture.members, base.members, "{recipe:?}");
            assert_eq!(part(&fixture, path), value, "{recipe:?}");
            assert_ne!(part(&base, path), value, "{recipe:?}");
            let mut config = fixture.config.clone();
            let mut slot = &mut config["profiles"]["app"];
            for key in &path[..path.len() - 1] {
                slot = &mut slot[*key];
            }
            let last = path[path.len() - 1];
            match base_part(&base, path) {
                Some(original) => slot[last] = original,
                None => {
                    slot.as_object_mut().unwrap().remove(last);
                }
            }
            assert_eq!(config, base.config, "{recipe:?}");
        }
        let ssr = artifact(ArtifactKind::TargetProfile, "storefront-ssr");
        let missing = R::TargetProfileMissing.fixture();
        assert_eq!(missing.config, base.config);
        assert_eq!(
            missing.members,
            base.members
                .iter()
                .filter(|member| **member != ssr)
                .cloned()
                .collect::<Vec<_>>()
        );
        let duplicated = R::CollectionNotAdmitted.fixture();
        assert_eq!(duplicated.config, base.config);
        assert_eq!(duplicated.members[..base.members.len()], base.members[..]);
        assert_eq!(
            duplicated.members[base.members.len()..],
            [artifact(ArtifactKind::TrustPolicy, "storefront-trust")]
        );
    }

    fn base_part(base: &TargetAndGroupFixture, path: &[&str]) -> Option<Value> {
        let mut value = declaration(base);
        for key in path {
            value = value.get(*key)?;
        }
        Some(value.clone())
    }

    #[test]
    fn the_scales_and_the_reordering_declare_what_their_names_say() {
        let minimal = R::Minimal.fixture();
        assert_eq!(targets(&minimal).len(), 1);
        assert_eq!(
            Value::Object(groups(&minimal).clone()),
            json!({"web": {"members": ["browser"]}})
        );
        let many = R::ManyTargets.fixture();
        assert_eq!(targets(&many).len(), SCALE);
        assert_eq!(groups(&many).len(), 1);
        assert_eq!(
            groups(&many)["web"]["members"].as_array().unwrap().len(),
            SCALE
        );
        let grouped = R::ManyGroups.fixture();
        assert_eq!(targets(&grouped).len(), SCALE);
        assert_eq!(groups(&grouped).len(), SCALE);
        for (index, (id, group)) in groups(&grouped).iter().enumerate() {
            assert_eq!(id, &format!("group-{index:02}"));
            assert_eq!(group["members"], json!([format!("target-{index:02}")]));
        }
        // Both scales share one Target Profile, so only the declaration grows.
        for fixture in [&minimal, &many, &grouped] {
            assert_eq!(fixture.members.len(), 6);
        }
        // The example with every object's members, the group's members, and
        // the collection reversed: equal as a set of facts, apart as bytes.
        let storefront = R::Storefront.fixture();
        let reordered = R::StorefrontReordered.fixture();
        assert_ne!(
            serde_json::to_vec(&reordered.config).unwrap(),
            serde_json::to_vec(&storefront.config).unwrap()
        );
        assert_eq!(
            groups(&reordered)["web"]["members"],
            json!(["ssr", "browser"])
        );
        let mut restored = reordered.config.clone();
        restored["profiles"]["app"]["deploymentGroups"]["web"]["members"] =
            json!(["browser", "ssr"]);
        assert_eq!(restored, storefront.config);
        assert_eq!(
            reordered.members.iter().rev().collect::<Vec<_>>(),
            storefront.members.iter().collect::<Vec<_>>()
        );
    }
}
