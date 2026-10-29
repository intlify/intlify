// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite Phase 13 inputs: 015's storefront example and its relation, a group
//! without relations, fan-out and fan-in, a scale of relations among few
//! targets, and one change of the example for each decision Phase 13 reaches
//! or waits on. Each Target Profile declares its hydration roles in its body
//! (024), and the static incompatibility is a client whose Target Profile
//! names another MF2 specification revision than its server's.

use serde::{Deserialize, Serialize};
use serde_json::json;

use super::target_group_inputs::{
    Declared, Target, TargetAndGroupFixture, BROWSER_HYDRATION_CLIENT, SSR_RENDERER, STOREFRONT,
};

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
                declared.declare("storefront-edge", &[SSR_RENDERER]);
                declared.declare("storefront-app", &[BROWSER_HYDRATION_CLIENT]);
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
            // The server's Target Profile declares no role.
            Self::ServerCapabilityMissing => declared.declare("storefront-ssr", &[]),
            // The client's Target Profile declares no role.
            Self::ClientCapabilityMissing => declared.declare("storefront-browser", &[]),
            // `browser` hydrates from `ssr` and serves `app`, and its Target
            // Profile declares both roles.
            Self::DualRole => {
                add(&mut declared, "app", "storefront-app");
                declared.declare(
                    "storefront-browser",
                    &[BROWSER_HYDRATION_CLIENT, SSR_RENDERER],
                );
                declared.declare("storefront-app", &[BROWSER_HYDRATION_CLIENT]);
                declared.relations.push(pair("browser", "app"));
            }
            // The client's Target Profile names another MF2 specification
            // revision than the server's.
            Self::StaticIncompatibility => {
                declared.content("storefront-browser")["mf2Specification"]["revision"] =
                    json!("47");
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
            Self::Storefront | Self::TargetProfileMissing => {}
        }
        declared
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

    /// The content of the submitted Target Profile `identity` names.
    fn content<'a>(fixture: &'a TargetAndGroupFixture, identity: &str) -> &'a Value {
        &fixture
            .members
            .iter()
            .find(|member| {
                member["kind"] == "target-profile" && member["body"]["identity"] == identity
            })
            .unwrap()["body"]["content"]
    }

    fn roles(fixture: &TargetAndGroupFixture, identity: &str) -> Value {
        content(fixture, identity)["hydration"]["roles"].clone()
    }

    #[test]
    fn the_storefront_keeps_fifteens_relation() {
        let fixture = R::Storefront.fixture();
        assert_eq!(
            relations(&fixture, "web"),
            json!([{"server": "ssr", "client": "browser"}])
        );
        assert_eq!(roles(&fixture, "storefront-ssr"), json!(["ssr-renderer"]));
        assert_eq!(
            roles(&fixture, "storefront-browser"),
            json!(["browser-hydration-client"])
        );
        // Without relations, the group omits the member.
        assert_eq!(relations(&R::NoRelations.fixture(), "web"), Value::Null);
    }

    /// Each blocked recipe changes one part of the example: its relations,
    /// a target, the groups, the collection, or one endpoint's Target
    /// Profile.
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
        // A role or the MF2 specification revision of one endpoint's Target
        // Profile: only its content and the reference naming it change.
        for (recipe, target, identity, pointer, value) in [
            (
                R::ServerCapabilityMissing,
                "ssr",
                "storefront-ssr",
                "/hydration/roles",
                json!([]),
            ),
            (
                R::ClientCapabilityMissing,
                "browser",
                "storefront-browser",
                "/hydration/roles",
                json!([]),
            ),
            (
                R::StaticIncompatibility,
                "browser",
                "storefront-browser",
                "/mf2Specification/revision",
                json!("47"),
            ),
        ] {
            let fixture = recipe.fixture();
            let (changed, original) = (content(&fixture, identity), content(&base, identity));
            assert_eq!(changed.pointer(pointer), Some(&value), "{recipe:?}");
            assert_ne!(changed, original, "{recipe:?}");
            let mut restored = changed.clone();
            *restored.pointer_mut(pointer).unwrap() = original.pointer(pointer).unwrap().clone();
            assert_eq!(&restored, original, "{recipe:?}");
            let others = |fixture: &TargetAndGroupFixture| {
                fixture
                    .members
                    .iter()
                    .filter(|member| member["body"]["identity"] != identity)
                    .cloned()
                    .collect::<Vec<_>>()
            };
            assert_eq!(others(&fixture), others(&base), "{recipe:?}");
            assert_ne!(fixture.config, base.config, "{recipe:?}");
            let mut config = fixture.config.clone();
            config["profiles"]["app"]["targetProfiles"][target]["profile"] =
                targets(&base)[target]["profile"].clone();
            assert_eq!(config, base.config, "{recipe:?}");
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
        // `browser` hydrates from `ssr` and serves `app`, and its Target
        // Profile declares both roles.
        let dual = R::DualRole.fixture();
        assert_eq!(
            roles(&dual, "storefront-browser"),
            json!(["browser-hydration-client", "ssr-renderer"])
        );
        assert_eq!(
            roles(&dual, "storefront-app"),
            json!(["browser-hydration-client"])
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
        // `edge` serves and `app` hydrates, as their Target Profiles declare.
        assert_eq!(roles(&fan, "storefront-edge"), json!(["ssr-renderer"]));
        assert_eq!(
            roles(&fan, "storefront-app"),
            json!(["browser-hydration-client"])
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
