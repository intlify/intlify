// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite Phase 13 inputs: 015's storefront example and its relation, a group
//! without relations, fan-out and fan-in, a scale of relations among few
//! targets, and one change of the example for each decision Phase 13 reaches
//! or waits on. The fixture also answers what 015 asks 024 about each Target
//! Profile, which 024 has not defined yet (plan 023 ◆#1).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::super::hydration_relation::FixtureCapabilities;
use super::target_group_inputs::{Declared, Target, TargetAndGroupFixture, STOREFRONT};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum HydrationRecipe {
    NoRelations,
    Storefront,
    FanOutFanIn,
    FanReordered,
    ManyRelations,
    EqualEndpoint,
    UnknownEndpoint,
    CrossGroup,
    DuplicateRelation,
    ServerCapabilityMissing,
    ClientCapabilityMissing,
    DualRole,
    RequestedLocales,
    EffectiveDefault,
    StaticIncompatibility,
    EndpointLocalesUnresolved,
    GroupMembersUnresolved,
    TargetProfileMissing,
    NegotiationUnavailable,
}

/// The servers, and as many clients, `ManyRelations` relates each to each.
pub(super) const SIDE: usize = 8;

fn pair(server: &str, client: &str) -> (String, String) {
    (server.into(), client.into())
}

impl HydrationRecipe {
    pub(super) const ALL: [Self; 19] = [
        Self::NoRelations,
        Self::Storefront,
        Self::FanOutFanIn,
        Self::FanReordered,
        Self::ManyRelations,
        Self::EqualEndpoint,
        Self::UnknownEndpoint,
        Self::CrossGroup,
        Self::DuplicateRelation,
        Self::ServerCapabilityMissing,
        Self::ClientCapabilityMissing,
        Self::DualRole,
        Self::RequestedLocales,
        Self::EffectiveDefault,
        Self::StaticIncompatibility,
        Self::EndpointLocalesUnresolved,
        Self::GroupMembersUnresolved,
        Self::TargetProfileMissing,
        Self::NegotiationUnavailable,
    ];

    /// Whether every group's relations hold.
    pub(in crate::benchmark) const fn completes(self) -> bool {
        matches!(
            self,
            Self::NoRelations
                | Self::Storefront
                | Self::FanOutFanIn
                | Self::FanReordered
                | Self::ManyRelations
        )
    }

    fn declared(self) -> Declared {
        let mut declared = Declared::storefront();
        let add = |declared: &mut Declared, id: &str, profile: &'static str| {
            declared.targets.push(Target::new(id, profile, &STOREFRONT));
            declared.members().push(id.into());
        };
        match self {
            Self::NoRelations => declared.relations.clear(),
            Self::FanOutFanIn | Self::FanReordered => {
                add(&mut declared, "edge", "storefront-edge");
                add(&mut declared, "app", "storefront-app");
                declared.relations = vec![
                    pair("ssr", "browser"),
                    pair("ssr", "app"),
                    pair("edge", "browser"),
                ];
            }
            Self::ManyRelations => {
                declared.targets.clear();
                declared.members().clear();
                declared.relations.clear();
                for index in 0..SIDE {
                    add(&mut declared, &format!("server-{index}"), "storefront-ssr");
                    add(
                        &mut declared,
                        &format!("client-{index}"),
                        "storefront-browser",
                    );
                }
                for server in 0..SIDE {
                    for client in 0..SIDE {
                        declared.relations.push(pair(
                            &format!("server-{server}"),
                            &format!("client-{client}"),
                        ));
                    }
                }
            }
            Self::EqualEndpoint => declared.relations.push(pair("ssr", "ssr")),
            Self::UnknownEndpoint => declared.relations.push(pair("ssr", "phone")),
            Self::CrossGroup => {
                declared
                    .targets
                    .push(Target::new("worker", "storefront-worker", &STOREFRONT));
                declared.groups.push(("jobs".into(), vec!["worker".into()]));
                declared.relations.push(pair("ssr", "worker"));
            }
            Self::DuplicateRelation => declared.relations.push(pair("ssr", "browser")),
            Self::DualRole => {
                add(&mut declared, "app", "storefront-app");
                declared.relations.push(pair("browser", "app"));
            }
            // A subset of the project's locales, but not the client's.
            Self::RequestedLocales => declared.target("ssr").requested = vec!["en", "fr-FR"],
            // One of the client's locales, but not the server's default.
            Self::EffectiveDefault => declared.target("browser").default = Some("fr-FR"),
            // Phase 11 finds `de` outside the project, so the server's
            // locales are unavailable.
            Self::EndpointLocalesUnresolved => declared.target("ssr").requested.push("de"),
            // Phase 12 finds `tablet` names no target, so the group's members
            // are unavailable.
            Self::GroupMembersUnresolved => declared.members().push("tablet".into()),
            // Phase 7 rejects the alias's destination, so Phase 8 admits no
            // Locale Negotiation Profile for the relations to share.
            Self::NegotiationUnavailable => declared.aliases = vec![("fr", "en_US")],
            Self::Storefront
            | Self::ServerCapabilityMissing
            | Self::ClientCapabilityMissing
            | Self::StaticIncompatibility
            | Self::TargetProfileMissing => {}
        }
        declared
    }

    /// The fixture's answers to what 015 asks 024, by Target Profile
    /// identity.
    pub(in crate::benchmark) fn capabilities(self) -> FixtureCapabilities {
        let names = |names: &[&str]| names.iter().map(|&name| name.to_owned()).collect();
        let mut capabilities = FixtureCapabilities {
            renders: names(&["storefront-ssr"]),
            hydrates: names(&["storefront-browser"]),
            incompatible: BTreeSet::new(),
        };
        match self {
            Self::FanOutFanIn | Self::FanReordered => {
                capabilities.renders = names(&["storefront-ssr", "storefront-edge"]);
                capabilities.hydrates = names(&["storefront-browser", "storefront-app"]);
            }
            Self::DualRole => {
                capabilities.renders = names(&["storefront-ssr", "storefront-browser"]);
                capabilities.hydrates = names(&["storefront-browser", "storefront-app"]);
            }
            Self::ServerCapabilityMissing => capabilities.renders.clear(),
            Self::ClientCapabilityMissing => capabilities.hydrates.clear(),
            Self::StaticIncompatibility => {
                capabilities
                    .incompatible
                    .insert(pair("storefront-ssr", "storefront-browser"));
            }
            _ => {}
        }
        capabilities
    }

    /// The configuration and the members, in submission order.
    pub(in crate::benchmark) fn fixture(self) -> TargetAndGroupFixture {
        let withheld: &[&str] = if self == Self::TargetProfileMissing {
            &["storefront-ssr"]
        } else {
            &[]
        };
        self.declared()
            .assemble(withheld, false, self == Self::FanReordered)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map, Value};

    use super::*;

    use HydrationRecipe as R;

    fn relations(fixture: &TargetAndGroupFixture, group: &str) -> Value {
        fixture.config["profiles"]["app"]["deploymentGroups"][group]["hydrationRelations"].clone()
    }

    fn targets(fixture: &TargetAndGroupFixture) -> &Map<String, Value> {
        fixture.config["profiles"]["app"]["targetProfiles"]
            .as_object()
            .unwrap()
    }

    #[test]
    fn the_storefront_keeps_fifteens_relation() {
        let fixture = R::Storefront.fixture();
        assert_eq!(
            relations(&fixture, "web"),
            json!([{"server": "ssr", "client": "browser"}])
        );
        let capabilities = R::Storefront.capabilities();
        assert_eq!(
            capabilities.renders.into_iter().collect::<Vec<_>>(),
            ["storefront-ssr"]
        );
        assert_eq!(
            capabilities.hydrates.into_iter().collect::<Vec<_>>(),
            ["storefront-browser"]
        );
        assert!(capabilities.incompatible.is_empty());
        // Without relations, the group omits the member.
        assert_eq!(relations(&R::NoRelations.fixture(), "web"), Value::Null);
    }

    /// Each blocked recipe changes one part of the example: its relations,
    /// a target, the groups, the collection, or the fixture's answers.
    #[test]
    fn each_blocked_recipe_changes_only_what_its_name_says() {
        let base = R::Storefront.fixture();
        let storefront = json!({"server": "ssr", "client": "browser"});
        for (recipe, added) in [
            (R::EqualEndpoint, json!({"server": "ssr", "client": "ssr"})),
            (
                R::UnknownEndpoint,
                json!({"server": "ssr", "client": "phone"}),
            ),
            (R::DuplicateRelation, storefront.clone()),
        ] {
            let fixture = recipe.fixture();
            assert_eq!(fixture.members, base.members, "{recipe:?}");
            assert_eq!(
                relations(&fixture, "web"),
                json!([storefront.clone(), added]),
                "{recipe:?}"
            );
            let mut config = fixture.config.clone();
            config["profiles"]["app"]["deploymentGroups"]["web"]["hydrationRelations"] =
                json!([storefront.clone()]);
            assert_eq!(config, base.config, "{recipe:?}");
        }
        for recipe in [
            R::ServerCapabilityMissing,
            R::ClientCapabilityMissing,
            R::StaticIncompatibility,
        ] {
            let fixture = recipe.fixture();
            assert_eq!(
                (fixture.config, fixture.members),
                (base.config.clone(), base.members.clone())
            );
            assert_ne!(
                recipe.capabilities(),
                R::Storefront.capabilities(),
                "{recipe:?}"
            );
        }
        let target = |recipe: HydrationRecipe, id: &str| targets(&recipe.fixture())[id].clone();
        assert_eq!(
            target(R::RequestedLocales, "ssr")["requestedLocales"],
            json!(["en", "fr-FR"])
        );
        assert_eq!(
            target(R::EffectiveDefault, "browser")["defaultRequestedLocale"],
            "fr-FR"
        );
        assert_eq!(
            target(R::EndpointLocalesUnresolved, "ssr")["requestedLocales"],
            json!(["en", "fr-FR", "ja", "de"])
        );
        let unresolved = R::GroupMembersUnresolved.fixture();
        assert_eq!(
            unresolved.config["profiles"]["app"]["deploymentGroups"]["web"]["members"],
            json!(["browser", "ssr", "tablet"])
        );
        let unavailable = R::NegotiationUnavailable.fixture();
        assert_eq!(
            unavailable.config["profiles"]["app"]["localeNegotiation"],
            json!({"aliases": {"fr": "en_US"}})
        );
        assert_eq!(unavailable.members, base.members);
        let missing = R::TargetProfileMissing.fixture();
        assert_eq!(missing.config, base.config);
        assert_eq!(missing.members.len(), base.members.len() - 1);
        let cross = R::CrossGroup.fixture();
        assert_eq!(
            cross.config["profiles"]["app"]["deploymentGroups"]["jobs"],
            json!({"members": ["worker"]})
        );
        assert_eq!(
            relations(&cross, "web"),
            json!([storefront, {"server": "ssr", "client": "worker"}])
        );
        // `browser` serves `app` and hydrates from `ssr`, and the fixture
        // answers that it can do both.
        let dual = R::DualRole.capabilities();
        assert!(
            dual.renders.contains("storefront-browser")
                && dual.hydrates.contains("storefront-browser")
        );
    }

    #[test]
    fn the_fans_and_the_scale_declare_what_their_names_say() {
        let fan = R::FanOutFanIn.fixture();
        assert_eq!(
            relations(&fan, "web"),
            json!([
                {"server": "ssr", "client": "browser"},
                {"server": "ssr", "client": "app"},
                {"server": "edge", "client": "browser"}
            ])
        );
        // The same declaration with every object's members, the group's
        // members and relations, and the collection reversed.
        let reordered = R::FanReordered.fixture();
        assert_ne!(
            serde_json::to_vec(&reordered.config).unwrap(),
            serde_json::to_vec(&fan.config).unwrap()
        );
        let mut restored = reordered.config.clone();
        let web = &mut restored["profiles"]["app"]["deploymentGroups"]["web"];
        web["members"].as_array_mut().unwrap().reverse();
        web["hydrationRelations"].as_array_mut().unwrap().reverse();
        assert_eq!(restored, fan.config);
        assert_eq!(
            reordered.members.iter().rev().collect::<Vec<_>>(),
            fan.members.iter().collect::<Vec<_>>()
        );
        // Eight servers, each related to each of eight clients: 64 relations
        // among 16 targets that share two Target Profiles.
        let many = R::ManyRelations.fixture();
        assert_eq!(targets(&many).len(), 2 * SIDE);
        assert_eq!(
            relations(&many, "web").as_array().unwrap().len(),
            SIDE * SIDE
        );
        assert_eq!(many.members.len(), 7);
    }
}
