// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's four cross-authority rules, in the order construction applies
//! them.
//!
//! Each relates two construction authorities that are admitted one at a time,
//! so none of them can be checked inside a single authority's own schema. The
//! cross-authority-rules member records each rule's ID with the relationship
//! design 015 states; construction applies them in this order, after every
//! authority's own rows, and refuses with one row when any fails.
//!
//! A rule reads the Profile Specification's body as its admitted schema left
//! it. That schema is the one the input supplied, which may admit any body, so
//! a member a rule reads may be missing or of another type: the rule then
//! fails. Only the implemented authority, construction's last row, fixes the
//! body.

use std::collections::BTreeSet;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::component::Declared;
use super::flow::AdmittedAuthorities;

/// One cross-authority rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CrossAuthorityRule {
    ProfileAssociation,
    ProfileAuthorities,
    CapabilityBoundMirror,
    RegistryPhases,
}

impl CrossAuthorityRule {
    /// Every rule, in the order construction applies them.
    pub(crate) const ALL: [Self; 4] = [
        Self::ProfileAssociation,
        Self::ProfileAuthorities,
        Self::CapabilityBoundMirror,
        Self::RegistryPhases,
    ];

    /// The relationship the rule requires, as design 015 states it.
    pub(crate) const fn relationship(self) -> &'static str {
        match self {
            Self::ProfileAssociation => {
                "The Configuration Schema Authority Set's profileSpecification equals the admitted \
                 Profile Specification's identity, revision, and semantic digest."
            }
            Self::ProfileAuthorities => {
                "The Profile Specification's authorities name the admitted Profile Specification \
                 Schema, Configuration Schema Authority Set, Capability Specification, and \
                 Registry Package."
            }
            Self::CapabilityBoundMirror => {
                "The Capability Specification's policyComparable paths equal the Profile \
                 Specification's resourceBounds, in order."
            }
            Self::RegistryPhases => {
                "Every phase ID in the Check Registry is one of the Profile Specification's phases."
            }
        }
    }

    /// Whether the authorities construction admitted satisfy the rule.
    pub(crate) fn holds(self, admitted: &AdmittedAuthorities) -> bool {
        let profile = &admitted.profile_specification;
        let body = &profile.body;
        match self {
            Self::ProfileAssociation => {
                let association = &admitted
                    .configuration_schema_authority_set
                    .profile_specification;
                association.identity == profile.declared.identity
                    && association.revision == profile.declared.revision
                    && association.semantic_digest == profile.semantic_digest.as_str()
            }
            Self::ProfileAuthorities => {
                let authorities = body.get("authorities");
                [
                    (
                        "profileSpecificationSchema",
                        &admitted.profile_specification_schema.declared,
                    ),
                    (
                        "configurationSchemaAuthoritySet",
                        &admitted.configuration_schema_authority_set.declared,
                    ),
                    (
                        "resolverCapabilitySpecification",
                        &admitted.resolver_capability_specification.declared,
                    ),
                    ("registryPackage", &admitted.registry_package.declared),
                ]
                .into_iter()
                .all(|(member, declared)| {
                    names(authorities.and_then(|named| named.get(member)), declared)
                })
            }
            Self::CapabilityBoundMirror => {
                let paths = admitted
                    .resolver_capability_specification
                    .body
                    .policy_comparable
                    .iter()
                    .map(|bound| Some(bound.path.as_str()));
                body.get("resourceBounds")
                    .and_then(Value::as_array)
                    .is_some_and(|bounds| bounds.iter().map(Value::as_str).eq(paths))
            }
            Self::RegistryPhases => {
                let Some(phases) = body.get("phases").and_then(Value::as_array) else {
                    return false;
                };
                let listed = phases
                    .iter()
                    .filter_map(Value::as_str)
                    .collect::<BTreeSet<_>>();
                admitted
                    .registry_package
                    .checks
                    .rows
                    .iter()
                    .all(|row| listed.contains(row.phase_id.id()))
            }
        }
    }
}

/// Whether a pin in the Profile Specification's body names exactly the
/// identity and revision an authority declares.
fn names(pin: Option<&Value>, declared: &Declared) -> bool {
    let member = |name: &str| pin.and_then(|pin| pin.get(name)).and_then(Value::as_str);
    member("identity") == Some(declared.identity.as_str())
        && member("revision") == Some(declared.revision.as_str())
}

/// One rule as the cross-authority-rules member records it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuleRecord {
    pub(crate) id: CrossAuthorityRule,
    pub(crate) relationship: &'static str,
}

/// The cross-authority-rules member's body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CrossAuthorityRules {
    pub(crate) rules: Vec<RuleRecord>,
}

impl CrossAuthorityRules {
    /// The revision-`"0"` body.
    pub(crate) fn revision_zero() -> Self {
        Self {
            rules: CrossAuthorityRule::ALL
                .iter()
                .map(|&id| RuleRecord {
                    id,
                    relationship: id.relationship(),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::construction::authority_set::ProfileAssociation;
    use crate::construction::flow::construct;
    use crate::construction::flow::testing::committed;
    use crate::construction::root::ConstructionRoot;

    use CrossAuthorityRule as R;

    /// The rules that fail for the committed authorities after `change`.
    fn failing(change: impl FnOnce(&mut AdmittedAuthorities)) -> Vec<CrossAuthorityRule> {
        let mut admitted = construct(ConstructionRoot::built_in().unwrap(), &committed()).unwrap();
        change(&mut admitted);
        CrossAuthorityRule::ALL
            .into_iter()
            .filter(|rule| !rule.holds(&admitted))
            .collect()
    }

    /// The committed body's array at `member`.
    fn array<'a>(admitted: &'a mut AdmittedAuthorities, member: &str) -> &'a mut Vec<Value> {
        admitted.profile_specification.body[member]
            .as_array_mut()
            .unwrap()
    }

    #[test]
    fn the_rules_are_015s_four_in_their_order() {
        assert_eq!(
            CrossAuthorityRule::ALL.map(|rule| serde_json::to_value(rule).unwrap()),
            [
                "profile-association",
                "profile-authorities",
                "capability-bound-mirror",
                "registry-phases"
            ]
            .map(|rule| json!(rule))
        );
    }

    #[test]
    fn each_rule_is_recorded_with_its_relationship() {
        let body = serde_json::to_value(CrossAuthorityRules::revision_zero()).unwrap();
        let rules = body["rules"].as_array().unwrap();
        assert_eq!(rules.len(), 4);
        assert_eq!(rules[0]["id"], "profile-association");
        assert_eq!(
            rules[2]["relationship"],
            "The Capability Specification's policyComparable paths equal the Profile \
             Specification's resourceBounds, in order."
        );
        for (rule, record) in CrossAuthorityRule::ALL.iter().zip(rules) {
            assert_eq!(record["relationship"], rule.relationship());
            assert!(rule.relationship().ends_with('.'));
        }
    }

    #[test]
    fn the_committed_authorities_satisfy_every_rule() {
        assert_eq!(failing(|_| {}), []);
    }

    /// The set's association with its Profile Specification.
    fn association(admitted: &mut AdmittedAuthorities) -> &mut ProfileAssociation {
        &mut admitted
            .configuration_schema_authority_set
            .profile_specification
    }

    #[test]
    fn the_association_is_the_profile_specifications_identity_revision_and_semantic_digest() {
        assert_eq!(
            failing(|admitted| association(admitted).identity.push('x')),
            [R::ProfileAssociation]
        );
        assert_eq!(
            failing(|admitted| association(admitted).revision = "1".to_owned()),
            [R::ProfileAssociation]
        );
        assert_eq!(
            failing(|admitted| {
                association(admitted).semantic_digest.pop();
            }),
            [R::ProfileAssociation]
        );
    }

    #[test]
    fn the_body_names_each_admitted_authority_by_identity_and_revision() {
        for member in [
            "profileSpecificationSchema",
            "configurationSchemaAuthoritySet",
            "resolverCapabilitySpecification",
            "registryPackage",
        ] {
            for name in ["identity", "revision"] {
                assert_eq!(
                    failing(|admitted| {
                        admitted.profile_specification.body["authorities"][member][name] =
                            json!("other");
                    }),
                    [R::ProfileAuthorities],
                    "{member}.{name}"
                );
            }
        }
        // The admitted side is compared too, and nothing else of a pin is.
        assert_eq!(
            failing(|admitted| admitted.registry_package.declared.revision = "1".to_owned()),
            [R::ProfileAuthorities]
        );
        assert_eq!(
            failing(|admitted| {
                admitted.profile_specification.body["authorities"]["registryPackage"]["digest"] =
                    json!(admitted.registry_package.declared.digest);
            }),
            []
        );
    }

    #[test]
    fn the_bounds_mirror_the_capability_paths_in_order() {
        assert_eq!(
            failing(|admitted| array(admitted, "resourceBounds").swap(0, 1)),
            [R::CapabilityBoundMirror]
        );
        assert_eq!(
            failing(|admitted| {
                array(admitted, "resourceBounds").pop();
            }),
            [R::CapabilityBoundMirror]
        );
        assert_eq!(
            failing(|admitted| {
                admitted
                    .resolver_capability_specification
                    .body
                    .policy_comparable
                    .pop();
            }),
            [R::CapabilityBoundMirror]
        );
        assert_eq!(
            failing(|admitted| {
                let last = array(admitted, "resourceBounds").last().cloned().unwrap();
                array(admitted, "resourceBounds").push(last);
            }),
            [R::CapabilityBoundMirror]
        );
    }

    #[test]
    fn every_phase_the_check_registry_uses_is_listed() {
        assert_eq!(
            failing(|admitted| {
                array(admitted, "phases").retain(|phase| phase != "target-resolution");
            }),
            [R::RegistryPhases]
        );
        // The rule asks for membership only: a phase no row uses may be
        // missing, and the order is not compared.
        assert_eq!(
            failing(|admitted| {
                array(admitted, "phases").retain(|phase| phase != "profile-projection");
            }),
            []
        );
        assert_eq!(failing(|admitted| array(admitted, "phases").reverse()), []);
    }

    /// A body its schema left open may lack a member a rule reads, or hold it
    /// as another type: that rule fails, and the others still hold.
    #[test]
    fn a_member_a_rule_reads_may_be_missing_or_of_another_type() {
        fn body(admitted: &mut AdmittedAuthorities) -> &mut Value {
            &mut admitted.profile_specification.body
        }
        for value in [json!({}), json!([])] {
            assert_eq!(
                failing(|admitted| *body(admitted) = value),
                [
                    R::ProfileAuthorities,
                    R::CapabilityBoundMirror,
                    R::RegistryPhases
                ]
            );
        }
        assert_eq!(
            failing(|admitted| body(admitted)["authorities"]["registryPackage"] = json!("x")),
            [R::ProfileAuthorities]
        );
        assert_eq!(
            failing(|admitted| body(admitted)["resourceBounds"] = json!("input.maxNodes")),
            [R::CapabilityBoundMirror]
        );
        assert_eq!(
            failing(|admitted| body(admitted)["resourceBounds"][0] = json!(false)),
            [R::CapabilityBoundMirror]
        );
        assert_eq!(
            failing(|admitted| body(admitted)["phases"] = json!({ "0": "target-resolution" })),
            [R::RegistryPhases]
        );
    }
}
