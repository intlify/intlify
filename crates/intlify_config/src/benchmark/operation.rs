// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Prepared owner operations. Only actual ordinary core functions run between
//! markers; setup, dispatch, observation encoding, and destruction remain outside.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::fixtures::{FixtureConfig, FixturePolicyReference, FixtureTargetReference};
use crate::input_limits::InputLimits;
use crate::materialize::{materialize_file, MaterializationError, MaterializedDocument};
use crate::structural::selection::{
    InvalidSelectorType, Selection, SelectionFailure, SelectionInvariant, SelectionPrerequisite,
    SelectorInput,
};
use crate::structural::{
    AdmissionInvariant, AuthoringSchema, StructuralAnalysis, StructuralLimits,
};

use super::clock::Clock;
use super::locale;
use super::measure::{measure, Measured, MeasurementFailure};
use super::observation::{self, Frame, Observation};

pub(super) type Analysis = StructuralAnalysis<FixturePolicyReference, FixtureTargetReference>;
pub(super) type Schema = AuthoringSchema<FixturePolicyReference, FixtureTargetReference>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Operation {
    FileMaterialization,
    StructuralAnalysis,
    AuthoringConstruction,
    ProfileSelection,
    LocaleCanonicalization,
}

impl Operation {
    pub(super) const ALL: [Self; 5] = [
        Self::FileMaterialization,
        Self::StructuralAnalysis,
        Self::AuthoringConstruction,
        Self::ProfileSelection,
        Self::LocaleCanonicalization,
    ];
    pub(super) const fn phase(self) -> &'static str {
        match self {
            Self::FileMaterialization => "profile_entry_materialize",
            Self::StructuralAnalysis | Self::AuthoringConstruction => "profile_structural_admit",
            Self::ProfileSelection => "profile_select",
            Self::LocaleCanonicalization => "profile_locale_resolve",
        }
    }
    pub(super) const fn cost(self) -> &'static str {
        match self {
            Self::FileMaterialization => "file_decode_and_source_map",
            Self::StructuralAnalysis => "structural_analysis",
            Self::AuthoringConstruction => "authoring_model_construction",
            Self::ProfileSelection => "named_profile_selection",
            Self::LocaleCanonicalization => "locale_canonicalization",
        }
    }
    pub(super) const fn boundary(self) -> &'static str {
        match self {
            Self::FileMaterialization => "minimum-file-materialization/0",
            Self::StructuralAnalysis => "minimum-structural-analysis/0",
            Self::AuthoringConstruction => "minimum-authoring-construction/0",
            Self::ProfileSelection => "minimum-provisional-selection/0",
            Self::LocaleCanonicalization => "minimum-single-locale-canonicalization/0",
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
}

pub(super) enum Output {
    Entry(Result<MaterializedDocument, MaterializationError>),
    Structural(Result<Analysis, AdmissionInvariant>),
    Authoring(Result<Option<FixtureConfig>, AdmissionInvariant>),
    Select(Result<Selection<FixturePolicyReference>, SelectionInvariant>),
    Locale(Result<crate::locale::Canonicalized, crate::locale::CanonicalizationFailure>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OutputFailure {
    AdmissionInvariant,
    SelectionInvariant,
    MissingCompleteRoot,
    ObservationEncoding,
    LocaleProviderUnavailable,
    LocaleProviderInvariant,
}

impl Prepared {
    pub(super) const fn operation(&self) -> Operation {
        match self {
            Self::Entry { .. } => Operation::FileMaterialization,
            Self::Structural { .. } => Operation::StructuralAnalysis,
            Self::Authoring(_) => Operation::AuthoringConstruction,
            Self::Select { .. } => Operation::ProfileSelection,
            Self::Locale { .. } => Operation::LocaleCanonicalization,
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
        }
    }

    pub(super) fn prerequisites_admitted(&self) -> bool {
        match self {
            Self::Authoring(analysis) => analysis.is_complete(),
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
        }
    }
}

fn observe_selection(
    selection: &Selection<FixturePolicyReference>,
) -> Result<Observation, OutputFailure> {
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
