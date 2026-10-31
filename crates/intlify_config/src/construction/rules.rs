// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's four cross-authority rules, in the order construction applies
//! them.
//!
//! Each relates two construction authorities that are admitted one at a time,
//! so none of them can be checked inside a single authority's own schema. The
//! cross-authority-rules member records each rule's ID with the relationship
//! design 015 states; construction applies them in this order and reports the
//! first that fails.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

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
}
