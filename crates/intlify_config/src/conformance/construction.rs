// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Running a Resolver Construction Case: the input its fixture materializes,
//! and what construction does with it, in the form of the case's expected
//! outcome (D15:3035-3105).
//!
//! Construction runs under the built-in Resolver Construction Root Package,
//! the Suite Index's (D15:3035). It succeeds with the identity derived from
//! what it admitted, or refuses with the reason of the first precedence row
//! that fails, the construction-root bounds' row also naming its bound.
//! Construction never invokes the resolver, so neither outcome has anything
//! of a resolution.

use serde_json::{json, Map, Value};

use crate::construction::authority_set::SubmittedSet;
use crate::construction::component::{Artifact, Submitted};
use crate::construction::flow::{construct, ConstructionInput};
use crate::construction::identity;
use crate::construction::registry::{RegistryRole, SubmittedPackage};
use crate::construction::root::ConstructionRoot;

use super::fixture::{bytes, Bodies};

/// The construction input a fixture of the Case Schema's form materializes,
/// with the bodies it references: each component present as its declared
/// tuple and its artifact, the containers with their members.
pub(crate) fn input(fixture: &Value, bodies: &Bodies) -> ConstructionInput {
    let submitted = |name: &str| {
        let component = fixture.get(name)?;
        Some(Submitted {
            tuple: component["tuple"]["json"].clone(),
            artifact: Artifact::read(bytes(&component["bytes"], bodies)),
        })
    };
    let members = |name: &str| {
        fixture[name]["members"]
            .as_object()
            .into_iter()
            .flatten()
            .map(|(key, raw)| (key.as_str(), Artifact::read(bytes(raw, bodies))))
            .collect::<Vec<_>>()
    };
    let set = "configurationSchemaAuthoritySet";
    let package = "registryPackage";
    ConstructionInput {
        profile_specification_schema: submitted("profileSpecificationSchema"),
        profile_specification: submitted("profileSpecification"),
        configuration_schema_authority_set: submitted(set).map(|component| SubmittedSet {
            component,
            members: members(set)
                .into_iter()
                .map(|(version, artifact)| (version.to_owned(), artifact))
                .collect(),
        }),
        resolver_capability_specification: submitted("resolverCapabilitySpecification"),
        registry_package: submitted(package).map(|component| SubmittedPackage {
            component,
            members: members(package)
                .into_iter()
                .map(|(name, artifact)| (role(name), artifact))
                .collect(),
        }),
    }
}

/// The role a Registry Package member's key names, which the Case Schema
/// closes to the six.
fn role(name: &str) -> RegistryRole {
    RegistryRole::ALL
        .into_iter()
        .find(|role| role.name() == name)
        .expect("the Case Schema closes the roles")
}

/// What construction does with `input`, in the form of a construction case's
/// expected outcome.
pub(crate) fn outcome(input: &ConstructionInput) -> Value {
    let root = ConstructionRoot::built_in().expect("the built-in construction root verifies");
    let mut outcome = Map::new();
    match construct(root, input) {
        Ok(admitted) => {
            outcome.insert("constructed".into(), json!(true));
            let identity = identity::derive(root, &admitted).text();
            outcome.insert("resolverConstructionIdentity".into(), json!(identity));
        }
        Err(refusal) => {
            outcome.insert("constructed".into(), json!(false));
            outcome.insert("resolverConstructionIdentity".into(), Value::Null);
            outcome.insert("failureReason".into(), json!(refusal.reason()));
            if let Some(bound) = refusal.bound() {
                outcome.insert("failureDetails".into(), json!({ "boundId": bound }));
            }
        }
    }
    outcome.insert("resolverInvoked".into(), json!(false));
    Value::Object(outcome)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::component::file_digest;
    use crate::construction::flow::testing::committed;
    use crate::resolver::evidence_source::hex;

    /// A fixture of the committed input, its bodies in hexadecimal.
    fn committed_fixture() -> Value {
        let input = committed();
        let component = |submitted: &Submitted| json!({ "tuple": { "json": submitted.tuple }, "bytes": { "bytes": hex(submitted.artifact.bytes()) } });
        let set = input.configuration_schema_authority_set.unwrap();
        let package = input.registry_package.unwrap();
        let mut set_value = component(&set.component);
        set_value["members"] = set
            .members
            .iter()
            .map(|(version, artifact)| (version.clone(), json!({ "bytes": hex(artifact.bytes()) })))
            .collect::<Map<_, _>>()
            .into();
        let mut package_value = component(&package.component);
        package_value["members"] = package
            .members
            .iter()
            .map(|(role, artifact)| {
                (
                    role.name().to_owned(),
                    json!({ "bytes": hex(artifact.bytes()) }),
                )
            })
            .collect::<Map<_, _>>()
            .into();
        json!({
            "profileSpecificationSchema": component(&input.profile_specification_schema.unwrap()),
            "profileSpecification": component(&input.profile_specification.unwrap()),
            "configurationSchemaAuthoritySet": set_value,
            "resolverCapabilitySpecification": component(&input.resolver_capability_specification.unwrap()),
            "registryPackage": package_value,
        })
    }

    /// A fixture materializes the input it holds, and a reference reads the
    /// body the runner verified.
    #[test]
    fn a_fixture_materializes_the_input_it_holds() {
        let fixture = committed_fixture();
        assert_eq!(input(&fixture, &Bodies::new()), committed());
        let mut referenced = fixture.clone();
        let schema = committed().profile_specification_schema.unwrap();
        let bytes = schema.artifact.bytes().to_vec();
        referenced["profileSpecificationSchema"]["bytes"] =
            json!({ "path": "a.json", "digest": file_digest(&bytes).as_str() });
        let bodies = Bodies::from([("a.json".to_owned(), bytes)]);
        assert_eq!(input(&referenced, &bodies), committed());
    }

    /// An omitted component or member is absent.
    #[test]
    fn an_omitted_component_or_member_is_absent() {
        let mut fixture = committed_fixture();
        fixture
            .as_object_mut()
            .unwrap()
            .remove("profileSpecification");
        fixture["registryPackage"]["members"]
            .as_object_mut()
            .unwrap()
            .remove("finding-registry");
        let materialized = input(&fixture, &Bodies::new());
        assert!(materialized.profile_specification.is_none());
        let package = materialized.registry_package.unwrap();
        assert_eq!(package.members.len(), RegistryRole::ALL.len() - 1);
        assert!(!package.members.contains_key(&RegistryRole::FindingRegistry));
        let none = input(&json!({}), &Bodies::new());
        assert_eq!(none, ConstructionInput::default());
    }

    /// Success is the derived identity; a refusal is its row's reason, the
    /// bounds' row with its bound; neither invokes the resolver.
    #[test]
    fn an_outcome_has_the_expected_outcome_s_form() {
        let constructed = outcome(&committed());
        let root = ConstructionRoot::built_in().unwrap();
        let admitted = construct(root, &committed()).unwrap();
        assert_eq!(
            constructed,
            json!({
                "constructed": true,
                "resolverConstructionIdentity": identity::derive(root, &admitted).text(),
                "resolverInvoked": false,
            })
        );
        let mut missing = committed();
        missing.registry_package = None;
        assert_eq!(
            outcome(&missing),
            json!({
                "constructed": false,
                "resolverConstructionIdentity": null,
                "failureReason": "registry-package-incomplete",
                "resolverInvoked": false,
            })
        );
        let mut deep = committed();
        let nested = format!("{}{}", "[".repeat(65), "]".repeat(65)).into_bytes();
        deep.profile_specification_schema.as_mut().unwrap().artifact = Artifact::read(nested);
        assert_eq!(
            outcome(&deep),
            json!({
                "constructed": false,
                "resolverConstructionIdentity": null,
                "failureReason": "construction-input-limit-exceeded",
                "failureDetails": { "boundId": "input.maxDepth" },
                "resolverInvoked": false,
            })
        );
    }
}
