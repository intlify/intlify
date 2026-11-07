// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's construction flow: the construction-root bounds, the five
//! components, the cross-authority rules, and the implemented authority, in
//! the order of the construction-failure precedence.
//!
//! Construction first applies the construction-root bounds to the whole
//! input ([`bounds`](super::bounds)). It then checks the Profile
//! Specification Schema, the Profile Specification against it, the
//! Configuration Schema Authority Set, the Resolver Capability Specification,
//! and the Registry Package, each with its own rows, then applies the four
//! cross-authority rules in the order the construction-admission body lists
//! them ([`rules`](super::rules)), and ends with one check after every other:
//! every admitted authority must be the one the resolver is compiled from
//! (D15:991). It reports the first row that fails and nothing else.

use super::authority_set::{self, AdmittedSet, SubmittedSet};
use super::bounds;
use super::capability::{self, AdmittedCapabilitySpecification};
use super::component::{Refusal, Submitted};
use super::failure::{Check, Component};
use super::profile::{self, AdmittedProfileSpecification, AdmittedSchema};
use super::registry::{self, AdmittedPackage, RegistryRole, SubmittedPackage};
use super::root::ConstructionRoot;

use crate::model::CONFIGURATION_SCHEMA_VERSION;

const CROSS_AUTHORITY_ROW: Refusal = Refusal::at(Component::All, Check::CrossAuthorityRules);
const IMPLEMENTED_ROW: Refusal = Refusal::at(Component::All, Check::ImplementedAuthority);

/// Each configuration schema the resolver is compiled from, by
/// `schemaVersion`, as its file digest.
const IMPLEMENTED_MEMBERS: [(&str, &str); 1] =
    [(CONFIGURATION_SCHEMA_VERSION, authority_set::MEMBER_DIGEST)];

/// The Resolver Construction Input Set: the five components construction is
/// given, each a declared tuple and the exact bytes it declares. A component
/// that is not given is `None`, which its presence row refuses.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ConstructionInput {
    pub(crate) profile_specification_schema: Option<Submitted>,
    pub(crate) profile_specification: Option<Submitted>,
    pub(crate) configuration_schema_authority_set: Option<SubmittedSet>,
    pub(crate) resolver_capability_specification: Option<Submitted>,
    pub(crate) registry_package: Option<SubmittedPackage>,
}

/// The five authorities construction admitted.
#[derive(Debug)]
pub(crate) struct AdmittedAuthorities {
    pub(crate) profile_specification_schema: AdmittedSchema,
    pub(crate) profile_specification: AdmittedProfileSpecification,
    pub(crate) configuration_schema_authority_set: AdmittedSet,
    pub(crate) resolver_capability_specification: AdmittedCapabilitySpecification,
    pub(crate) registry_package: AdmittedPackage,
}

/// Construct from the input: the 43 rows of the construction-failure
/// precedence, in order, refusing with the first that fails.
pub(crate) fn construct(
    root: &ConstructionRoot,
    input: &ConstructionInput,
) -> Result<AdmittedAuthorities, Refusal> {
    bounds::check(root, input)?;
    let profile_specification_schema =
        profile::admit_schema(root, input.profile_specification_schema.as_ref())?;
    let profile_specification = profile::admit(
        root,
        &profile_specification_schema,
        input.profile_specification.as_ref(),
    )?;
    let configuration_schema_authority_set =
        authority_set::admit(root, input.configuration_schema_authority_set.as_ref())?;
    let resolver_capability_specification =
        capability::admit(root, input.resolver_capability_specification.as_ref())?;
    let registry_package = registry::admit(root, input.registry_package.as_ref())?;
    let admitted = AdmittedAuthorities {
        profile_specification_schema,
        profile_specification,
        configuration_schema_authority_set,
        resolver_capability_specification,
        registry_package,
    };
    let rules = &root.admission().cross_authority_rules;
    if !rules.iter().all(|rule| rule.holds(&admitted)) {
        return Err(CROSS_AUTHORITY_ROW);
    }
    if !implemented(&admitted) {
        return Err(IMPLEMENTED_ROW);
    }
    Ok(admitted)
}

/// Whether every admitted authority is the one the resolver is compiled from:
/// the Profile Specification by its semantic digest alone, and every other
/// component and member by its file digest (D15:991).
///
/// The rows before have checked each declared digest against the bytes, so
/// the declared digests are the ones compared.
fn implemented(admitted: &AdmittedAuthorities) -> bool {
    let set = &admitted.configuration_schema_authority_set;
    let package = &admitted.registry_package;
    admitted.profile_specification_schema.declared.digest == profile::SCHEMA_DIGEST
        && admitted.profile_specification.semantic_digest.as_str() == profile::SEMANTIC_DIGEST
        && set.declared.digest == authority_set::DIGEST
        && set
            .members
            .iter()
            .map(|(version, member)| (version.as_str(), member.member.digest.as_str()))
            .eq(IMPLEMENTED_MEMBERS)
        && admitted.resolver_capability_specification.declared.digest == capability::DIGEST
        && package.declared.digest == registry::DIGEST
        && RegistryRole::ALL
            .into_iter()
            .zip(&package.members)
            .all(|(role, member)| member.digest == role.implemented())
}

/// Helpers for tests that construct from the committed input.
#[cfg(test)]
pub(crate) mod testing {
    use serde_json::Value;

    use super::ConstructionInput;
    use crate::construction::{committed, component};

    pub(crate) use crate::construction::committed::{
        authority_set, package, profile_specification, AUTHORITY_SET, CAPABILITY,
        CONFIGURATION_SCHEMA, PACKAGE, PROFILE_SPECIFICATION, PROFILE_SPECIFICATION_SCHEMA,
        REGISTRY_MEMBERS,
    };

    /// An artifact's value, read as construction reads it.
    pub(crate) fn value(bytes: &[u8]) -> Value {
        component::strict_json(bytes).unwrap()
    }

    /// The bytes of a value, written with another layout than the committed
    /// artifacts.
    pub(crate) fn compact(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    /// The committed input: every committed authority and member, each
    /// declared as construction admits it.
    pub(crate) fn committed() -> ConstructionInput {
        committed::input()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::testing::{
        authority_set, committed, compact, package, profile_specification, value, AUTHORITY_SET,
        CONFIGURATION_SCHEMA, PACKAGE, PROFILE_SPECIFICATION, REGISTRY_MEMBERS,
    };
    use super::*;
    use crate::construction::admission::{ConstructionBoundId, DRAFT_7};
    use crate::construction::capability::CapabilitySpecification;
    use crate::construction::component::testing::{row, submitted};
    use crate::construction::component::{self, Artifact};
    use crate::construction::registry::check::CheckRegistry;
    use crate::construction::registry::finding::FindingRegistry;
    use crate::construction::rules::CrossAuthorityRule;

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    fn constructed(input: &ConstructionInput) -> Option<usize> {
        row(construct(root(), input))
    }

    /// The committed input with the Profile Specification's body changed,
    /// and the set's association changed to match it, so that only what the
    /// change itself breaks fails.
    fn with_body(change: impl FnOnce(&mut Value)) -> ConstructionInput {
        let mut body = value(PROFILE_SPECIFICATION);
        change(&mut body);
        let bytes = compact(&body);
        let mut input = committed();
        input.profile_specification = Some(profile_specification(&bytes));
        let semantic = profile::semantic_digest(&body).unwrap();
        input.configuration_schema_authority_set = Some(authority_set(&set_bytes(|set| {
            set["profileSpecification"]["semanticDigest"] = json!(semantic.as_str());
        })));
        input
    }

    /// The committed set's bytes after `change`.
    fn set_bytes(change: impl FnOnce(&mut Value)) -> Vec<u8> {
        let mut set = value(AUTHORITY_SET);
        change(&mut set);
        compact(&set)
    }

    #[test]
    fn construction_admits_the_committed_input() {
        let admitted = construct(root(), &committed()).unwrap();
        assert_eq!(
            admitted.profile_specification.semantic_digest.as_str(),
            profile::SEMANTIC_DIGEST
        );
        assert_eq!(
            admitted.resolver_capability_specification.body,
            CapabilitySpecification::revision_zero()
        );
        assert_eq!(
            admitted.registry_package.checks,
            CheckRegistry::revision_zero()
        );
        assert_eq!(
            admitted.registry_package.findings,
            FindingRegistry::revision_zero()
        );
        assert!(implemented(&admitted));
    }

    /// Each component is checked with its own rows, in the precedence's
    /// order, and only the first failing row is reported.
    #[test]
    fn the_components_are_checked_in_the_precedence_order() {
        let none = ConstructionInput::default();
        assert_eq!(constructed(&none), Some(2));
        let without = |remove: fn(&mut ConstructionInput)| {
            let mut input = committed();
            remove(&mut input);
            constructed(&input)
        };
        assert_eq!(without(|input| input.profile_specification = None), Some(8));
        assert_eq!(
            without(|input| input.configuration_schema_authority_set = None),
            Some(15)
        );
        assert_eq!(
            without(|input| input.resolver_capability_specification = None),
            Some(25)
        );
        assert_eq!(without(|input| input.registry_package = None), Some(31));
        // No Profile Specification Schema, and a registry member whose digest
        // its container does not declare: only the first is reported.
        let mut both = committed();
        both.profile_specification_schema = None;
        let mut members = REGISTRY_MEMBERS.map(<[u8]>::to_vec);
        members[4].push(b' ');
        let members = members.each_ref().map(Vec::as_slice);
        both.registry_package = Some(package(PACKAGE, members));
        assert_eq!(constructed(&both), Some(2));
        both.profile_specification_schema = committed().profile_specification_schema;
        assert_eq!(constructed(&both), Some(39));
    }

    /// Each rule fails only through what the rows before it leave open, and
    /// refuses with row 42 although the change also leaves the authority
    /// unimplemented, which row 43 would refuse.
    #[test]
    fn each_cross_authority_rule_refuses_with_row_42() {
        // The set belongs to another Profile Specification.
        let mut association = committed();
        association.configuration_schema_authority_set = Some(authority_set(&set_bytes(|set| {
            set["profileSpecification"]["semanticDigest"] =
                json!(format!("sha256:{}", "0".repeat(64)));
        })));
        // The body names another Registry Package revision.
        let authorities = with_body(|body| {
            body["authorities"]["registryPackage"]["revision"] = json!("1");
        });
        // The bounds are not the capability's, in order.
        let mirror = with_body(|body| {
            body["resourceBounds"].as_array_mut().unwrap().swap(0, 1);
        });
        // The Check Registry uses a phase the body does not list.
        let phases = with_body(|body| {
            body["phases"]
                .as_array_mut()
                .unwrap()
                .retain(|phase| phase != "target-resolution");
        });
        for (rule, input) in
            CrossAuthorityRule::ALL
                .into_iter()
                .zip([&association, &authorities, &mirror, &phases])
        {
            assert_eq!(constructed(input), Some(42), "{rule:?}");
        }
        // The rows before rule 42 still come first.
        let mut earlier = association;
        earlier.resolver_capability_specification = None;
        assert_eq!(constructed(&earlier), Some(25));
    }

    /// A schema its package admitted may admit any body, so a member a rule
    /// reads may be missing or of another type: the rule fails, and nothing
    /// else does.
    #[test]
    fn a_rule_fails_on_a_body_its_schema_left_open() {
        let open = format!(r#"{{"$schema": "{DRAFT_7}"}}"#);
        let changes: [fn(&mut Value); 5] = [
            |body| *body = json!({}),
            |body| body["authorities"] = json!("none"),
            |body| body["resourceBounds"] = json!({}),
            |body| body["resourceBounds"][0] = json!(false),
            |body| body["phases"] = Value::Null,
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut input = with_body(change);
            input.profile_specification_schema = Some(submitted(
                profile::SCHEMA_IDENTITY,
                profile::REVISION,
                open.as_bytes(),
            ));
            assert_eq!(constructed(&input), Some(42), "change {index}");
        }
    }

    /// An authority with the implemented identity and revision but other
    /// bytes passes every row before the last and fails only there; a
    /// Profile Specification is compared by its semantic digest alone.
    #[test]
    fn an_authority_the_resolver_does_not_implement_fails_only_at_row_43() {
        let mut inputs = Vec::new();
        let mut schema = committed();
        schema.profile_specification_schema = Some(submitted(
            profile::SCHEMA_IDENTITY,
            profile::REVISION,
            &compact(&value(super::testing::PROFILE_SPECIFICATION_SCHEMA)),
        ));
        inputs.push(schema);
        // Another value the rules do not read.
        inputs.push(with_body(|body| {
            body["tokenDomains"]["subject"] = json!("intlify.project-profile.other");
        }));
        let mut set = committed();
        set.configuration_schema_authority_set =
            Some(authority_set(&compact(&value(AUTHORITY_SET))));
        inputs.push(set);
        // Another configuration schema, which the set declares.
        let member = compact(&value(CONFIGURATION_SCHEMA));
        let mut mapped = committed();
        let mut mapped_set = authority_set(&set_bytes(|set| {
            set["members"]["0"]["digest"] = json!(component::file_digest(&member).as_str());
        }));
        mapped_set
            .members
            .insert("0".to_owned(), Artifact::read(member));
        mapped.configuration_schema_authority_set = Some(mapped_set);
        inputs.push(mapped);
        let mut capability = committed();
        capability.resolver_capability_specification = Some(submitted(
            capability::IDENTITY,
            capability::REVISION,
            &compact(&value(super::testing::CAPABILITY)),
        ));
        inputs.push(capability);
        let mut container = committed();
        container.registry_package = Some(package(&compact(&value(PACKAGE)), REGISTRY_MEMBERS));
        inputs.push(container);
        // Each registry member in turn, which its container declares.
        for (index, role) in RegistryRole::ALL.into_iter().enumerate() {
            let mut members = REGISTRY_MEMBERS.map(<[u8]>::to_vec);
            members[index] = compact(&value(REGISTRY_MEMBERS[index]));
            let mut container = value(PACKAGE);
            container["members"][role.name()]["digest"] =
                json!(component::file_digest(&members[index]).as_str());
            let mut input = committed();
            input.registry_package = Some(package(
                &compact(&container),
                members.each_ref().map(Vec::as_slice),
            ));
            inputs.push(input);
        }
        assert_eq!(inputs.len(), 12);
        for (index, input) in inputs.iter().enumerate() {
            assert_eq!(constructed(input), Some(43), "input {index}");
        }
    }

    /// Each comparison stands on its own: some follow from others through
    /// the input, as a member's digest from its container's, so each is
    /// broken here on the admitted authorities themselves.
    #[test]
    fn each_compared_digest_must_be_the_one_compiled_in() {
        let other = || component::file_digest(b"other");
        let changes: [fn(&mut AdmittedAuthorities, String); 7] = [
            |admitted, other| admitted.profile_specification_schema.declared.digest = other,
            |admitted, other| admitted.configuration_schema_authority_set.declared.digest = other,
            |admitted, other| {
                admitted
                    .configuration_schema_authority_set
                    .members
                    .get_mut(CONFIGURATION_SCHEMA_VERSION)
                    .unwrap()
                    .member
                    .digest = other;
            },
            |admitted, _| admitted.configuration_schema_authority_set.members.clear(),
            |admitted, other| admitted.resolver_capability_specification.declared.digest = other,
            |admitted, other| admitted.registry_package.declared.digest = other,
            |admitted, other| {
                admitted.profile_specification.semantic_digest =
                    component::file_digest(other.as_bytes());
            },
        ];
        for (index, change) in changes.into_iter().enumerate() {
            let mut admitted = construct(root(), &committed()).unwrap();
            change(&mut admitted, other().as_str().to_owned());
            assert!(!implemented(&admitted), "change {index}");
        }
        for index in 0..RegistryRole::ALL.len() {
            let mut admitted = construct(root(), &committed()).unwrap();
            admitted.registry_package.members[index].digest = other().as_str().to_owned();
            assert!(!implemented(&admitted), "member {index}");
        }
    }

    /// The committed input, given `bytes` for a `schemaVersion` the set does
    /// not map: the construction-root bounds count them, and no later row
    /// reads them.
    fn with_unmapped(bytes: Vec<u8>) -> ConstructionInput {
        let mut input = committed();
        input
            .configuration_schema_authority_set
            .as_mut()
            .unwrap()
            .members
            .insert("1".to_owned(), Artifact::read(bytes));
        input
    }

    fn bound(input: &ConstructionInput) -> Option<(usize, Option<ConstructionBoundId>)> {
        construct(root(), input)
            .err()
            .map(|refusal| (refusal.number(), refusal.bound()))
    }

    /// The construction-root bounds come before every other row, presence
    /// included, and a refusal there names its bound.
    #[test]
    fn the_bounds_come_before_every_other_row() {
        let mut input = with_unmapped(vec![b' '; 4 * 1024 * 1024 + 1]);
        input.profile_specification_schema = None;
        assert_eq!(
            bound(&input),
            Some((1, Some(ConstructionBoundId::InputMaxSingleArtifactBytes)))
        );
        input.configuration_schema_authority_set = None;
        assert_eq!(bound(&input), Some((2, None)));
    }

    /// An exact total is admitted and one more is refused, end to end.
    #[test]
    fn an_exact_total_constructs_and_one_more_is_refused() {
        let nested = |levels: usize| format!("{}{}", "[".repeat(levels), "]".repeat(levels));
        assert_eq!(bound(&with_unmapped(nested(64).into_bytes())), None);
        assert_eq!(
            bound(&with_unmapped(nested(65).into_bytes())),
            Some((1, Some(ConstructionBoundId::InputMaxDepth)))
        );
        // References up to the bound, in the subschemas of one schema.
        let committed_references = bounds::totals(root(), &committed())
            .get(ConstructionBoundId::SchemaMaxReferences)
            .unwrap();
        let references = |count: u64| {
            let item = r##"{"$ref": "#"}"##;
            let items = vec![item; usize::try_from(count).unwrap()].join(",");
            format!(r#"{{"items": [{items}]}}"#).into_bytes()
        };
        let exact = 65_536 - committed_references;
        assert_eq!(bound(&with_unmapped(references(exact))), None);
        assert_eq!(
            bound(&with_unmapped(references(exact + 1))),
            Some((1, Some(ConstructionBoundId::SchemaMaxReferences)))
        );
        // Mappings up to the bound: the exact count passes row 1 and is
        // refused later, here for want of the mapped schemas' bytes.
        let mappings = |count: usize| {
            let mut set = value(AUTHORITY_SET);
            let member = set["members"]["0"].clone();
            for version in 1..count {
                set["members"][version.to_string()] = member.clone();
            }
            authority_set(&compact(&set))
        };
        let mut exact = committed();
        exact.configuration_schema_authority_set = Some(mappings(64));
        assert!(bound(&exact).is_some_and(|(row, bound)| row > 1 && bound.is_none()));
        let mut over = committed();
        over.configuration_schema_authority_set = Some(mappings(65));
        assert_eq!(
            bound(&over),
            Some((1, Some(ConstructionBoundId::SchemaMaxConfigurationSchemas)))
        );
    }

    /// Of several exceeded bounds, the first in the canonical order is named,
    /// whichever artifact exceeds it.
    #[test]
    fn of_several_exceeded_bounds_the_first_is_named() {
        let long = format!(r#"["{}"]"#, "a".repeat(65_537));
        let mut input = with_unmapped(long.into_bytes());
        assert_eq!(
            bound(&input),
            Some((1, Some(ConstructionBoundId::InputMaxSingleStringBytes)))
        );
        input.registry_package.as_mut().unwrap().members.insert(
            RegistryRole::CheckRegistry,
            Artifact::read(format!("{}{}", "[".repeat(65), "]".repeat(65)).into_bytes()),
        );
        assert_eq!(
            bound(&input),
            Some((1, Some(ConstructionBoundId::InputMaxDepth)))
        );
    }

    #[test]
    fn a_byte_different_profile_specification_with_an_equal_semantic_digest_constructs() {
        let bytes = compact(&value(PROFILE_SPECIFICATION));
        assert_ne!(bytes, PROFILE_SPECIFICATION);
        let mut input = committed();
        input.profile_specification = Some(profile_specification(&bytes));
        let admitted = construct(root(), &input).unwrap();
        assert_ne!(
            admitted.profile_specification.declared.digest,
            component::file_digest(PROFILE_SPECIFICATION).as_str()
        );
        assert_eq!(
            admitted.profile_specification.semantic_digest.as_str(),
            profile::SEMANTIC_DIGEST
        );
    }
}
