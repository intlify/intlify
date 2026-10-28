// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Prepared owner operations. Only actual ordinary core functions run between
//! markers; setup, dispatch, observation encoding, and destruction remain outside.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

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
use super::locale_icu4x::PreparedEngine;
use super::locale_resolution::Canonicalizers;
use super::measure::{measure, Measured, MeasurementFailure};
use super::observation::{self, Frame, Observation};
use super::{
    artifact_collection, locale, locale_icu4x, locale_resolution, resolved_binding, resource_policy,
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
}

impl Operation {
    pub(super) const ALL: [Self; 9] = [
        Self::FileMaterialization,
        Self::StructuralAnalysis,
        Self::AuthoringConstruction,
        Self::ProfileSelection,
        Self::LocaleCanonicalization,
        Self::LocalePolicyResolution,
        Self::ResourcePolicyBootstrap,
        Self::ArtifactCollectionAdmission,
        Self::ResolvedBindingConstruction,
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
        }
    }
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

    pub(super) fn once(&self, clock: &impl Clock) -> Result<Measured<Output>, MeasurementFailure> {
        // Prepare owned immutable handles and dispatch before the first marker.
        match self {
            Self::Entry { source, limits } => {
                let measured =
                    measure(clock, (Arc::clone(source), *limits), |(source, limits)| {
                        materialize_file(source, limits)
                    })?;
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::Entry(measured.output),
                })
            }
            Self::Structural {
                schema,
                doc,
                limits,
            } => {
                let measured = measure(
                    clock,
                    (schema, Arc::clone(doc), *limits),
                    |(schema, doc, limits)| schema.analyze(doc, limits),
                )?;
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::Structural(measured.output),
                })
            }
            Self::Authoring(analysis) => {
                let measured = measure(clock, analysis, Analysis::construct)?;
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::Authoring(measured.output),
                })
            }
            Self::Select { analysis, selector } => {
                let measured = measure(clock, (analysis, selector), |(analysis, selector)| {
                    analysis.select(selector)
                })?;
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::Select(measured.output),
                })
            }
            Self::Locale { core, input } => {
                let measured = measure(clock, (core.as_ref(), input.as_ref()), |(core, input)| {
                    core.canonicalize(input)
                })?;
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::Locale(measured.output),
                })
            }
            Self::LocalePolicyResolution(prepared) => {
                let fields = prepared
                    .fields()
                    .ok_or(MeasurementFailure::PrerequisiteUnavailable)?;
                let measured = match &prepared.canonicalizer {
                    Canonicalizers::Finite(core) => measure(
                        clock,
                        (&prepared.policy, &prepared.inputs, core.as_ref(), &fields),
                        |(policy, inputs, canonicalizer, fields)| {
                            locale_resolution::resolve(policy, inputs, canonicalizer, fields)
                        },
                    )?,
                    // A fresh engine is admitted and bound before the start
                    // read and released after the end read.
                    Canonicalizers::Icu4x(engine) => engine
                        .with_engine(|core| {
                            measure(
                                clock,
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
                        .map_err(|_| MeasurementFailure::PrerequisiteUnavailable)??,
                };
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::LocalePolicyResolution(measured.output),
                })
            }
            Self::ResourcePolicy(prepared) => {
                let inputs = prepared
                    .inputs()
                    .ok_or(MeasurementFailure::PrerequisiteUnavailable)?;
                let measured = measure(
                    clock,
                    (&prepared.capability, &prepared.collection, &inputs),
                    |(capability, collection, inputs)| {
                        resource_policy::resolve(capability, collection, inputs)
                    },
                )?;
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::ResourcePolicy(measured.output),
                })
            }
            Self::ArtifactCollection(prepared) => {
                let measured = measure(
                    clock,
                    (&prepared.policy, &prepared.collection),
                    |(policy, collection)| artifact_collection::admit(policy, collection),
                )?;
                // The Set borrows the prepared collection, so it is copied out
                // after the end read.
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::ArtifactCollection(artifact_collection::Resolved::retain(
                        measured.output,
                    )),
                })
            }
            Self::ResolvedBinding(prepared) => {
                // The collection is admitted into the Set before the start
                // read; the resolution and the bindings borrow it, so they
                // are copied out after the end read.
                let inputs = prepared
                    .inputs()
                    .ok_or(MeasurementFailure::PrerequisiteUnavailable)?;
                let measured = measure(clock, &inputs, resolved_binding::construct)?;
                Ok(Measured {
                    duration: measured.duration,
                    output: Output::ResolvedBinding(resolved_binding::Resolved::retain(
                        measured.output,
                    )),
                })
            }
            Self::Icu4xLocale { engine, input } => {
                // A fresh engine is admitted and bound before the start read
                // and released after the end read.
                let measured = engine
                    .with_engine(|core| {
                        measure(clock, (core, input.as_ref()), |(core, input)| {
                            core.canonicalize(input)
                        })
                    })
                    .map_err(|_| MeasurementFailure::PrerequisiteUnavailable)??;
                Ok(Measured {
                    duration: measured.duration,
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
