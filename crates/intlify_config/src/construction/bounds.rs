// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's construction-root bounds: the first row of the
//! construction-failure precedence.
//!
//! Construction computes every bound's total over the complete input before
//! any other check, so each total is known before the validation it bounds,
//! and refuses with the first bound, in the canonical order, whose total
//! exceeds the value the construction-admission body gives it; an exact total
//! is admitted (D15:955). The `input` bounds cover every submitted artifact,
//! the `schema` bounds every schema artifact and every validation but a
//! registry's, and the `registry` bounds the three registries and their
//! validations. An artifact that is not strict UTF-8 JSON counts only its
//! bytes, and a validation whose schema or instance cannot be decoded counts
//! no pairs.
//!
//! What 015 leaves open is read as plan 025 records it: a bound without
//! `Single` in its name is a total over the input; a declared tuple is not an
//! artifact, so it counts only as the instance of its envelope schema's
//! validation; bytes supplied for a `schemaVersion` the set does not map are a
//! submitted schema artifact, which no validation reads; and one
//! cross-authority check is one rule applied to one item it compares.
//! Totals use checked unsigned arithmetic: a total past `u64` exceeds every
//! bound (D15:1991).

use serde_json::Value;

use super::admission::{BoundRecord, ConstructionBoundId, EnvelopeSchema};
use super::authority_set::SubmittedSet;
use super::component::{Artifact, Refusal, Submitted};
use super::flow::ConstructionInput;
use super::registry::{RegistryRole, SubmittedPackage};
use super::root::ConstructionRoot;
use super::validator::schema::census;
use super::validator::validation::instance_nodes;
use crate::input_limits::Bound;

use ConstructionBoundId as B;

/// Every bound's total, in the canonical order. `None` is a total past
/// `u64`, which exceeds every bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Totals([Option<u64>; 18]);

impl Default for Totals {
    fn default() -> Self {
        Self([Some(0); 18])
    }
}

impl Totals {
    /// One bound's total.
    pub(crate) const fn get(&self, bound: ConstructionBoundId) -> Option<u64> {
        self.0[bound as usize]
    }

    fn add(&mut self, bound: ConstructionBoundId, count: Option<u64>) {
        let total = &mut self.0[bound as usize];
        *total = total
            .zip(count)
            .and_then(|(total, count)| total.checked_add(count));
    }

    fn max(&mut self, bound: ConstructionBoundId, count: u64) {
        let total = &mut self.0[bound as usize];
        *total = total.map(|total| total.max(count));
    }
}

/// A count of a length, which fits in 64 bits.
fn count(length: usize) -> u64 {
    u64::try_from(length).expect("an addressable length fits in u64")
}

/// The pairs of one validation: the product of the schema's nodes and the
/// instance's, or none when either cannot be decoded.
fn pairs(schema: Option<u64>, instance: Option<u64>) -> Option<u64> {
    match schema.zip(instance) {
        Some((schema, instance)) => schema.checked_mul(instance),
        None => Some(0),
    }
}

/// The length of an array member of a decoded value, or zero.
fn length(value: Option<&Value>, member: &str) -> u64 {
    value
        .and_then(|value| value.get(member))
        .and_then(Value::as_array)
        .map_or(0, |items| count(items.len()))
}

/// A component's artifact.
fn artifact(component: Option<&Submitted>) -> Option<&Artifact> {
    component.map(|component| &component.artifact)
}

/// An artifact's nodes, when it is strict UTF-8 JSON.
fn nodes(artifact: Option<&Artifact>) -> Option<u64> {
    artifact
        .and_then(Artifact::counts)
        .map(|counts| counts.nodes)
}

/// A schema artifact's schema nodes, when the shared decoder reads it.
fn schema_nodes(artifact: Option<&Artifact>) -> Option<u64> {
    artifact
        .and_then(Artifact::value)
        .map(|document| census(document).nodes)
}

/// The artifacts of an input, by where construction is given them.
struct Parts<'a> {
    input: &'a ConstructionInput,
    set: Option<&'a SubmittedSet>,
    package: Option<&'a SubmittedPackage>,
}

impl<'a> Parts<'a> {
    fn of(input: &'a ConstructionInput) -> Self {
        Self {
            input,
            set: input.configuration_schema_authority_set.as_ref(),
            package: input.registry_package.as_ref(),
        }
    }

    fn profile_specification_schema(&self) -> Option<&'a Artifact> {
        artifact(self.input.profile_specification_schema.as_ref())
    }

    fn profile_specification(&self) -> Option<&'a Artifact> {
        artifact(self.input.profile_specification.as_ref())
    }

    fn capability(&self) -> Option<&'a Artifact> {
        artifact(self.input.resolver_capability_specification.as_ref())
    }

    fn set_container(&self) -> Option<&'a Artifact> {
        self.set.map(|set| &set.component.artifact)
    }

    /// Every schema the set is given bytes for, mapped or not.
    fn set_members(&self) -> impl Iterator<Item = &'a Artifact> + 'a {
        self.set.into_iter().flat_map(|set| set.members.values())
    }

    /// The set container's mappings, when it reads as an object of them.
    fn mappings(&self) -> Option<&'a serde_json::Map<String, Value>> {
        self.set_container()
            .and_then(Artifact::value)
            .and_then(|container| container.get("members"))
            .and_then(Value::as_object)
    }

    fn member(&self, role: RegistryRole) -> Option<&'a Artifact> {
        self.package.and_then(|package| package.members.get(&role))
    }

    /// The registry schemas the package is given.
    fn registry_schemas(&self) -> impl Iterator<Item = &'a Artifact> + 'a {
        let package = self.package;
        RegistryRole::ALL
            .into_iter()
            .filter(|role| role.schema().is_none())
            .filter_map(move |role| package.and_then(|package| package.members.get(&role)))
    }

    /// Every submitted artifact.
    fn submitted(&self) -> impl Iterator<Item = &'a Artifact> + 'a {
        let package = self.package;
        [
            self.profile_specification_schema(),
            self.profile_specification(),
            self.set_container(),
            self.capability(),
            package.map(|package| &package.component.artifact),
        ]
        .into_iter()
        .flatten()
        .chain(self.set_members())
        .chain(
            package
                .into_iter()
                .flat_map(|package| package.members.values()),
        )
    }

    /// Every schema artifact: the Profile Specification Schema, every schema
    /// the set is given bytes for, and the registry schemas.
    fn schemas(&self) -> impl Iterator<Item = &'a Artifact> + 'a {
        self.profile_specification_schema()
            .into_iter()
            .chain(self.set_members())
            .chain(self.registry_schemas())
    }
}

/// Every bound's total over the input.
pub(crate) fn totals(root: &ConstructionRoot, input: &ConstructionInput) -> Totals {
    let parts = Parts::of(input);
    let mut totals = Totals::default();
    count_artifacts(&mut totals, &parts);
    count_schemas(&mut totals, &parts);
    count_validations(&mut totals, root, &parts);
    count_registries(&mut totals, &parts);
    count_cross_authority_checks(&mut totals, &parts);
    totals
}

/// The `input` bounds, over every submitted artifact.
fn count_artifacts(totals: &mut Totals, parts: &Parts<'_>) {
    for artifact in parts.submitted() {
        let bytes = count(artifact.bytes().len());
        totals.add(B::InputMaxTotalArtifactBytes, Some(bytes));
        totals.max(B::InputMaxSingleArtifactBytes, bytes);
        if let Some(counts) = artifact.counts() {
            totals.max(B::InputMaxDepth, counts.depth);
            totals.add(B::InputMaxNodes, Some(counts.nodes));
            totals.add(B::InputMaxCollectionEntries, Some(counts.entries));
            totals.add(B::InputMaxTotalStringBytes, Some(counts.string_bytes));
            totals.max(B::InputMaxSingleStringBytes, counts.longest_string);
        }
    }
}

/// The `schema` bounds over the set's mappings and every schema artifact.
fn count_schemas(totals: &mut Totals, parts: &Parts<'_>) {
    totals.add(
        B::SchemaMaxConfigurationSchemas,
        Some(parts.mappings().map_or(0, |mappings| count(mappings.len()))),
    );
    for artifact in parts.schemas() {
        let bytes = count(artifact.bytes().len());
        totals.add(B::SchemaMaxTotalArtifactBytes, Some(bytes));
        totals.max(B::SchemaMaxSingleArtifactBytes, bytes);
        if let Some(document) = artifact.value() {
            let census = census(document);
            totals.add(B::SchemaMaxNodes, Some(census.nodes));
            totals.add(B::SchemaMaxReferences, Some(census.references));
        }
    }
}

/// The pairs of every validation but a registry's: each declared tuple and
/// container, and the Capability Specification's body, against its envelope
/// schemas; each schema against the meta-schema; and the Profile
/// Specification against the schema it is given.
fn count_validations(totals: &mut Totals, root: &ConstructionRoot, parts: &Parts<'_>) {
    let input = parts.input;
    let components = &root.admission().components;
    let envelope = |schema: Option<&EnvelopeSchema>| {
        schema.map(|&schema| count(root.envelope(schema).node_count()))
    };
    for (admission, component) in [
        (
            &components.profile_specification_schema,
            input.profile_specification_schema.as_ref(),
        ),
        (
            &components.profile_specification,
            input.profile_specification.as_ref(),
        ),
        (
            &components.configuration_schema_authority_set,
            parts.set.map(|set| &set.component),
        ),
        (
            &components.resolver_capability_specification,
            input.resolver_capability_specification.as_ref(),
        ),
        (
            &components.registry_package,
            parts.package.map(|package| &package.component),
        ),
    ] {
        let Some(component) = component else {
            continue;
        };
        let mut schemas = admission.envelope_schemas.iter();
        let tuple = Some(instance_nodes(&component.tuple));
        totals.add(
            B::SchemaMaxValidationWorkUnits,
            pairs(envelope(schemas.next()), tuple),
        );
        totals.add(
            B::SchemaMaxValidationWorkUnits,
            pairs(envelope(schemas.next()), nodes(Some(&component.artifact))),
        );
    }
    let meta_schema = Some(count(root.meta_schema().node_count()));
    let mapped = parts
        .mappings()
        .into_iter()
        .flat_map(|mappings| mappings.keys())
        .filter_map(|version| parts.set.and_then(|set| set.members.get(version)));
    for artifact in parts
        .profile_specification_schema()
        .into_iter()
        .chain(mapped)
        .chain(parts.registry_schemas())
    {
        totals.add(
            B::SchemaMaxValidationWorkUnits,
            pairs(meta_schema, nodes(Some(artifact))),
        );
    }
    totals.add(
        B::SchemaMaxValidationWorkUnits,
        pairs(
            schema_nodes(parts.profile_specification_schema()),
            nodes(parts.profile_specification()),
        ),
    );
}

/// The `registry` bounds over the three registries, and each one's
/// validation against its role's schema.
fn count_registries(totals: &mut Totals, parts: &Parts<'_>) {
    for role in RegistryRole::ALL {
        let (Some(schema), Some(registry)) = (role.schema(), parts.member(role)) else {
            continue;
        };
        let bytes = count(registry.bytes().len());
        totals.add(B::RegistryMaxTotalArtifactBytes, Some(bytes));
        totals.max(B::RegistryMaxSingleMemberArtifactBytes, bytes);
        totals.add(B::RegistryMaxRows, Some(length(registry.value(), "rows")));
        totals.add(
            B::RegistryMaxValidationWorkUnits,
            pairs(schema_nodes(parts.member(schema)), nodes(Some(registry))),
        );
    }
}

/// One check for each item a rule compares: the association, the four
/// authorities the Profile Specification names, each bound path position,
/// and each Check Registry row.
fn count_cross_authority_checks(totals: &mut Totals, parts: &Parts<'_>) {
    let positions = length(
        parts.profile_specification().and_then(Artifact::value),
        "resourceBounds",
    )
    .max(length(
        parts.capability().and_then(Artifact::value),
        "policyComparable",
    ));
    let rows = length(
        parts
            .member(RegistryRole::CheckRegistry)
            .and_then(Artifact::value),
        "rows",
    );
    for checks in [1, 4, positions, rows] {
        totals.add(B::CrossAuthorityMaxChecks, Some(checks));
    }
}

/// The first bound, in the canonical order of `bounds`, whose total exceeds
/// its value.
pub(crate) fn first_exceeded(
    totals: &Totals,
    bounds: &[BoundRecord],
) -> Option<ConstructionBoundId> {
    bounds
        .iter()
        .find(|record| {
            let value = Bound::read(&record.value)
                .expect("the built-in admission body spells every bound")
                .get();
            totals.get(record.id).is_none_or(|total| total > value)
        })
        .map(|record| record.id)
}

/// Check the construction-root bounds: row 1 of the construction-failure
/// precedence.
pub(crate) fn check(root: &ConstructionRoot, input: &ConstructionInput) -> Result<(), Refusal> {
    first_exceeded(&totals(root, input), &root.admission().bounds)
        .map_or(Ok(()), |bound| Err(Refusal::exceeding(bound)))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::construction::admission::ConstructionAdmission;
    use crate::construction::component;
    use crate::construction::flow::testing::{
        committed, AUTHORITY_SET, CAPABILITY, CONFIGURATION_SCHEMA, PACKAGE, PROFILE_SPECIFICATION,
        PROFILE_SPECIFICATION_SCHEMA, REGISTRY_MEMBERS,
    };
    use crate::construction::registry::check::CheckRegistry;
    use crate::construction::registry::finding::FindingRegistry;
    use crate::construction::registry::safe_origin::SafeOriginKindRegistry;
    use crate::construction::scan::testing::counted;

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    fn value(bytes: &[u8]) -> Value {
        component::strict_json(bytes).unwrap()
    }

    #[test]
    fn the_totals_follow_the_canonical_order_of_the_admission_body() {
        for (index, bound) in ConstructionBoundId::ALL.into_iter().enumerate() {
            assert_eq!(bound as usize, index);
        }
        let bounds = &root().admission().bounds;
        assert_eq!(
            bounds.iter().map(|record| record.id).collect::<Vec<_>>(),
            ConstructionBoundId::ALL
        );
        assert_eq!(*bounds, ConstructionAdmission::revision_zero().bounds);
    }

    /// The committed input totals what its artifacts count, walked from
    /// their decoded values, and what its validations pair.
    #[test]
    fn the_committed_input_totals_what_its_artifacts_count() {
        let input = committed();
        let totals = totals(root(), &input);
        let all: Vec<&[u8]> = [
            PROFILE_SPECIFICATION_SCHEMA,
            PROFILE_SPECIFICATION,
            AUTHORITY_SET,
            CONFIGURATION_SCHEMA,
            CAPABILITY,
            PACKAGE,
        ]
        .into_iter()
        .chain(REGISTRY_MEMBERS)
        .collect();
        let sum = |count: fn(&[u8]) -> u64, artifacts: &[&[u8]]| {
            artifacts.iter().map(|bytes| count(bytes)).sum::<u64>()
        };
        let most = |count: fn(&[u8]) -> u64, artifacts: &[&[u8]]| {
            artifacts.iter().map(|bytes| count(bytes)).max().unwrap()
        };
        let bytes: fn(&[u8]) -> u64 = |bytes| super::count(bytes.len());
        let walked = |bytes: &[u8]| counted(&value(bytes));
        assert_eq!(
            totals.get(B::InputMaxTotalArtifactBytes),
            Some(sum(bytes, &all))
        );
        assert_eq!(
            totals.get(B::InputMaxSingleArtifactBytes),
            Some(most(bytes, &all))
        );
        assert_eq!(
            totals.get(B::InputMaxDepth),
            all.iter().map(|bytes| walked(bytes).depth).max()
        );
        assert_eq!(
            totals.get(B::InputMaxNodes),
            Some(all.iter().map(|bytes| walked(bytes).nodes).sum())
        );
        assert_eq!(
            totals.get(B::InputMaxCollectionEntries),
            Some(all.iter().map(|bytes| walked(bytes).entries).sum())
        );
        assert_eq!(
            totals.get(B::InputMaxTotalStringBytes),
            Some(all.iter().map(|bytes| walked(bytes).string_bytes).sum())
        );
        assert_eq!(
            totals.get(B::InputMaxSingleStringBytes),
            all.iter().map(|bytes| walked(bytes).longest_string).max()
        );

        // The schema artifacts: the Profile Specification Schema, the mapped
        // schema, and the three registry schemas.
        let schemas = [
            PROFILE_SPECIFICATION_SCHEMA,
            CONFIGURATION_SCHEMA,
            REGISTRY_MEMBERS[0],
            REGISTRY_MEMBERS[1],
            REGISTRY_MEMBERS[2],
        ];
        assert_eq!(totals.get(B::SchemaMaxConfigurationSchemas), Some(1));
        assert_eq!(
            totals.get(B::SchemaMaxTotalArtifactBytes),
            Some(sum(bytes, &schemas))
        );
        assert_eq!(
            totals.get(B::SchemaMaxSingleArtifactBytes),
            Some(most(bytes, &schemas))
        );
        let census = |bytes: &[u8]| census(&value(bytes));
        assert_eq!(
            totals.get(B::SchemaMaxNodes),
            Some(schemas.iter().map(|bytes| census(bytes).nodes).sum())
        );
        assert_eq!(
            totals.get(B::SchemaMaxReferences),
            Some(schemas.iter().map(|bytes| census(bytes).references).sum())
        );

        // Every validation but a registry's, each the product of its
        // schema's nodes and its instance's.
        let node_count =
            |schema: EnvelopeSchema| super::count(root().envelope(schema).node_count());
        let tuple =
            |component: &Option<Submitted>| instance_nodes(&component.as_ref().unwrap().tuple);
        let set_tuple = instance_nodes(
            &input
                .configuration_schema_authority_set
                .as_ref()
                .unwrap()
                .component
                .tuple,
        );
        let package_tuple =
            instance_nodes(&input.registry_package.as_ref().unwrap().component.tuple);
        let meta = super::count(root().meta_schema().node_count());
        let nodes = |bytes: &[u8]| walked(bytes).nodes;
        let expected = node_count(EnvelopeSchema::ComponentTuple)
            * (tuple(&input.profile_specification_schema)
                + set_tuple
                + tuple(&input.resolver_capability_specification)
                + package_tuple)
            + node_count(EnvelopeSchema::ProfileSpecificationTuple)
                * tuple(&input.profile_specification)
            + node_count(EnvelopeSchema::ConfigurationSchemaAuthoritySet) * nodes(AUTHORITY_SET)
            + node_count(EnvelopeSchema::ResolverCapabilitySpecification) * nodes(CAPABILITY)
            + node_count(EnvelopeSchema::RegistryPackage) * nodes(PACKAGE)
            + meta * schemas.iter().map(|bytes| nodes(bytes)).sum::<u64>()
            + census(PROFILE_SPECIFICATION_SCHEMA).nodes * nodes(PROFILE_SPECIFICATION);
        assert_eq!(totals.get(B::SchemaMaxValidationWorkUnits), Some(expected));

        // The three registries.
        let registries = [
            REGISTRY_MEMBERS[3],
            REGISTRY_MEMBERS[4],
            REGISTRY_MEMBERS[5],
        ];
        assert_eq!(
            totals.get(B::RegistryMaxRows),
            Some(super::count(
                SafeOriginKindRegistry::revision_zero().rows.len()
                    + CheckRegistry::revision_zero().rows.len()
                    + FindingRegistry::revision_zero().rows.len()
            ))
        );
        assert_eq!(
            totals.get(B::RegistryMaxTotalArtifactBytes),
            Some(sum(bytes, &registries))
        );
        assert_eq!(
            totals.get(B::RegistryMaxSingleMemberArtifactBytes),
            Some(most(bytes, &registries))
        );
        let expected = (0..3)
            .map(|index| census(REGISTRY_MEMBERS[index]).nodes * nodes(REGISTRY_MEMBERS[index + 3]))
            .sum::<u64>();
        assert_eq!(
            totals.get(B::RegistryMaxValidationWorkUnits),
            Some(expected)
        );

        // One association, four named authorities, 52 bound paths, and a
        // check for each Check Registry row.
        assert_eq!(
            totals.get(B::CrossAuthorityMaxChecks),
            Some(1 + 4 + 52 + super::count(CheckRegistry::revision_zero().rows.len()))
        );
        assert_eq!(first_exceeded(&totals, &root().admission().bounds), None);
    }

    /// Bytes the set is given for a `schemaVersion` it does not map count as
    /// a submitted schema artifact, which no validation reads (plan 025's
    /// #125), and a declared tuple counts only as its validation's instance.
    #[test]
    fn each_scope_counts_what_construction_is_given_for_it() {
        let before = totals(root(), &committed());
        let changed = |input: &ConstructionInput| {
            let totals = totals(root(), input);
            ConstructionBoundId::ALL
                .into_iter()
                .filter(|&bound| totals.get(bound) != before.get(bound))
                .collect::<Vec<_>>()
        };
        let mut unmapped = committed();
        let extra = br##"{"$schema": "x", "properties": {"a": {"$ref": "#"}}}"##;
        unmapped
            .configuration_schema_authority_set
            .as_mut()
            .unwrap()
            .members
            .insert("1".to_owned(), Artifact::read(extra.to_vec()));
        assert_eq!(
            changed(&unmapped),
            [
                B::InputMaxTotalArtifactBytes,
                B::InputMaxNodes,
                B::InputMaxCollectionEntries,
                B::InputMaxTotalStringBytes,
                B::SchemaMaxTotalArtifactBytes,
                B::SchemaMaxNodes,
                B::SchemaMaxReferences,
            ]
        );
        let mut tuple = committed();
        tuple
            .resolver_capability_specification
            .as_mut()
            .unwrap()
            .tuple["extra"] = json!([1, 2]);
        assert_eq!(changed(&tuple), [B::SchemaMaxValidationWorkUnits]);
        // An artifact that is not strict UTF-8 JSON counts only its bytes.
        let mut bytes_only = committed();
        bytes_only
            .configuration_schema_authority_set
            .as_mut()
            .unwrap()
            .members
            .insert("1".to_owned(), Artifact::read(b"[".to_vec()));
        assert_eq!(
            changed(&bytes_only),
            [
                B::InputMaxTotalArtifactBytes,
                B::SchemaMaxTotalArtifactBytes
            ]
        );
        // A missing component counts nothing.
        let mut without = committed();
        without.resolver_capability_specification = None;
        let totals = totals(root(), &without);
        assert_eq!(
            before.get(B::InputMaxTotalArtifactBytes).unwrap()
                - totals.get(B::InputMaxTotalArtifactBytes).unwrap(),
            super::count(CAPABILITY.len())
        );
        assert_eq!(
            totals.get(B::CrossAuthorityMaxChecks),
            before.get(B::CrossAuthorityMaxChecks)
        );
    }

    /// A rule's checks grow with what it compares: the longer list of bound
    /// paths, and the Check Registry's rows.
    #[test]
    fn the_cross_authority_checks_grow_with_what_the_rules_compare() {
        let checks =
            |input: &ConstructionInput| totals(root(), input).get(B::CrossAuthorityMaxChecks);
        let rows = super::count(CheckRegistry::revision_zero().rows.len());
        let mut longer = committed();
        let mut body = value(PROFILE_SPECIFICATION);
        body["resourceBounds"]
            .as_array_mut()
            .unwrap()
            .extend((0..8).map(|index| json!(format!("extra.path{index}"))));
        longer.profile_specification = Some(component::testing::submitted(
            "intlify-project-profile",
            "0",
            &serde_json::to_vec(&body).unwrap(),
        ));
        assert_eq!(checks(&longer), Some(1 + 4 + 60 + rows));
        let mut shorter = committed();
        body["resourceBounds"] = json!(["only.one"]);
        shorter.profile_specification = Some(component::testing::submitted(
            "intlify-project-profile",
            "0",
            &serde_json::to_vec(&body).unwrap(),
        ));
        assert_eq!(checks(&shorter), Some(1 + 4 + 52 + rows));
        let mut none = committed();
        none.registry_package = None;
        none.profile_specification = None;
        assert_eq!(checks(&none), Some(1 + 4 + 52));
    }

    fn records(value: &str) -> Vec<BoundRecord> {
        ConstructionBoundId::ALL
            .into_iter()
            .map(|id| BoundRecord {
                id,
                value: value.to_owned(),
                unit: id.unit(),
            })
            .collect()
    }

    /// An exact total is admitted, one more is refused, and of several
    /// exceeded bounds the first in the canonical order is named.
    #[test]
    fn the_first_bound_over_its_value_is_named() {
        let bounds = records("1000");
        for bound in ConstructionBoundId::ALL {
            let mut totals = Totals::default();
            totals.0[bound as usize] = Some(1000);
            assert_eq!(first_exceeded(&totals, &bounds), None, "{bound:?}");
            totals.0[bound as usize] = Some(1001);
            assert_eq!(first_exceeded(&totals, &bounds), Some(bound), "{bound:?}");
            // A total past u64 exceeds every bound.
            totals.0[bound as usize] = None;
            assert_eq!(first_exceeded(&totals, &bounds), Some(bound), "{bound:?}");
        }
        let mut totals = Totals([Some(1001); 18]);
        assert_eq!(
            first_exceeded(&totals, &bounds),
            Some(B::InputMaxTotalArtifactBytes)
        );
        totals.0[0] = Some(0);
        assert_eq!(
            first_exceeded(&totals, &bounds),
            Some(B::InputMaxSingleArtifactBytes)
        );
    }

    #[test]
    fn the_arithmetic_is_checked_and_never_saturates_into_a_bound() {
        let mut totals = Totals::default();
        totals.add(B::InputMaxNodes, Some(u64::MAX));
        assert_eq!(totals.get(B::InputMaxNodes), Some(u64::MAX));
        totals.add(B::InputMaxNodes, Some(1));
        assert_eq!(totals.get(B::InputMaxNodes), None);
        // Once past u64, a total stays past it.
        totals.add(B::InputMaxNodes, Some(0));
        totals.max(B::InputMaxNodes, 0);
        assert_eq!(totals.get(B::InputMaxNodes), None);
        assert_eq!(pairs(Some(u64::MAX), Some(2)), None);
        assert_eq!(pairs(None, Some(2)), Some(0));
        assert_eq!(pairs(Some(3), Some(2)), Some(6));
    }

    #[test]
    fn a_refusal_at_row_1_names_its_bound_and_no_other_row_does() {
        let refusal = Refusal::exceeding(B::InputMaxDepth);
        assert_eq!(refusal.number(), 1);
        assert_eq!(refusal.bound(), Some(B::InputMaxDepth));
        let mut input = committed();
        input
            .configuration_schema_authority_set
            .as_mut()
            .unwrap()
            .members
            .insert(
                "1".to_owned(),
                Artifact::read(format!("{}{}", "[".repeat(65), "]".repeat(65)).into_bytes()),
            );
        assert_eq!(check(root(), &input), Err(refusal));
        assert_eq!(check(root(), &committed()), Ok(()));
    }
}
