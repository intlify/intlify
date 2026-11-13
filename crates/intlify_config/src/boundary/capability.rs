// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `implementation-capability` boundary: one submitted capability value,
//! admitted against the constructed Capability Specification or refused with
//! one of the nine reasons of its row (D15:1764-1788, 3150).
//!
//! The value names the specification it asserts and sets every bound of both
//! namespaces: `policyComparable`, the mirror of a Resource Limit Policy's
//! `projectProfileResolution` section, and `bootstrapOnly`, the work no policy
//! bounds. The boundary checks, in order: that a value is present; its closed
//! top level; the asserted specification's identity and then its revision,
//! before the inventory that specification defines; every group and bound of
//! the inventory, refusing an unknown or mistyped member before a missing one;
//! and every bound's value, in the order of the reasons: its spelling, zero,
//! its size, and last its minimum (D15:1788). The first check that fails
//! decides, and among the bounds failing the same check the first in the
//! specification's canonical order is named (D15:3155, 3157). Construction
//! admits only the Capability Specification this crate writes from its
//! inventory, so the inventory and its minimum vector are the constructed
//! specification's.
//!
//! Only complete success admits the capability. The admitted value keeps the
//! specification it asserts and both namespaces, and writes them back as the
//! normalized value the Materialized Resolution Input Set carries (D15:205).

use serde_json::{Map, Value};

use crate::input_limits::{Bound, BoundFailure};
use crate::locale::VersionedIdentity;
use crate::resolver::capability::{
    BootstrapLimits, CapabilityBound, BOOTSTRAP_ONLY, POLICY_COMPARABLE,
};
use crate::resolver::limits::ResolutionLimits;

/// The top-level member that names the asserted specification.
const SPECIFICATION: &str = "specification";

/// An Admitted Implementation Capability: the Capability Specification it
/// asserts, and every bound of both namespaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ImplementationCapability {
    specification: VersionedIdentity<String>,
    policy_comparable: ResolutionLimits,
    bootstrap_only: BootstrapLimits,
}

impl ImplementationCapability {
    /// A capability a fixture owner states outright, as the boundary would
    /// admit it: the specification this crate writes, the given
    /// `policyComparable` bounds, and `bootstrapOnly`'s minimum vector except
    /// `artifactBootstrap`, whose bounds are as large as a bound can be: at its
    /// minima no Resource Limit Policy can be admitted (Decision 015-164).
    #[cfg(any(test, feature = "benchmark", feature = "dev-example"))]
    pub(crate) fn fixture_owned(policy_comparable: ResolutionLimits) -> Self {
        use crate::construction::capability::{IDENTITY, REVISION};
        use crate::input_limits::Bound;
        use crate::resolver::capability::ArtifactBootstrapLimits;
        let largest = Bound::new(u64::MAX - 1).expect("the largest bound is representable");
        let bootstrap_only = BootstrapLimits {
            artifact_bootstrap: ArtifactBootstrapLimits {
                max_envelope_depth: largest,
                max_decoded_allocation_bytes: largest,
                max_metadata_scan_units: largest,
                max_validation_work_units: largest,
            },
            ..BootstrapLimits::MINIMUM
        };
        Self {
            specification: VersionedIdentity {
                identity: IDENTITY.to_owned(),
                revision: REVISION.to_owned(),
            },
            policy_comparable,
            bootstrap_only,
        }
    }

    /// The same capability with its `artifactBootstrap` bounds replaced, for
    /// the tests of the bootstrap's work.
    #[cfg(test)]
    pub(crate) fn with_artifact_bootstrap(
        mut self,
        limits: crate::resolver::capability::ArtifactBootstrapLimits,
    ) -> Self {
        self.bootstrap_only.artifact_bootstrap = limits;
        self
    }

    /// The same capability with its `localeData` bounds replaced, for the
    /// tests of specification admission's reading of the data.
    #[cfg(test)]
    pub(crate) fn with_locale_data(mut self, limits: crate::locale::LocaleDataLimits) -> Self {
        self.bootstrap_only.locale_data = limits;
        self
    }

    /// The Capability Specification the capability asserts.
    pub(crate) const fn specification(&self) -> &VersionedIdentity<String> {
        &self.specification
    }

    /// The `policyComparable` bounds: before a Resource Limit Policy is
    /// admitted, the sole authority for the work they name, and then the
    /// values Phase 4 compares the policy with.
    pub(crate) const fn policy_comparable(&self) -> &ResolutionLimits {
        &self.policy_comparable
    }

    /// The `bootstrapOnly` bounds, which no Resource Limit Policy has.
    pub(crate) const fn bootstrap_only(&self) -> &BootstrapLimits {
        &self.bootstrap_only
    }

    /// The normalized value: the closed shape the boundary admits, every
    /// group and bound in the canonical order, and every bound as its
    /// shortest decimal string. The boundary admits it as this capability.
    pub(crate) fn normalized(&self) -> Value {
        let mut specification = Map::new();
        specification.insert(
            "identity".to_owned(),
            Value::from(self.specification.identity.as_str()),
        );
        specification.insert(
            "revision".to_owned(),
            Value::from(self.specification.revision.as_str()),
        );
        let mut value = Map::new();
        value.insert(SPECIFICATION.to_owned(), Value::Object(specification));
        // Each bound goes in at its capability bound path, in the canonical
        // order, so every namespace and group first appears where its first
        // bound does.
        for (bound, setting) in self.bounds() {
            let path = bound.to_string();
            let mut segments: Vec<&str> = path.split('.').collect();
            let name = segments.pop().expect("a bound path names its bound");
            let mut node = &mut value;
            for segment in segments {
                node = node
                    .entry(segment)
                    .or_insert_with(|| Value::Object(Map::new()))
                    .as_object_mut()
                    .expect("a namespace or group is an object");
            }
            node.insert(name.to_owned(), Value::from(setting.get().to_string()));
        }
        Value::Object(value)
    }

    /// Every bound of both namespaces, in the canonical order.
    fn bounds(&self) -> impl Iterator<Item = (CapabilityBound, Bound)> {
        let policy = self
            .policy_comparable
            .bounds()
            .map(|(id, bound)| (CapabilityBound::PolicyComparable(id), bound));
        let bootstrap = self
            .bootstrap_only
            .bounds()
            .map(|(id, bound)| (CapabilityBound::BootstrapOnly(id), bound));
        policy.into_iter().chain(bootstrap)
    }
}

/// Why the boundary refuses a submitted value: one of the nine reasons of its
/// row, in that row's order. A refusal of a bound's value names the bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityRefusal {
    /// No value.
    Missing,
    /// A top level other than exactly the specification and the two
    /// namespaces as objects, or, against the inventory, an unknown group or
    /// bound or a member of the wrong type.
    Malformed,
    /// A specification other than the constructed one.
    SpecificationIdentityMismatch,
    /// The constructed specification at another revision.
    SpecificationRevisionMismatch,
    /// A group or bound of the inventory left out.
    Incomplete,
    /// A JSON number, or a string other than a count's shortest decimal digits.
    NonCanonical(CapabilityBound),
    /// `"0"`.
    NonPositive(CapabilityBound),
    /// `"18446744073709551615"` or more.
    Unrepresentable(CapabilityBound),
    /// Below the bound's minimum.
    BelowMinimum(CapabilityBound),
}

impl CapabilityRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-capability",
            Self::Malformed => "malformed-capability",
            Self::SpecificationIdentityMismatch => "capability-specification-identity-mismatch",
            Self::SpecificationRevisionMismatch => "capability-specification-revision-mismatch",
            Self::Incomplete => "incomplete-capability",
            Self::NonCanonical(_) => "non-canonical-bound",
            Self::NonPositive(_) => "non-positive-bound",
            Self::Unrepresentable(_) => "unrepresentable-bound",
            Self::BelowMinimum(_) => "below-bootstrap-minimum",
        }
    }

    /// The bound a refusal of a value names.
    pub(crate) const fn bound(self) -> Option<CapabilityBound> {
        match self {
            Self::NonCanonical(bound)
            | Self::NonPositive(bound)
            | Self::Unrepresentable(bound)
            | Self::BelowMinimum(bound) => Some(bound),
            Self::Missing
            | Self::Malformed
            | Self::SpecificationIdentityMismatch
            | Self::SpecificationRevisionMismatch
            | Self::Incomplete => None,
        }
    }
}

/// Admit `submitted`, or `None` when no value was submitted, against the
/// constructed Capability Specification `constructed`: the capability, or the
/// boundary's first failing check.
pub(crate) fn admit<Id: AsRef<str>>(
    submitted: Option<&Value>,
    constructed: &VersionedIdentity<Id>,
) -> Result<ImplementationCapability, CapabilityRefusal> {
    let value = submitted.ok_or(CapabilityRefusal::Missing)?;
    let (identity, revision) = top_level(value).ok_or(CapabilityRefusal::Malformed)?;
    if identity != constructed.identity.as_ref() {
        return Err(CapabilityRefusal::SpecificationIdentityMismatch);
    }
    if revision != constructed.revision.as_ref() {
        return Err(CapabilityRefusal::SpecificationRevisionMismatch);
    }
    let specification = VersionedIdentity {
        identity: identity.to_owned(),
        revision: revision.to_owned(),
    };
    // The least capability under the specification has every group and bound
    // of the inventory, so its normalized value is the shape to check.
    let shape = ImplementationCapability {
        specification: specification.clone(),
        policy_comparable: ResolutionLimits::from_bounds(|_| Bound::new(1))
            .expect("every bound is supplied"),
        bootstrap_only: BootstrapLimits::MINIMUM,
    }
    .normalized();
    if !known(value, &shape) {
        return Err(CapabilityRefusal::Malformed);
    }
    if !complete(value, &shape) {
        return Err(CapabilityRefusal::Incomplete);
    }
    let read = CapabilityBound::inventory().map(|(bound, minimum)| {
        let member = bound
            .to_string()
            .split('.')
            .try_fold(value, |node, name| node.get(name))
            .expect("a complete value has every bound");
        (bound, minimum, spelled(member))
    });
    for failure in [
        BoundFailure::NonCanonical,
        BoundFailure::NonPositive,
        BoundFailure::Unrepresentable,
    ] {
        if let Some((bound, ..)) = read.iter().find(|(.., value)| *value == Err(failure)) {
            return Err(value_refusal(failure, *bound));
        }
    }
    let below = read
        .iter()
        .find(|(_, minimum, value)| value.is_ok_and(|value| value.get() < minimum.get()));
    if let Some((bound, ..)) = below {
        return Err(CapabilityRefusal::BelowMinimum(*bound));
    }
    let bound = |wanted: CapabilityBound| {
        read.iter()
            .find(|(bound, ..)| *bound == wanted)
            .and_then(|(.., value)| value.ok())
    };
    Ok(ImplementationCapability {
        specification,
        policy_comparable: ResolutionLimits::from_bounds(|id| {
            bound(CapabilityBound::PolicyComparable(id))
        })
        .expect("every policyComparable bound was read"),
        bootstrap_only: BootstrapLimits::from_bounds(|id| {
            bound(CapabilityBound::BootstrapOnly(id))
        })
        .expect("every bootstrapOnly bound was read"),
    })
}

/// The asserted specification's identity and revision, when the top level is
/// exactly the specification and the two namespaces as objects and the
/// specification is exactly its identity and revision as strings.
fn top_level(value: &Value) -> Option<(&str, &str)> {
    let members = value.as_object()?;
    let closed = members.len() == 3
        && [POLICY_COMPARABLE, BOOTSTRAP_ONLY]
            .into_iter()
            .all(|namespace| members.get(namespace).is_some_and(Value::is_object));
    let specification = members.get(SPECIFICATION)?.as_object()?;
    let identity = specification.get("identity")?.as_str()?;
    let revision = specification.get("revision")?.as_str()?;
    (closed && specification.len() == 2).then_some((identity, revision))
}

/// Whether every member of `value` is one `shape` has: an object where
/// `shape` has an object, and a string or a number where it has a bound.
fn known(value: &Value, shape: &Value) -> bool {
    match (value, shape) {
        (Value::Object(members), Value::Object(expected)) => {
            members.iter().all(|(name, member)| {
                expected
                    .get(name)
                    .is_some_and(|expected| known(member, expected))
            })
        }
        (Value::String(_) | Value::Number(_), Value::String(_)) => true,
        _ => false,
    }
}

/// Whether `value` has every member `shape` has.
fn complete(value: &Value, shape: &Value) -> bool {
    match (value, shape) {
        (Value::Object(members), Value::Object(expected)) => {
            expected.iter().all(|(name, expected)| {
                members
                    .get(name)
                    .is_some_and(|member| complete(member, expected))
            })
        }
        _ => true,
    }
}

/// The refusal of `bound`'s value that `failure` names.
const fn value_refusal(failure: BoundFailure, bound: CapabilityBound) -> CapabilityRefusal {
    match failure {
        BoundFailure::NonCanonical => CapabilityRefusal::NonCanonical(bound),
        BoundFailure::NonPositive => CapabilityRefusal::NonPositive(bound),
        BoundFailure::Unrepresentable => CapabilityRefusal::Unrepresentable(bound),
    }
}

/// The bound a member spells. The shape admits only a string or a number,
/// and a JSON number is no shortest decimal string.
fn spelled(member: &Value) -> Result<Bound, BoundFailure> {
    match member {
        Value::String(text) => Bound::read(text),
        _ => Err(BoundFailure::NonCanonical),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::capability::{IDENTITY, REVISION};
    use crate::construction::resolver::committed;
    use crate::resolver::capability::{BootstrapBoundId, BootstrapGroup};
    use crate::resolver::limits::{fixture_limits, BoundId, Group};

    /// The first bound in the canonical order.
    const FIRST: CapabilityBound = CapabilityBound::PolicyComparable(BoundId {
        group: Group::ConfigurationInput,
        name: "maxNodes",
    });

    /// The first `bootstrapOnly` bound, after every `policyComparable` one.
    const RAW: CapabilityBound = CapabilityBound::BootstrapOnly(BootstrapBoundId {
        group: BootstrapGroup::RawInput,
        name: "maxFileBytes",
    });

    fn specification() -> VersionedIdentity<String> {
        VersionedIdentity {
            identity: IDENTITY.to_owned(),
            revision: REVISION.to_owned(),
        }
    }

    /// The capability with every bound at its minimum.
    fn least() -> ImplementationCapability {
        ImplementationCapability {
            specification: specification(),
            policy_comparable: ResolutionLimits::from_bounds(|_| Bound::new(1)).unwrap(),
            bootstrap_only: BootstrapLimits::MINIMUM,
        }
    }

    /// `least`'s normalized value with `edit` made to it.
    fn edited(edit: impl FnOnce(&mut Value)) -> Value {
        let mut value = least().normalized();
        edit(&mut value);
        value
    }

    /// The JSON Pointer to `bound` in a capability value.
    fn pointer(bound: CapabilityBound) -> String {
        format!("/{}", bound.to_string().replace('.', "/"))
    }

    fn set(value: &mut Value, bound: CapabilityBound, spelling: Value) {
        *value.pointer_mut(&pointer(bound)).unwrap() = spelling;
    }

    /// `least`'s value with `bound` set to `spelling`.
    fn with(bound: CapabilityBound, spelling: Value) -> Value {
        edited(|value| set(value, bound, spelling))
    }

    fn admitted(value: &Value) -> ImplementationCapability {
        admit(Some(value), &specification()).unwrap()
    }

    fn refused(value: &Value) -> CapabilityRefusal {
        admit(Some(value), &specification()).unwrap_err()
    }

    /// The bounds whose minimum is above one, in the canonical order.
    fn raised() -> Vec<(CapabilityBound, Bound)> {
        CapabilityBound::inventory()
            .into_iter()
            .filter(|(_, minimum)| minimum.get() > 1)
            .collect()
    }

    /// Every bound's capability bound path, as `value` lists them.
    fn listed(value: &Value, path: &str, paths: &mut Vec<String>) {
        match value {
            Value::Object(members) => {
                for (name, member) in members {
                    let path = if path.is_empty() {
                        name.clone()
                    } else {
                        format!("{path}.{name}")
                    };
                    listed(member, &path, paths);
                }
            }
            _ => paths.push(path.to_owned()),
        }
    }

    /// Reverses the member order of every object in `value`.
    fn reversed(value: &Value) -> Value {
        match value {
            Value::Object(members) => Value::Object(
                members
                    .iter()
                    .rev()
                    .map(|(name, member)| (name.clone(), reversed(member)))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    /// Every bound at its minimum is a capability, which writes back the
    /// value it was admitted from.
    #[test]
    fn the_minimum_vector_is_a_capability_that_normalizes_back() {
        let value = least().normalized();
        let capability = admitted(&value);
        assert_eq!(capability, least());
        assert_eq!(capability.normalized(), value);
        assert_eq!(capability.specification(), &specification());
        assert_eq!(capability.bootstrap_only(), &BootstrapLimits::MINIMUM);
        assert!(capability
            .policy_comparable()
            .bounds()
            .iter()
            .all(|(_, bound)| bound.get() == 1));
    }

    /// The normalized value lists the specification and then each namespace's
    /// groups and bounds in the canonical order, each bound as its shortest
    /// decimal string.
    #[test]
    fn the_normalized_value_follows_the_canonical_order() {
        let value = least().normalized();
        let top: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        assert_eq!(top, [SPECIFICATION, POLICY_COMPARABLE, BOOTSTRAP_ONLY]);
        assert_eq!(
            value[SPECIFICATION],
            json!({"identity": IDENTITY, "revision": REVISION})
        );
        let mut paths = Vec::new();
        listed(&value[POLICY_COMPARABLE], POLICY_COMPARABLE, &mut paths);
        listed(&value[BOOTSTRAP_ONLY], BOOTSTRAP_ONLY, &mut paths);
        let inventory: Vec<String> = CapabilityBound::inventory()
            .iter()
            .map(|(bound, _)| bound.to_string())
            .collect();
        assert_eq!(paths, inventory);
        for (bound, minimum) in CapabilityBound::inventory() {
            assert_eq!(
                value.pointer(&pointer(bound)),
                Some(&json!(minimum.get().to_string())),
                "{bound}"
            );
        }
    }

    /// Every bound may be the largest bound, and the smallest is one.
    #[test]
    fn every_bound_may_be_as_large_as_a_bound_is() {
        let largest = edited(|value| {
            for (bound, _) in CapabilityBound::inventory() {
                set(value, bound, json!("18446744073709551614"));
            }
        });
        let capability = admitted(&largest);
        let bounds = capability
            .policy_comparable()
            .bounds()
            .map(|(_, bound)| bound);
        assert!(bounds.iter().all(|bound| bound.get() == u64::MAX - 1));
        let bounds = capability.bootstrap_only().bounds().map(|(_, bound)| bound);
        assert!(bounds.iter().all(|bound| bound.get() == u64::MAX - 1));
        assert_eq!(capability.normalized(), largest);
        assert_eq!(
            admitted(&with(FIRST, json!("1")))
                .policy_comparable()
                .bounds()[0]
                .1,
            Bound::new(1).unwrap()
        );
    }

    #[test]
    fn an_absent_value_is_missing() {
        assert_eq!(
            admit(None, &specification()),
            Err(CapabilityRefusal::Missing)
        );
    }

    /// The top level is exactly the specification and the two namespaces as
    /// objects, and the specification exactly its identity and revision as
    /// strings.
    #[test]
    fn a_top_level_other_than_the_closed_one_is_malformed() {
        let cases = [
            ("an array", json!([])),
            ("a string", json!(IDENTITY)),
            ("null", Value::Null),
            (
                "no bootstrapOnly",
                edited(|value| {
                    value.as_object_mut().unwrap().remove(BOOTSTRAP_ONLY);
                }),
            ),
            (
                "another member",
                edited(|value| value["profile"] = json!({})),
            ),
            (
                "a namespace that is no object",
                edited(|value| value[POLICY_COMPARABLE] = json!([])),
            ),
            (
                "a specification that is no object",
                edited(|value| value[SPECIFICATION] = json!(IDENTITY)),
            ),
            (
                "a specification without its revision",
                edited(|value| {
                    value[SPECIFICATION]
                        .as_object_mut()
                        .unwrap()
                        .remove("revision");
                }),
            ),
            (
                "a specification with another member",
                edited(|value| value[SPECIFICATION]["digest"] = json!("sha256:0")),
            ),
            (
                "an identity that is no string",
                edited(|value| value[SPECIFICATION]["identity"] = json!(0)),
            ),
            (
                "another member and another identity",
                edited(|value| {
                    value["profile"] = json!({});
                    value[SPECIFICATION]["identity"] = json!("another-capability");
                }),
            ),
            (
                "a namespace that is no object and another identity",
                edited(|value| {
                    value[BOOTSTRAP_ONLY] = json!("1");
                    value[SPECIFICATION]["identity"] = json!("another-capability");
                }),
            ),
            (
                "a specification with another member and another revision",
                edited(|value| {
                    value[SPECIFICATION]["digest"] = json!("sha256:0");
                    value[SPECIFICATION]["revision"] = json!("1");
                }),
            ),
        ];
        for (name, value) in cases {
            assert_eq!(refused(&value), CapabilityRefusal::Malformed, "{name}");
        }
    }

    /// The asserted specification is compared before the inventory it
    /// defines: its identity, then its revision.
    #[test]
    fn the_specification_is_compared_before_the_inventory() {
        let identity = |value: &mut Value| {
            value[SPECIFICATION]["identity"] = json!("another-capability");
        };
        let revision = |value: &mut Value| value[SPECIFICATION]["revision"] = json!("1");
        let unknown = |value: &mut Value| value[BOOTSTRAP_ONLY]["unknownGroup"] = json!({});
        let missing = |value: &mut Value| {
            value[POLICY_COMPARABLE]
                .as_object_mut()
                .unwrap()
                .remove("targetGrouping");
        };
        let cases = [
            (
                "identity",
                edited(identity),
                CapabilityRefusal::SpecificationIdentityMismatch,
            ),
            (
                "revision",
                edited(revision),
                CapabilityRefusal::SpecificationRevisionMismatch,
            ),
            (
                "identity and revision",
                edited(|value| {
                    identity(value);
                    revision(value);
                }),
                CapabilityRefusal::SpecificationIdentityMismatch,
            ),
            (
                "revision and an unknown group",
                edited(|value| {
                    revision(value);
                    unknown(value);
                }),
                CapabilityRefusal::SpecificationRevisionMismatch,
            ),
            (
                "identity and a missing group",
                edited(|value| {
                    identity(value);
                    missing(value);
                }),
                CapabilityRefusal::SpecificationIdentityMismatch,
            ),
            (
                "revision and a zero",
                edited(|value| {
                    revision(value);
                    set(value, FIRST, json!("0"));
                }),
                CapabilityRefusal::SpecificationRevisionMismatch,
            ),
        ];
        for (name, value, refusal) in cases {
            assert_eq!(refused(&value), refusal, "{name}");
        }
    }

    /// Against the inventory, an unknown group or bound and a member of the
    /// wrong type are malformed, in either namespace and in `diagnostics`.
    #[test]
    fn an_unknown_or_mistyped_member_of_the_inventory_is_malformed() {
        let cases = [
            (
                "an unknown policyComparable group",
                edited(|value| value[POLICY_COMPARABLE]["unknownGroup"] = json!({})),
            ),
            (
                "an unknown bound",
                edited(|value| value[BOOTSTRAP_ONLY]["rawInput"]["maxUnknownBytes"] = json!("1")),
            ),
            (
                "an unknown diagnostics domain",
                edited(|value| value[POLICY_COMPARABLE]["diagnostics"]["provider"] = json!({})),
            ),
            (
                "a policyComparable group in bootstrapOnly",
                edited(|value| {
                    let group = value[POLICY_COMPARABLE]["localeResolution"].clone();
                    value[BOOTSTRAP_ONLY]["localeResolution"] = group;
                }),
            ),
            (
                "a group that is no object",
                edited(|value| value[BOOTSTRAP_ONLY]["snapshot"] = json!("1")),
            ),
            ("a null bound", with(FIRST, Value::Null)),
            ("a boolean bound", with(RAW, json!(true))),
            ("an array bound", with(FIRST, json!(["1"]))),
            ("an object bound", with(RAW, json!({"value": "1"}))),
        ];
        for (name, value) in cases {
            assert_eq!(refused(&value), CapabilityRefusal::Malformed, "{name}");
        }
    }

    /// A group or bound of the inventory left out makes the value incomplete.
    #[test]
    fn a_missing_group_or_bound_is_incomplete() {
        let removed = |pointer: &'static str, member: &'static str| {
            edited(move |value| {
                value
                    .pointer_mut(pointer)
                    .unwrap()
                    .as_object_mut()
                    .unwrap()
                    .remove(member);
            })
        };
        let cases = [
            (
                "a policyComparable group",
                removed("/policyComparable", "localeResolution"),
            ),
            (
                "a diagnostics domain",
                removed("/policyComparable/diagnostics", "shared"),
            ),
            (
                "a bootstrapOnly group",
                removed("/bootstrapOnly", "mandatoryReserve"),
            ),
            (
                "a bound",
                removed("/bootstrapOnly/rawInput", "maxParserTokens"),
            ),
            (
                "a namespace's every group",
                edited(|value| value[BOOTSTRAP_ONLY] = json!({})),
            ),
        ];
        for (name, value) in cases {
            assert_eq!(refused(&value), CapabilityRefusal::Incomplete, "{name}");
        }
    }

    /// An unknown or mistyped member is refused before a missing one.
    #[test]
    fn an_unknown_member_comes_before_a_missing_one() {
        let missing = |value: &mut Value| {
            value[POLICY_COMPARABLE]
                .as_object_mut()
                .unwrap()
                .remove("localeResolution");
        };
        let unknown = edited(|value| {
            missing(value);
            value[BOOTSTRAP_ONLY]["rawInput"]["maxUnknownBytes"] = json!("1");
        });
        assert_eq!(refused(&unknown), CapabilityRefusal::Malformed);
        let mistyped = edited(|value| {
            missing(value);
            set(value, RAW, Value::Null);
        });
        assert_eq!(refused(&mistyped), CapabilityRefusal::Malformed);
    }

    /// A value is judged by its spelling, then zero, then its size: a
    /// canonical spelling too large for a bound is unrepresentable, not
    /// non-canonical.
    #[test]
    fn a_value_is_judged_by_its_spelling_then_zero_then_its_size() {
        let non_canonical = [
            json!(1),
            json!(1.5),
            json!(-1),
            json!(""),
            json!("01"),
            json!("00"),
            json!("1a"),
            json!(" 1"),
            json!("+1"),
            json!("1e3"),
            json!("\u{ff11}"),
        ];
        for spelling in non_canonical {
            assert_eq!(
                refused(&with(FIRST, spelling.clone())),
                CapabilityRefusal::NonCanonical(FIRST),
                "{spelling}"
            );
        }
        assert_eq!(
            refused(&with(RAW, json!("0"))),
            CapabilityRefusal::NonPositive(RAW)
        );
        for spelling in [
            "18446744073709551615",
            "18446744073709551616",
            "99999999999999999999999",
        ] {
            assert_eq!(
                refused(&with(RAW, json!(spelling))),
                CapabilityRefusal::Unrepresentable(RAW),
                "{spelling}"
            );
        }
    }

    /// The first check that fails decides: a later bound's earlier reason
    /// comes before an earlier bound's later one.
    #[test]
    fn the_first_failing_check_decides_before_the_canonical_order() {
        let (raised, minimum) = raised()[0];
        let later = CapabilityBound::inventory()
            .into_iter()
            .rev()
            .find(|(_, minimum)| minimum.get() == 1)
            .unwrap()
            .0;
        let below = json!((minimum.get() - 1).to_string());
        let cases = [
            (
                "zero, then a number",
                edited(|value| {
                    set(value, FIRST, json!("0"));
                    set(value, RAW, json!(1));
                }),
                CapabilityRefusal::NonCanonical(RAW),
            ),
            (
                "too large, then zero",
                edited(|value| {
                    set(value, FIRST, json!("18446744073709551615"));
                    set(value, RAW, json!("0"));
                }),
                CapabilityRefusal::NonPositive(RAW),
            ),
            (
                "below the minimum, then too large",
                edited(|value| {
                    set(value, raised, below.clone());
                    set(value, later, json!("18446744073709551615"));
                }),
                CapabilityRefusal::Unrepresentable(later),
            ),
        ];
        for (name, value, refusal) in cases {
            assert_eq!(refused(&value), refusal, "{name}");
        }
    }

    /// Among bounds that fail the same check, the first in the canonical order
    /// is named, whatever order the value lists its members in.
    #[test]
    fn the_first_bound_in_the_canonical_order_is_named() {
        let value = edited(|value| {
            set(value, RAW, json!(1));
            set(value, FIRST, json!(1));
        });
        assert_eq!(refused(&value), CapabilityRefusal::NonCanonical(FIRST));
        assert_eq!(
            refused(&reversed(&value)),
            CapabilityRefusal::NonCanonical(FIRST)
        );
    }

    /// A bound below its minimum is refused and one at it admitted. With
    /// several below, the first in the canonical order is named. A bound whose
    /// minimum is one is below it only at zero, which is non-positive.
    #[test]
    fn a_bound_below_its_minimum_is_named_in_the_canonical_order() {
        let raised = raised();
        assert_eq!(raised.len(), 3);
        for &(bound, minimum) in &raised {
            let at = with(bound, json!(minimum.get().to_string()));
            assert!(admit(Some(&at), &specification()).is_ok(), "{bound}");
            let below = with(bound, json!((minimum.get() - 1).to_string()));
            assert_eq!(
                refused(&below),
                CapabilityRefusal::BelowMinimum(bound),
                "{bound}"
            );
        }
        let every = edited(|value| {
            for &(bound, minimum) in raised.iter().rev() {
                set(value, bound, json!((minimum.get() - 1).to_string()));
            }
        });
        assert_eq!(
            refused(&every),
            CapabilityRefusal::BelowMinimum(raised[0].0)
        );
        assert_eq!(
            refused(&with(RAW, json!("0"))),
            CapabilityRefusal::NonPositive(RAW)
        );
    }

    /// The order the value lists its members in decides nothing.
    #[test]
    fn the_member_order_of_the_value_decides_nothing() {
        assert_eq!(admitted(&reversed(&least().normalized())), least());
    }

    /// Each refusal spells one of the nine reasons of 015's boundary table, in
    /// its row's order, and a refusal of a value names its bound.
    #[test]
    fn the_refusals_are_015s_nine_reasons() {
        let refusals = [
            CapabilityRefusal::Missing,
            CapabilityRefusal::Malformed,
            CapabilityRefusal::SpecificationIdentityMismatch,
            CapabilityRefusal::SpecificationRevisionMismatch,
            CapabilityRefusal::Incomplete,
            CapabilityRefusal::NonCanonical(FIRST),
            CapabilityRefusal::NonPositive(RAW),
            CapabilityRefusal::Unrepresentable(FIRST),
            CapabilityRefusal::BelowMinimum(RAW),
        ];
        assert_eq!(
            refusals.map(CapabilityRefusal::reason),
            [
                "missing-capability",
                "malformed-capability",
                "capability-specification-identity-mismatch",
                "capability-specification-revision-mismatch",
                "incomplete-capability",
                "non-canonical-bound",
                "non-positive-bound",
                "unrepresentable-bound",
                "below-bootstrap-minimum",
            ]
        );
        assert_eq!(
            refusals.map(CapabilityRefusal::bound),
            [
                None,
                None,
                None,
                None,
                None,
                Some(FIRST),
                Some(RAW),
                Some(FIRST),
                Some(RAW),
            ]
        );
    }

    /// The committed resolver's Capability Specification is the one this
    /// crate writes, and a capability asserting it is admitted against it.
    #[test]
    fn a_capability_is_admitted_against_the_constructed_specification() {
        let constructed = committed().capability_specification();
        assert_eq!(constructed, specification());
        assert_eq!(
            admit(Some(&least().normalized()), &constructed),
            Ok(least())
        );
    }

    /// A fixture owner's capability is one the boundary admits as it is.
    #[test]
    fn a_fixture_capability_is_one_the_boundary_admits() {
        let fixture = ImplementationCapability::fixture_owned(fixture_limits(7));
        assert_eq!(admitted(&fixture.normalized()), fixture);
    }
}
