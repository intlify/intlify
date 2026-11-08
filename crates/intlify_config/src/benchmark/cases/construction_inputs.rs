// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Resolver construction recipes: the committed input, inputs that construct
//! the same resolver from other bytes, and inputs refused at one row each.
//! Every input is assembled from the committed artifacts before any interval;
//! nothing is read from a file.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::construction::capability;
use crate::construction::committed::{
    self, authority_set, declared, package, profile_specification, AUTHORITY_SET, CAPABILITY,
    PACKAGE, PROFILE_SPECIFICATION, REGISTRY_MEMBERS,
};
use crate::construction::component::{self, Artifact, Submitted};
use crate::construction::flow::ConstructionInput;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum ConstructionRecipe {
    /// The committed input.
    Committed,
    /// The Profile Specification written with other bytes: one construction
    /// (D15:2583).
    ProfileSpecificationLayout,
    /// One more schema, which the set does not map and no row reads.
    UnmappedSchema,
    /// No Profile Specification Schema: row 2.
    MissingProfileSpecificationSchema,
    /// No Profile Specification: row 8.
    MissingProfileSpecification,
    /// No Configuration Schema Authority Set: row 15.
    MissingAuthoritySet,
    /// No Resolver Capability Specification: row 25.
    MissingCapability,
    /// No Registry Package: row 31.
    MissingRegistryPackage,
    /// A registry member whose digest its container does not declare: row 39.
    UndeclaredRegistryMember,
    /// A set associated with another Profile Specification: row 42.
    ForeignAssociation,
    /// A Capability Specification with other bytes than the one implemented:
    /// row 43.
    UnimplementedCapability,
}

impl ConstructionRecipe {
    pub(in crate::benchmark) const ALL: [Self; 11] = [
        Self::Committed,
        Self::ProfileSpecificationLayout,
        Self::UnmappedSchema,
        Self::MissingProfileSpecificationSchema,
        Self::MissingProfileSpecification,
        Self::MissingAuthoritySet,
        Self::MissingCapability,
        Self::MissingRegistryPackage,
        Self::UndeclaredRegistryMember,
        Self::ForeignAssociation,
        Self::UnimplementedCapability,
    ];

    /// The row of design 015's construction-failure precedence that refuses
    /// this recipe's input; `None` for one that constructs. Owner-declared,
    /// not learned from a construction.
    pub(in crate::benchmark) const fn refused_at(self) -> Option<usize> {
        match self {
            Self::Committed | Self::ProfileSpecificationLayout | Self::UnmappedSchema => None,
            Self::MissingProfileSpecificationSchema => Some(2),
            Self::MissingProfileSpecification => Some(8),
            Self::MissingAuthoritySet => Some(15),
            Self::MissingCapability => Some(25),
            Self::MissingRegistryPackage => Some(31),
            Self::UndeclaredRegistryMember => Some(39),
            Self::ForeignAssociation => Some(42),
            Self::UnimplementedCapability => Some(43),
        }
    }

    pub(in crate::benchmark) const fn constructs(self) -> bool {
        self.refused_at().is_none()
    }

    /// The recipe's construction input.
    pub(in crate::benchmark) fn input(self) -> ConstructionInput {
        let mut input = committed::input();
        match self {
            Self::Committed => {}
            Self::ProfileSpecificationLayout => {
                input.profile_specification =
                    Some(profile_specification(&compact(PROFILE_SPECIFICATION)));
            }
            Self::UnmappedSchema => {
                if let Some(set) = input.configuration_schema_authority_set.as_mut() {
                    set.members
                        .insert("1".to_owned(), Artifact::read(b"{}".to_vec()));
                }
            }
            Self::MissingProfileSpecificationSchema => input.profile_specification_schema = None,
            Self::MissingProfileSpecification => input.profile_specification = None,
            Self::MissingAuthoritySet => input.configuration_schema_authority_set = None,
            Self::MissingCapability => input.resolver_capability_specification = None,
            Self::MissingRegistryPackage => input.registry_package = None,
            Self::UndeclaredRegistryMember => {
                let mut members = REGISTRY_MEMBERS.map(<[u8]>::to_vec);
                members[4].push(b' ');
                input.registry_package =
                    Some(package(PACKAGE, members.each_ref().map(Vec::as_slice)));
            }
            Self::ForeignAssociation => {
                let mut set = value(AUTHORITY_SET);
                set["profileSpecification"]["semanticDigest"] =
                    json!(format!("sha256:{}", "0".repeat(64)));
                input.configuration_schema_authority_set = Some(authority_set(&encoded(&set)));
            }
            Self::UnimplementedCapability => {
                input.resolver_capability_specification = Some(declared(
                    capability::IDENTITY,
                    capability::REVISION,
                    &compact(CAPABILITY),
                ));
            }
        }
        input
    }

    /// What the recipe's input declares: each component's tuple, and the
    /// schemas and members it gives. The input context binds the bytes.
    pub(in crate::benchmark) fn source(self) -> Value {
        let input = self.input();
        json!({
            "profileSpecificationSchema": tuple(input.profile_specification_schema.as_ref()),
            "profileSpecification": tuple(input.profile_specification.as_ref()),
            "configurationSchemaAuthoritySet": input.configuration_schema_authority_set.as_ref().map(|set| json!({
                "tuple": set.component.tuple,
                "schemas": set.members.keys().collect::<Vec<_>>(),
            })),
            "resolverCapabilitySpecification": tuple(input.resolver_capability_specification.as_ref()),
            "registryPackage": input.registry_package.as_ref().map(|package| json!({
                "tuple": package.component.tuple,
                "members": package.members.keys().map(|role| role.name()).collect::<Vec<_>>(),
            })),
        })
    }
}

/// A component's declared tuple, when it is given.
fn tuple(component: Option<&Submitted>) -> Option<&Value> {
    component.map(|component| &component.tuple)
}

/// A committed artifact's value.
fn value(bytes: &[u8]) -> Value {
    component::strict_json(bytes).expect("a committed artifact is strict JSON")
}

/// A value's bytes, written without the committed artifacts' layout.
fn encoded(value: &Value) -> Vec<u8> {
    serde_json::to_vec(value).expect("a JSON value encodes")
}

/// A committed artifact's value, written with other bytes.
fn compact(bytes: &[u8]) -> Vec<u8> {
    encoded(&value(bytes))
}
