// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's immutable resolver: what one successful construction built.
//!
//! Construction is given its input and keeps it: every component's declared
//! tuple and exact bytes stay beside the authorities construction admitted,
//! since staleness compares two retained inputs by value and replay needs
//! the complete input again (D15:2575, 2583). The resolver takes the input
//! rather than copying it, so construction holds no second copy of what the
//! caller acquired (D15:2796). Nothing changes the resolver after
//! construction.
//!
//! What the resolver supports in each specification slot comes from the
//! Profile Specification it admitted, which the implemented authority fixes
//! to the one the resolver is compiled from. The resolver's Resolver
//! Construction Identity is derived from it as a step of its own.
//!
//! The resolver analyzes a configuration with the schema the admitted
//! Configuration Schema Authority Set maps to the `schemaVersion` the model
//! reads, compiled once at construction, and selects it only for a
//! configuration whose root `schemaVersion` the mapping maps (D15:403, 424).
//! The implemented authority fixes the mapping to that one version and its
//! schema to the one this crate generates from its model.

#[cfg(any(test, feature = "benchmark", feature = "dev-example"))]
use std::sync::OnceLock;

use intlify_shared_json::decode;

use super::component::Refusal;
use super::flow::{self, AdmittedAuthorities, ConstructionInput};
use super::identity::{self, ConstructionIdentity};
use super::profile::{Pin, ProjectProfileSpecification, SlotValue};
use super::root::ConstructionRoot;
use super::staleness::ConstructionContext;
use crate::locale::{DatasetPin, SemanticPins, Supported, VersionedIdentity};
use crate::model::CONFIGURATION_SCHEMA_VERSION;
use crate::references::{PolicyReference, TargetProfileReference};
use crate::resolver::specification::SupportedSpecifications;
use crate::structural::AuthoringSchema;

/// A configuration schema compiled for the formal reference types.
type ConfigurationSchema = AuthoringSchema<PolicyReference, TargetProfileReference>;

/// A resolver one construction built: the input it was given, the
/// authorities it admitted, the Profile Specification's body as its type, and
/// the configuration schema it analyzes with.
#[derive(Debug)]
pub(crate) struct ConstructedResolver {
    input: ConstructionInput,
    admitted: AdmittedAuthorities,
    profile: ProjectProfileSpecification,
    configuration_schema: ConfigurationSchema,
}

/// Construct a resolver from the input, which it keeps: the 43 rows of the
/// construction-failure precedence, refusing with the first that fails.
pub(crate) fn build(
    root: &ConstructionRoot,
    input: ConstructionInput,
) -> Result<ConstructedResolver, Refusal> {
    let admitted = flow::construct(root, &input)?;
    // The implemented authority admits only the Profile Specification this
    // crate writes, so its body reads as the type it is written from.
    let profile =
        decode::typed::<ProjectProfileSpecification>(admitted.profile_specification.body.clone())
            .expect("the implemented Profile Specification reads as its type");
    // It admits only the configuration schema this crate generates from its
    // model too, so the schema compiles for that model.
    let configuration_schema = compile_configuration_schema(&input)
        .expect("the implemented configuration schema compiles for the model");
    Ok(ConstructedResolver {
        input,
        admitted,
        profile,
        configuration_schema,
    })
}

/// Compile the schema the input maps to the `schemaVersion` the model reads,
/// for that version of the mapping.
fn compile_configuration_schema(input: &ConstructionInput) -> Option<ConfigurationSchema> {
    let (version, member) = input
        .configuration_schema_authority_set
        .as_ref()?
        .members
        .get_key_value(CONFIGURATION_SCHEMA_VERSION)?;
    AuthoringSchema::compile(version.clone(), member.value()?.clone()).ok()
}

/// The specification a slot pins.
///
/// # Panics
///
/// Panics for a slot the invocation asserts, which no caller reads this way.
fn pinned(slot: &SlotValue) -> VersionedIdentity<String> {
    match slot {
        SlotValue::Pinned(Pin { identity, revision }) => VersionedIdentity {
            identity: identity.clone(),
            revision: revision.clone(),
        },
        SlotValue::Asserted(_) => unreachable!("only the vocabulary slot is asserted"),
    }
}

impl ConstructedResolver {
    /// The input construction was given.
    pub(crate) const fn input(&self) -> &ConstructionInput {
        &self.input
    }

    /// The authorities construction admitted.
    pub(crate) const fn admitted(&self) -> &AdmittedAuthorities {
        &self.admitted
    }

    /// The resolver's Resolver Construction Identity.
    pub(crate) fn identity(&self, root: &ConstructionRoot) -> ConstructionIdentity {
        identity::derive(root, &self.admitted)
    }

    /// The construction as staleness compares it.
    pub(crate) const fn context(&self) -> ConstructionContext<'_> {
        ConstructionContext {
            input: &self.input,
            admitted: &self.admitted,
        }
    }

    /// The configuration schema the resolver analyzes a configuration with.
    pub(crate) const fn configuration_schema(&self) -> &ConfigurationSchema {
        &self.configuration_schema
    }

    /// The canonicalization specification the resolver supports, with the
    /// one dataset it admits.
    pub(crate) fn locale_canonicalization(&self) -> Supported<String> {
        let dataset = &self.profile.locale_canonicalization_dataset;
        Supported::new(vec![SemanticPins {
            specification: pinned(&self.profile.specification_slots.locale_canonicalization),
            dataset: DatasetPin {
                identity: dataset.identity.clone(),
                semantic_digest: dataset.semantic_digest.clone(),
            },
        }])
        .expect("one entry lists its specification once")
    }

    /// What the resolver supports in each specification slot.
    pub(crate) fn supported(&self) -> SupportedSpecifications<String> {
        let slots = &self.profile.specification_slots;
        SupportedSpecifications::new(
            pinned(&slots.project_profile),
            self.locale_canonicalization(),
            pinned(&slots.locale_negotiation),
            pinned(&slots.message_locale_fallback),
            pinned(&slots.coverage),
            pinned(&slots.delivery_graph),
            pinned(&slots.delivery_placement),
        )
    }
}

/// The resolver the committed input builds, built once.
///
/// # Panics
///
/// Panics when the committed input does not construct, which the tests rule
/// out.
#[cfg(any(test, feature = "benchmark", feature = "dev-example"))]
pub(crate) fn committed() -> &'static ConstructedResolver {
    static COMMITTED: OnceLock<ConstructedResolver> = OnceLock::new();
    COMMITTED.get_or_init(|| {
        let root = ConstructionRoot::built_in().expect("the built-in Root Package verifies");
        build(root, super::committed::input()).expect("the committed input constructs")
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::construction::committed;
    use crate::construction::component::testing::row;
    use crate::construction::component::Artifact;
    use crate::construction::flow::testing::value;
    use crate::construction::profile;
    use crate::input_limits::Bound;
    use crate::materialize::materialize_file;
    use crate::model::IntlifyConfig;
    use crate::profile_fixtures::{complete_config, minimal_config};
    use crate::structural::{StructuralAnalysis, StructuralFailure, StructuralLimits};

    const VECTORS: &[u8] =
        include_bytes!("../../conformance/construction/construction-vectors-v0.json");

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    /// `schema`'s analysis of `config`, under fixed bounds.
    fn analysis(
        schema: &ConfigurationSchema,
        config: &serde_json::Value,
    ) -> StructuralAnalysis<PolicyReference, TargetProfileReference> {
        let bytes = serde_json::to_vec(config).unwrap();
        let doc = materialize_file(Arc::from(bytes), crate::materialize_tests::limits()).unwrap();
        let limits = StructuralLimits {
            max_profiles: Bound::new(8).unwrap(),
            max_profile_id_bytes: Bound::new(64).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        };
        schema.analyze(Arc::new(doc), limits).unwrap()
    }

    /// What `schema` constructs from `config`.
    fn analyzed(
        schema: &ConfigurationSchema,
        config: &serde_json::Value,
    ) -> Option<IntlifyConfig<PolicyReference, TargetProfileReference>> {
        analysis(schema, config).construct().unwrap()
    }

    #[test]
    fn a_resolver_keeps_the_input_it_was_built_from() {
        let input = committed::input();
        let resolver = build(root(), input.clone()).unwrap();
        assert_eq!(resolver.input(), &input);
        assert_eq!(
            resolver
                .admitted()
                .profile_specification
                .semantic_digest
                .as_str(),
            profile::SEMANTIC_DIGEST
        );
        assert_eq!(
            resolver.identity(root()).text(),
            value(VECTORS)["constructionIdentity"]["text"]
        );
        assert_eq!(committed().identity(root()), resolver.identity(root()));
    }

    #[test]
    fn a_refused_input_builds_no_resolver() {
        let mut input = committed::input();
        input.profile_specification_schema = None;
        assert_eq!(row(build(root(), input)), Some(2));
        // Nothing reads the configuration schema before construction admits
        // the set that maps it.
        let mut input = committed::input();
        input.configuration_schema_authority_set = None;
        assert_eq!(row(build(root(), input)), Some(15));
    }

    /// The values the resolver tests write by hand are the ones a resolver
    /// built from the committed input supports.
    #[test]
    fn the_supported_values_are_the_profile_specifications() {
        let resolver = committed();
        assert_eq!(
            resolver.locale_canonicalization(),
            Supported::revision_zero()
        );
        let project_profile = VersionedIdentity {
            identity: profile::IDENTITY.to_owned(),
            revision: profile::REVISION.to_owned(),
        };
        assert_eq!(
            resolver.supported(),
            SupportedSpecifications::revision_zero(project_profile, Supported::revision_zero())
        );
    }

    /// The resolver compiles the document it admitted, member for member in
    /// the document's order, and not a schema generated again.
    #[test]
    fn the_configuration_schema_is_the_admitted_document() {
        let schema = committed().configuration_schema();
        assert_eq!(
            serde_json::to_string(schema.schema_body()).unwrap(),
            serde_json::to_string(&value(committed::CONFIGURATION_SCHEMA)).unwrap()
        );
    }

    /// The configuration schema the resolver admitted is the one this crate
    /// generates from its model, and constructs what that schema constructs.
    #[test]
    fn the_admitted_schema_is_the_one_generated_from_the_model() {
        let schema = committed().configuration_schema();
        let generated = crate::schema::project_profile_config_schema().unwrap();
        assert_eq!(schema.schema_body(), &generated);
        let model = ConfigurationSchema::for_model().unwrap();
        for config in [minimal_config(), complete_config()] {
            let constructed = analyzed(schema, &config);
            assert!(constructed.is_some());
            assert_eq!(constructed, analyzed(&model, &config));
        }
    }

    /// A root `schemaVersion` the admitted mapping does not map is
    /// unsupported (D15:424).
    #[test]
    fn a_version_the_mapping_does_not_map_is_unsupported() {
        let mut config = minimal_config();
        config["schemaVersion"] = serde_json::json!("1");
        let analysis = analysis(committed().configuration_schema(), &config);
        assert!(!analysis.version_selected());
        assert_eq!(
            analysis.issues()[0].reason,
            StructuralFailure::SchemaVersionUnsupported
        );
    }

    /// Construction reads no member the set does not map, and neither does
    /// the resolver.
    #[test]
    fn an_unmapped_member_leaves_the_configuration_schema() {
        let mut input = committed::input();
        input
            .configuration_schema_authority_set
            .as_mut()
            .unwrap()
            .members
            .insert(
                "1".to_owned(),
                Artifact::read(committed::PROFILE_SPECIFICATION_SCHEMA.to_vec()),
            );
        let resolver = build(root(), input).unwrap();
        assert_eq!(
            resolver.configuration_schema().schema_body(),
            committed().configuration_schema().schema_body()
        );
    }
}
