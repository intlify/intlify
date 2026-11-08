// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Resolver construction recipes: the committed input, inputs that construct
//! the same resolver from other bytes, inputs refused at one row each, and
//! the committed input grown to a construction-root bound or one past it.
//! Every input is assembled from the committed artifacts before any interval;
//! nothing is read from a file.

use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::construction::admission::ConstructionBoundId as B;
use crate::construction::bounds;
use crate::construction::capability;
use crate::construction::committed::{
    self, authority_set, declared, package, profile_specification, AUTHORITY_SET, CAPABILITY,
    PACKAGE, PROFILE_SPECIFICATION, REGISTRY_MEMBERS,
};
use crate::construction::component::{self, Artifact, Submitted};
use crate::construction::flow::ConstructionInput;
use crate::construction::root::ConstructionRoot;

use super::{Declaration, LimitEdge, LimitKind, Recipe};

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

/// Each construction bound the committed input can be grown to, with whether
/// one past it is reachable too, in the bounds' canonical order (plan 025,
/// #134). A first-over input names its bound only if no bound before it in
/// that order is exceeded. Not here: the collection entries (every entry is a
/// node, so the nodes exceed their bound first), the total string bytes
/// (decoded strings never outgrow the artifacts' bytes, which exceed theirs
/// first), the registries' total bytes (three bodies of at most the input's
/// single-artifact bound), one past the single registry member's bytes (the
/// input's single-artifact bound comes first), and the two validation-work
/// bounds, which PR 6c-2b adds.
pub(in crate::benchmark) const EDGES: [(LimitKind, bool); 13] = [
    (LimitKind::ConstructionTotalArtifactBytes, true),
    (LimitKind::ConstructionSingleArtifactBytes, true),
    (LimitKind::ConstructionDepth, true),
    (LimitKind::ConstructionNodes, true),
    (LimitKind::ConstructionSingleStringBytes, true),
    (LimitKind::ConstructionConfigurationSchemas, true),
    (LimitKind::ConstructionSchemaSingleArtifactBytes, true),
    (LimitKind::ConstructionSchemaTotalArtifactBytes, true),
    (LimitKind::ConstructionSchemaNodes, true),
    (LimitKind::ConstructionSchemaReferences, true),
    (LimitKind::ConstructionRegistryRows, true),
    (
        LimitKind::ConstructionRegistrySingleMemberArtifactBytes,
        false,
    ),
    (LimitKind::ConstructionCrossAuthorityChecks, true),
];

/// The construction bound a limit kind grows the input to.
pub(in crate::benchmark) const fn bound(kind: LimitKind) -> Option<B> {
    Some(match kind {
        LimitKind::ConstructionTotalArtifactBytes => B::InputMaxTotalArtifactBytes,
        LimitKind::ConstructionSingleArtifactBytes => B::InputMaxSingleArtifactBytes,
        LimitKind::ConstructionDepth => B::InputMaxDepth,
        LimitKind::ConstructionNodes => B::InputMaxNodes,
        LimitKind::ConstructionSingleStringBytes => B::InputMaxSingleStringBytes,
        LimitKind::ConstructionConfigurationSchemas => B::SchemaMaxConfigurationSchemas,
        LimitKind::ConstructionSchemaSingleArtifactBytes => B::SchemaMaxSingleArtifactBytes,
        LimitKind::ConstructionSchemaTotalArtifactBytes => B::SchemaMaxTotalArtifactBytes,
        LimitKind::ConstructionSchemaNodes => B::SchemaMaxNodes,
        LimitKind::ConstructionSchemaReferences => B::SchemaMaxReferences,
        LimitKind::ConstructionRegistryRows => B::RegistryMaxRows,
        LimitKind::ConstructionRegistrySingleMemberArtifactBytes => {
            B::RegistryMaxSingleMemberArtifactBytes
        }
        LimitKind::ConstructionCrossAuthorityChecks => B::CrossAuthorityMaxChecks,
        _ => return None,
    })
}

/// The row that refuses an edge's input: the bounds' row one past a bound,
/// and at a bound the row the grown input reaches after passing it, `None`
/// for one that constructs. Owner-declared, not learned from a construction.
pub(in crate::benchmark) const fn edge_refused_at(
    kind: LimitKind,
    edge: LimitEdge,
) -> Option<usize> {
    match (edge, kind) {
        (LimitEdge::FirstOver, _) => Some(1),
        // The padded authorities are not the ones implemented.
        (LimitEdge::Exact, LimitKind::ConstructionTotalArtifactBytes) => Some(43),
        // The container maps schemas it is given no bytes for: the set's
        // envelope row.
        (LimitEdge::Exact, LimitKind::ConstructionConfigurationSchemas) => Some(16),
        // A grown registry is no member its container declares.
        (
            LimitEdge::Exact,
            LimitKind::ConstructionRegistryRows
            | LimitKind::ConstructionRegistrySingleMemberArtifactBytes,
        ) => Some(39),
        // The grown bound paths leave the body no semantic digest for its
        // tuple to declare: the Profile Specification's envelope row.
        (LimitEdge::Exact, LimitKind::ConstructionCrossAuthorityChecks) => Some(9),
        (LimitEdge::Exact, _) => None,
    }
}

/// The input a construction case is prepared from, and the row that refuses
/// it: its recipe's, or, at a bound's edge, the committed input grown to it.
/// `None` for a declaration that is no construction case.
pub(in crate::benchmark) fn case_input(
    declaration: &Declaration,
) -> Option<(ConstructionInput, Option<usize>)> {
    let Recipe::Construction(recipe) = declaration.fixture else {
        return None;
    };
    match declaration.limit {
        None => Some((recipe.input(), recipe.refused_at())),
        Some((kind, edge)) if recipe == ConstructionRecipe::Committed => {
            Some((edge_input(kind, edge)?, edge_refused_at(kind, edge)))
        }
        Some(_) => None,
    }
}

const MEBIBYTE: u64 = 1024 * 1024;

/// The committed input grown to `kind`'s bound, or one past it, built once:
/// the inputs run to megabytes, and each preparation of a case and of its
/// input context takes its own copy.
pub(in crate::benchmark) fn edge_input(
    kind: LimitKind,
    edge: LimitEdge,
) -> Option<ConstructionInput> {
    static BUILT: [OnceLock<Option<ConstructionInput>>; 2 * EDGES.len()] =
        [const { OnceLock::new() }; 2 * EDGES.len()];
    let index = EDGES.iter().position(|&(edges, _)| edges == kind)?;
    BUILT[2 * index + usize::from(edge == LimitEdge::FirstOver)]
        .get_or_init(|| grown(kind, edge))
        .clone()
}

/// The committed input grown to `kind`'s bound, or one past it: each grows
/// one element whose every unit adds one to the bound's total, measured on
/// the input first.
fn grown(kind: LimitKind, edge: LimitEdge) -> Option<ConstructionInput> {
    let root = ConstructionRoot::built_in().ok()?;
    let bound = bound(kind)?;
    let target = bound.value().get() + u64::from(edge == LimitEdge::FirstOver);
    let input = match (kind, edge) {
        // Four artifacts the input's single-artifact bound admits: three at
        // it, and the Registry Package's container with the rest. Spaces add
        // bytes and nothing else, so the rest is counted from the committed
        // total.
        (LimitKind::ConstructionTotalArtifactBytes, LimitEdge::Exact) => {
            let single = 4 * MEBIBYTE;
            let grown = [PROFILE_SPECIFICATION, CAPABILITY, AUTHORITY_SET]
                .map(|bytes| single - length(bytes))
                .into_iter()
                .sum::<u64>();
            let rest = target.checked_sub(committed_total(root, bound)? + grown)?;
            let mut input = committed::input();
            input.profile_specification = Some(profile_specification(&padded(
                PROFILE_SPECIFICATION,
                single,
            )));
            input.resolver_capability_specification = Some(declared(
                capability::IDENTITY,
                capability::REVISION,
                &padded(CAPABILITY, single),
            ));
            input.configuration_schema_authority_set =
                Some(authority_set(&padded(AUTHORITY_SET, single)));
            input.registry_package = Some(package(
                &padded(PACKAGE, length(PACKAGE) + rest),
                REGISTRY_MEMBERS,
            ));
            Some(input)
        }
        // The Profile Specification, compared by its semantic digest, padded.
        (LimitKind::ConstructionTotalArtifactBytes, LimitEdge::FirstOver) => {
            let rest = target.checked_sub(committed_total(root, bound)?)?;
            let mut input = committed::input();
            input.profile_specification = Some(profile_specification(&padded(
                PROFILE_SPECIFICATION,
                length(PROFILE_SPECIFICATION) + rest,
            )));
            Some(input)
        }
        (LimitKind::ConstructionSingleArtifactBytes, _) => {
            let mut input = committed::input();
            input.profile_specification = Some(profile_specification(&padded(
                PROFILE_SPECIFICATION,
                target,
            )));
            Some(input)
        }
        (LimitKind::ConstructionDepth, _) => Some(unmapped(&[nested(target)])),
        // Two arrays of zeros, each a schema artifact the schema's
        // single-artifact bound admits.
        (LimitKind::ConstructionNodes, _) => fit(root, bound, target, 0, |extra| {
            let first = extra.min(MEBIBYTE / 2 - 1);
            unmapped(&[zeros(first), zeros(extra - first)])
        }),
        (LimitKind::ConstructionSingleStringBytes, _) => Some(unmapped(&[encoded(&json!(
            "a".repeat(usize::try_from(target).ok()?)
        ))])),
        // The container maps more schemas, under the versions after "0".
        (LimitKind::ConstructionConfigurationSchemas, _) => {
            let mut set = value(AUTHORITY_SET);
            let member = set["members"]["0"].clone();
            for version in 1..target {
                set["members"][version.to_string()] = member.clone();
            }
            let mut input = committed::input();
            input.configuration_schema_authority_set = Some(authority_set(&encoded(&set)));
            Some(input)
        }
        (LimitKind::ConstructionSchemaSingleArtifactBytes, _) => {
            Some(unmapped(&[padded(b"{}", target)]))
        }
        // Schemas the schema's single-artifact bound admits, as many as the
        // rest takes.
        (LimitKind::ConstructionSchemaTotalArtifactBytes, _) => {
            fit(root, bound, target, 0, |extra| {
                let mut schemas = Vec::new();
                let mut rest = extra;
                while rest > 0 {
                    let size = rest.min(MEBIBYTE);
                    schemas.push(padded(b"0", size));
                    rest -= size;
                }
                unmapped(&schemas)
            })
        }
        // `anyOf` subschemas, one schema node each.
        (LimitKind::ConstructionSchemaNodes, _) => fit(root, bound, target, 1, |items| {
            unmapped(&[any_of(items, "{}")])
        }),
        // `$ref` subschemas, one reference each.
        (LimitKind::ConstructionSchemaReferences, _) => fit(root, bound, target, 1, |items| {
            unmapped(&[any_of(items, r##"{"$ref":"#"}"##)])
        }),
        // More rows in the Safe Origin Kind Registry.
        (LimitKind::ConstructionRegistryRows, _) => fit(root, bound, target, 0, |extra| {
            let mut registry = value(REGISTRY_MEMBERS[3]);
            if let Some(rows) = registry["rows"].as_array_mut() {
                rows.extend((0..extra).map(|_| json!(0)));
            }
            let mut members = REGISTRY_MEMBERS.map(<[u8]>::to_vec);
            members[3] = encoded(&registry);
            let mut input = committed::input();
            input.registry_package = Some(package(PACKAGE, members.each_ref().map(Vec::as_slice)));
            input
        }),
        // The Check Registry, padded.
        (LimitKind::ConstructionRegistrySingleMemberArtifactBytes, LimitEdge::Exact) => {
            let mut members = REGISTRY_MEMBERS.map(<[u8]>::to_vec);
            members[4] = padded(REGISTRY_MEMBERS[4], target);
            let mut input = committed::input();
            input.registry_package = Some(package(PACKAGE, members.each_ref().map(Vec::as_slice)));
            Some(input)
        }
        // More bound paths in the Profile Specification than the Capability
        // Specification compares.
        (LimitKind::ConstructionCrossAuthorityChecks, _) => fit(root, bound, target, 64, |paths| {
            let mut body = value(PROFILE_SPECIFICATION);
            body["resourceBounds"] = (0..paths).map(|_| json!(0)).collect();
            let mut input = committed::input();
            input.profile_specification = Some(profile_specification(&encoded(&body)));
            input
        }),
        _ => None,
    }?;
    (bounds::totals(root, &input).get(bound) == Some(target)).then_some(input)
}

/// `bound`'s total over the committed input.
fn committed_total(root: &ConstructionRoot, bound: B) -> Option<u64> {
    bounds::totals(root, &committed::input()).get(bound)
}

/// The input `grown` builds for the size at which `bound`'s total is
/// `target`, when each unit of size adds one to it from `start` on.
fn fit(
    root: &ConstructionRoot,
    bound: B,
    target: u64,
    start: u64,
    grown: impl Fn(u64) -> ConstructionInput,
) -> Option<ConstructionInput> {
    let total = bounds::totals(root, &grown(start)).get(bound)?;
    Some(grown(start.checked_add(target.checked_sub(total)?)?))
}

/// The committed input with these schemas, which the set does not map, under
/// the versions after "0".
fn unmapped(schemas: &[Vec<u8>]) -> ConstructionInput {
    let mut input = committed::input();
    if let Some(set) = input.configuration_schema_authority_set.as_mut() {
        for (index, schema) in schemas.iter().enumerate() {
            set.members
                .insert((index + 1).to_string(), Artifact::read(schema.clone()));
        }
    }
    input
}

fn length(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap_or(u64::MAX)
}

/// `bytes`, then spaces up to `size` bytes.
fn padded(bytes: &[u8], size: u64) -> Vec<u8> {
    let mut padded = bytes.to_vec();
    padded.resize(
        usize::try_from(size).unwrap_or(usize::MAX).max(bytes.len()),
        b' ',
    );
    padded
}

/// An array nested `depth` levels deep.
fn nested(depth: u64) -> Vec<u8> {
    let depth = usize::try_from(depth).unwrap_or(0);
    [b"[".repeat(depth), b"]".repeat(depth)].concat()
}

/// An array of `count` zeros.
fn zeros(count: u64) -> Vec<u8> {
    encoded(&(0..count).map(|_| json!(0)).collect())
}

/// A schema of `count` `anyOf` subschemas, each `item`.
fn any_of(count: u64, item: &str) -> Vec<u8> {
    let items = vec![item; usize::try_from(count).unwrap_or(0)].join(",");
    format!(r#"{{"anyOf":[{items}]}}"#).into_bytes()
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
