// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Prepared owner operations. Only actual ordinary core functions run between
//! markers; setup, dispatch, observation encoding, and destruction remain outside.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::construction::identity::ConstructionIdentity;
use crate::input_limits::InputLimits;
use crate::materialize::{materialize_file, MaterializationError, MaterializedDocument};
use crate::model::IntlifyConfig;
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::selection::{
    InvalidSelectorType, Selection, SelectionFailure, SelectionInvariant, SelectionPrerequisite,
    SelectorInput,
};
use crate::structural::{
    AdmissionInvariant, AuthoringSchema, StructuralAnalysis, StructuralLimits,
};

use super::cases::Engine;
use super::clock::Clock;
use super::instrument::{Instrument, Interval, Timed};
use super::locale_icu4x::PreparedEngine;
use super::locale_resolution::Canonicalizers;
use super::measure::{Measured, MeasurementFailure};
use super::observation::{self, Frame, Observation};
use super::{
    artifact_collection, construction, hydration_relation, locale, locale_icu4x, locale_resolution,
    resolved_binding, resource_policy, target_group,
};

pub(super) type Config = IntlifyConfig<PolicyReference, TargetProfileReference>;
pub(super) type Analysis = StructuralAnalysis<PolicyReference, TargetProfileReference>;
pub(super) type Schema = AuthoringSchema<PolicyReference, TargetProfileReference>;

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Operation {
    FileMaterialization,
    StructuralAnalysis,
    AuthoringConstruction,
    ProfileSelection,
    LocaleCanonicalization,
    LocalePolicyResolution,
    ResourcePolicyBootstrap,
    ArtifactCollectionAdmission,
    ResolvedBindingConstruction,
    TargetAndGroupResolution,
    HydrationRelationValidation,
    ConstructionAuthorityAdmission,
    ConstructionIdentity,
}

impl Operation {
    pub(super) const ALL: [Self; 13] = [
        Self::FileMaterialization,
        Self::StructuralAnalysis,
        Self::AuthoringConstruction,
        Self::ProfileSelection,
        Self::LocaleCanonicalization,
        Self::LocalePolicyResolution,
        Self::ResourcePolicyBootstrap,
        Self::ArtifactCollectionAdmission,
        Self::ResolvedBindingConstruction,
        Self::TargetAndGroupResolution,
        Self::HydrationRelationValidation,
        Self::ConstructionAuthorityAdmission,
        Self::ConstructionIdentity,
    ];
    pub(super) const fn phase(self) -> &'static str {
        match self {
            Self::FileMaterialization => "profile_entry_materialize",
            Self::StructuralAnalysis | Self::AuthoringConstruction => "profile_structural_admit",
            Self::ProfileSelection => "profile_select",
            Self::LocaleCanonicalization | Self::LocalePolicyResolution => "profile_locale_resolve",
            Self::ResourcePolicyBootstrap
            | Self::ArtifactCollectionAdmission
            | Self::ResolvedBindingConstruction => "profile_artifact_admit",
            Self::TargetAndGroupResolution | Self::HydrationRelationValidation => {
                "profile_target_group_resolve"
            }
            Self::ConstructionAuthorityAdmission | Self::ConstructionIdentity => {
                "profile_resolver_construct"
            }
        }
    }
    pub(super) const fn cost(self) -> &'static str {
        match self {
            Self::FileMaterialization => "file_decode_and_source_map",
            Self::StructuralAnalysis => "structural_analysis",
            Self::AuthoringConstruction => "authoring_model_construction",
            Self::ProfileSelection => "named_profile_selection",
            Self::LocaleCanonicalization => "locale_canonicalization",
            Self::LocalePolicyResolution => "locale_policy_resolution",
            Self::ResourcePolicyBootstrap => "resource_policy_bootstrap",
            Self::ArtifactCollectionAdmission => "complete_artifact_collection_admission",
            Self::ResolvedBindingConstruction => "resolved_binding_construction",
            Self::TargetAndGroupResolution => "target_and_group_resolution",
            Self::HydrationRelationValidation => "hydration_relation_validation",
            Self::ConstructionAuthorityAdmission => "construction_authority_admission",
            Self::ConstructionIdentity => "construction_identity",
        }
    }
    pub(super) const fn boundary(self) -> &'static str {
        match self {
            Self::FileMaterialization => "minimum-file-materialization/0",
            Self::StructuralAnalysis => "minimum-structural-analysis/0",
            Self::AuthoringConstruction => "minimum-authoring-construction/0",
            Self::ProfileSelection => "minimum-provisional-selection/0",
            Self::LocaleCanonicalization => "minimum-single-locale-canonicalization/0",
            Self::LocalePolicyResolution => "locale-policy-resolution/0",
            Self::ResourcePolicyBootstrap => "resource-policy-bootstrap/0",
            Self::ArtifactCollectionAdmission => "artifact-collection-admission/0",
            Self::ResolvedBindingConstruction => "resolved-binding-construction/0",
            Self::TargetAndGroupResolution => "target-and-group-resolution/0",
            Self::HydrationRelationValidation => "hydration-relation-validation/0",
            Self::ConstructionAuthorityAdmission => "resolver-construction/0",
            Self::ConstructionIdentity => "resolver-construction-identity/0",
        }
    }
    /// The pair and boundary of the peak live memory measured over this
    /// operation's interval (D15:2764). Only a fresh resolver construction has
    /// one yet; a complete invocation's comes with that boundary.
    pub(super) const fn peak_memory(self) -> Option<PeakMemory> {
        match self {
            Self::ConstructionAuthorityAdmission => Some(PeakMemory {
                phase: "profile_resolve_peak_memory",
                cost: "construction_peak_live_memory",
                boundary: "resolver-construction-peak-memory/0",
            }),
            _ => None,
        }
    }
}

/// Design 015's memory pair over one operation's interval, and the boundary
/// the pair is measured at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PeakMemory {
    pub(super) phase: &'static str,
    pub(super) cost: &'static str,
    pub(super) boundary: &'static str,
}

pub(super) enum Prepared {
    Entry {
        source: Arc<[u8]>,
        limits: InputLimits,
    },
    Structural {
        schema: Schema,
        doc: Arc<MaterializedDocument>,
        limits: StructuralLimits,
    },
    Authoring(Analysis),
    Select {
        analysis: Analysis,
        selector: SelectorInput,
    },
    Locale {
        core: Arc<locale::Core>,
        input: Arc<str>,
    },
    LocalePolicyResolution(Box<locale_resolution::PreparedLocales>),
    ResourcePolicy(Box<resource_policy::PreparedPolicy>),
    ArtifactCollection(Box<artifact_collection::PreparedCollection>),
    ResolvedBinding(Box<resolved_binding::PreparedBindings>),
    TargetAndGroup(Box<target_group::PreparedTargets>),
    Hydration(Box<hydration_relation::PreparedHydration>),
    Construction(Box<construction::PreparedConstruction>),
    ConstructionIdentity(Box<construction::PreparedIdentity>),
    Icu4xLocale {
        engine: locale_icu4x::PreparedEngine,
        input: Arc<str>,
    },
}

pub(super) enum Output {
    Entry(Result<MaterializedDocument, MaterializationError>),
    Structural(Result<Analysis, AdmissionInvariant>),
    Authoring(Result<Option<Config>, AdmissionInvariant>),
    Select(Result<Selection<PolicyReference>, SelectionInvariant>),
    Locale(Result<crate::locale::Canonicalized, crate::locale::CanonicalizationFailure>),
    LocalePolicyResolution(locale_resolution::Resolved),
    ResourcePolicy(resource_policy::Resolved),
    ArtifactCollection(artifact_collection::Resolved),
    ResolvedBinding(resolved_binding::Resolved),
    TargetAndGroup(target_group::Resolved),
    Hydration(hydration_relation::Resolved),
    Construction(Box<construction::Resolved>),
    ConstructionIdentity(ConstructionIdentity),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum OutputFailure {
    AdmissionInvariant,
    SelectionInvariant,
    MissingCompleteRoot,
    ObservationEncoding,
    LocaleProviderUnavailable,
    LocaleProviderInvariant,
    LocalePolicyResolutionInvariant,
    ResourcePolicyInvariant,
    ArtifactCollectionInvariant,
    ResolvedBindingInvariant,
    TargetAndGroupResolutionInvariant,
    HydrationRelationValidationInvariant,
}

impl Prepared {
    pub(super) const fn operation(&self) -> Operation {
        match self {
            Self::Entry { .. } => Operation::FileMaterialization,
            Self::Structural { .. } => Operation::StructuralAnalysis,
            Self::Authoring(_) => Operation::AuthoringConstruction,
            Self::Select { .. } => Operation::ProfileSelection,
            Self::Locale { .. } | Self::Icu4xLocale { .. } => Operation::LocaleCanonicalization,
            Self::LocalePolicyResolution(_) => Operation::LocalePolicyResolution,
            Self::ResourcePolicy(_) => Operation::ResourcePolicyBootstrap,
            Self::ArtifactCollection(_) => Operation::ArtifactCollectionAdmission,
            Self::ResolvedBinding(_) => Operation::ResolvedBindingConstruction,
            Self::TargetAndGroup(_) => Operation::TargetAndGroupResolution,
            Self::Hydration(_) => Operation::HydrationRelationValidation,
            Self::Construction(_) => Operation::ConstructionAuthorityAdmission,
            Self::ConstructionIdentity(_) => Operation::ConstructionIdentity,
        }
    }

    /// The engine state of a case canonicalizing through the ICU4X provider;
    /// `None` for every other case.
    pub(super) fn icu4x_engine(&self) -> Option<Engine> {
        match self {
            Self::Icu4xLocale { engine, .. } => Some(engine.engine()),
            Self::LocalePolicyResolution(prepared) => prepared.icu4x().map(PreparedEngine::engine),
            _ => None,
        }
    }

    /// Time one invocation, as a duration sample's repetition does.
    pub(super) fn once(&self, clock: &impl Clock) -> Result<Measured<Output>, MeasurementFailure> {
        let interval = self.interval(&Timed(clock))?;
        Ok(Measured {
            duration: interval.quantity,
            output: interval.output,
        })
    }

    /// Measure one invocation with either instrument. Both measure the same
    /// production boundary: the clock times it, and the observer counts the
    /// live bytes it requests.
    pub(super) fn interval<I: Instrument>(
        &self,
        instrument: &I,
    ) -> Result<Interval<Output>, I::Failure> {
        // Prepare owned immutable handles and dispatch before the first marker.
        match self {
            Self::Entry { source, limits } => {
                let measured = instrument
                    .interval((Arc::clone(source), *limits), |(source, limits)| {
                        materialize_file(source, limits)
                    })?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Entry(measured.output),
                })
            }
            Self::Structural {
                schema,
                doc,
                limits,
            } => {
                let measured = instrument.interval(
                    (schema, Arc::clone(doc), *limits),
                    |(schema, doc, limits)| schema.analyze(doc, limits),
                )?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Structural(measured.output),
                })
            }
            Self::Authoring(analysis) => {
                let measured = instrument.interval(analysis, Analysis::construct)?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Authoring(measured.output),
                })
            }
            Self::Select { analysis, selector } => {
                let measured = instrument
                    .interval((analysis, selector), |(analysis, selector)| {
                        analysis.select(selector)
                    })?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Select(measured.output),
                })
            }
            Self::Locale { core, input } => {
                let measured = instrument
                    .interval((core.as_ref(), input.as_ref()), |(core, input)| {
                        core.canonicalize(input)
                    })?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Locale(measured.output),
                })
            }
            Self::LocalePolicyResolution(prepared) => {
                let fields = prepared.fields().ok_or_else(I::unavailable)?;
                let measured = match &prepared.canonicalizer {
                    Canonicalizers::Finite(core) => instrument.interval(
                        (&prepared.policy, &prepared.inputs, core.as_ref(), &fields),
                        |(policy, inputs, canonicalizer, fields)| {
                            locale_resolution::resolve(policy, inputs, canonicalizer, fields)
                        },
                    )?,
                    // A fresh engine is admitted and bound before the start
                    // mark and released after the end mark.
                    Canonicalizers::Icu4x(engine) => engine
                        .with_engine(|core| {
                            instrument.interval(
                                (&prepared.policy, &prepared.inputs, core, &fields),
                                |(policy, inputs, canonicalizer, fields)| {
                                    locale_resolution::resolve(
                                        policy,
                                        inputs,
                                        canonicalizer,
                                        fields,
                                    )
                                },
                            )
                        })
                        .map_err(|_| I::unavailable())??,
                };
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::LocalePolicyResolution(measured.output),
                })
            }
            Self::ResourcePolicy(prepared) => {
                let inputs = prepared.inputs().ok_or_else(I::unavailable)?;
                let measured = instrument.interval(
                    (&prepared.capability, &prepared.collection, &inputs),
                    |(capability, collection, inputs)| {
                        resource_policy::resolve(capability, collection, inputs)
                    },
                )?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::ResourcePolicy(measured.output),
                })
            }
            Self::ArtifactCollection(prepared) => {
                let measured = instrument.interval(
                    (&prepared.policy, &prepared.collection),
                    |(policy, collection)| artifact_collection::admit(policy, collection),
                )?;
                // The Set borrows the prepared collection, so it is copied out
                // after the end mark.
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::ArtifactCollection(artifact_collection::Resolved::retain(
                        measured.output,
                    )),
                })
            }
            Self::ResolvedBinding(prepared) => {
                // The collection is admitted into the Set before the start
                // mark; the resolution and the bindings borrow it, so they
                // are copied out after the end mark.
                let inputs = prepared.inputs().ok_or_else(I::unavailable)?;
                let measured = instrument.interval(&inputs, resolved_binding::construct)?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::ResolvedBinding(resolved_binding::Resolved::retain(
                        measured.output,
                    )),
                })
            }
            Self::TargetAndGroup(prepared) => {
                // Phase 7, the collection admission, and Phase 9 run before
                // the start mark; the targets borrow the Set, so they are
                // copied out after the end mark.
                let set = prepared.set().ok_or_else(I::unavailable)?;
                let inputs = prepared.inputs(&set).ok_or_else(I::unavailable)?;
                let measured = instrument.interval(&inputs, target_group::construct)?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::TargetAndGroup(target_group::Resolved::retain(measured.output)),
                })
            }
            Self::Hydration(prepared) => {
                // Every earlier phase runs before the start mark; Phase 13's
                // result owns its values.
                let set = prepared.set().ok_or_else(I::unavailable)?;
                let inputs = prepared.inputs(&set).ok_or_else(I::unavailable)?;
                let measured = instrument.interval(&inputs, hydration_relation::construct)?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Hydration(hydration_relation::Resolved::retain(
                        measured.output,
                    )),
                })
            }
            Self::Construction(prepared) => {
                // The input is copied before the start mark. The resolver,
                // or the refused input construction hands back, is released
                // after the end mark.
                let measured = instrument
                    .interval((prepared.root, prepared.input.clone()), construction::build)?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Construction(Box::new(construction::Resolved {
                        root: prepared.root,
                        constructed: measured.output,
                    })),
                })
            }
            Self::ConstructionIdentity(prepared) => {
                // The resolver is constructed before the first start mark and
                // reused; the identity's bytes are copied out.
                let measured = instrument
                    .interval((prepared.root, &prepared.resolver), |(root, resolver)| {
                        resolver.identity(root)
                    })?;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::ConstructionIdentity(measured.output),
                })
            }
            Self::Icu4xLocale { engine, input } => {
                // A fresh engine is admitted and bound before the start mark
                // and released after the end mark.
                let measured = engine
                    .with_engine(|core| {
                        instrument.interval((core, input.as_ref()), |(core, input)| {
                            core.canonicalize(input)
                        })
                    })
                    .map_err(|_| I::unavailable())??;
                Ok(Interval {
                    quantity: measured.quantity,
                    output: Output::Locale(measured.output),
                })
            }
        }
    }

    pub(super) fn prerequisites_admitted(&self) -> bool {
        match self {
            Self::Authoring(analysis) => analysis.is_complete(),
            Self::LocalePolicyResolution(prepared) => prepared.fields().is_some(),
            Self::ResourcePolicy(prepared) => prepared.inputs().is_some(),
            Self::ResolvedBinding(prepared) => prepared.inputs().is_some(),
            Self::TargetAndGroup(prepared) => prepared
                .set()
                .is_some_and(|set| prepared.inputs(&set).is_some()),
            Self::Hydration(prepared) => prepared
                .set()
                .is_some_and(|set| prepared.inputs(&set).is_some()),
            _ => true,
        }
    }
}

impl Output {
    pub(super) fn observe(&self) -> Result<Observation, OutputFailure> {
        match self {
            Self::Entry(Ok(doc)) => Ok(observation::document(doc)),
            Self::Entry(Err(error)) => Ok(observation::materialization_failure(error)),
            Self::Structural(Ok(analysis)) => Ok(analysis.benchmark_observation()),
            Self::Structural(Err(_)) | Self::Authoring(Err(_)) => {
                Err(OutputFailure::AdmissionInvariant)
            }
            Self::Authoring(Ok(None)) => Err(OutputFailure::MissingCompleteRoot),
            Self::Authoring(Ok(Some(config))) => {
                let value =
                    serde_json::to_value(config).map_err(|_| OutputFailure::ObservationEncoding)?;
                let mut frame = Frame::new("complete-authoring-model");
                frame.json(&value);
                Ok(Observation {
                    shared: frame.finish(),
                    entry: None,
                })
            }
            Self::Select(Err(_)) => Err(OutputFailure::SelectionInvariant),
            Self::Select(Ok(selection)) => observe_selection(selection),
            Self::Locale(result) => locale::observe(result),
            Self::LocalePolicyResolution(resolved) => locale_resolution::observe(resolved),
            Self::ResourcePolicy(resolved) => resource_policy::observe(resolved),
            Self::ArtifactCollection(resolved) => artifact_collection::observe(resolved),
            Self::ResolvedBinding(resolved) => resolved_binding::observe(resolved),
            Self::TargetAndGroup(resolved) => target_group::observe(resolved),
            Self::Hydration(resolved) => hydration_relation::observe(resolved),
            Self::Construction(resolved) => construction::observe(resolved),
            Self::ConstructionIdentity(identity) => Ok(construction::observe_identity(identity)),
        }
    }
}

fn observe_selection(selection: &Selection<PolicyReference>) -> Result<Observation, OutputFailure> {
    let mut shared = Frame::new("provisional-selection");
    let entry = match selection {
        Selection::Selected(selected) => {
            shared.uint(0);
            shared.text(selected.id().as_str());
            shared.json(
                &serde_json::to_value(selected.resource_limits())
                    .map_err(|_| OutputFailure::ObservationEncoding)?,
            );
            let mut entry = Frame::new("selected-declaration-source");
            entry.span(selected.declaration_key_span());
            Some(entry.finish())
        }
        Selection::Rejected(failure) => {
            shared.uint(1);
            match failure {
                SelectionFailure::Required => shared.uint(0),
                SelectionFailure::InvalidType(kind) => {
                    shared.uint(1);
                    shared.uint(match kind {
                        InvalidSelectorType::Null => 0,
                        InvalidSelectorType::Boolean => 1,
                        InvalidSelectorType::Number => 2,
                        InvalidSelectorType::Array => 3,
                        InvalidSelectorType::Object => 4,
                    });
                }
                SelectionFailure::InvalidSyntax { bytes } => {
                    shared.uint(2);
                    shared.uint(*bytes);
                }
                SelectionFailure::OverLimit { limit, first_over } => {
                    shared.uint(3);
                    shared.uint(limit.get());
                    shared.uint(*first_over);
                }
                SelectionFailure::Unknown { bytes } => {
                    shared.uint(4);
                    shared.uint(*bytes);
                }
            }
            None
        }
        Selection::Unavailable(cause) => {
            shared.uint(2);
            shared.uint(match cause {
                SelectionPrerequisite::ConfigurationVersion => 0,
                SelectionPrerequisite::StructuralWork => 1,
                SelectionPrerequisite::ProfilesShape => 2,
                SelectionPrerequisite::ProfilesLimit => 3,
                SelectionPrerequisite::DeclarationIdentity => 4,
                SelectionPrerequisite::DeclarationBoundary => 5,
                SelectionPrerequisite::ResourceLimitsReference => 6,
            });
            None
        }
    };
    Ok(Observation {
        shared: shared.finish(),
        entry,
    })
}

#[cfg(test)]
pub(super) mod tests;
