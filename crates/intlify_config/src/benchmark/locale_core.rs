// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite owner observations for project source/requested/default only. Neither
//! the private core nor this observation is a Profile or a shared artifact.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::operation::Config;
use crate::locale::core::{Failure, Input, Issue, Limits, Location, Resolution};
use crate::locale::{CanonicalizationFailure, ProviderFailure, Spelling};
use crate::model::ProfileId;

use super::locale;
use super::observation::{Frame, Observation};
use super::operation::{Analysis, OutputFailure};
use super::quantity::Quantity;

pub(super) struct PreparedCore {
    pub(super) analysis: Analysis,
    pub(super) config: Config,
    pub(super) selected: ProfileId,
    pub(super) provider: Arc<locale::Core>,
    pub(super) limits: Limits,
}

impl PreparedCore {
    pub(super) fn input(&self) -> Option<Input<'_>> {
        if !self.analysis.is_complete() {
            return None;
        }
        Input::from_selected(&self.config, &self.selected)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct InputFacts {
    scope: String,
    selected_profile: String,
    active_occurrence_limit: Quantity,
    active_occurrence_domain: [String; 3],
    requested_locale_limit: Quantity,
    requested_cardinality_domain: String,
    input_storage: String,
    structural_preparation: String,
    ordinary_output: String,
}

impl InputFacts {
    pub(super) fn observe(prepared: &PreparedCore) -> Self {
        Self {
            scope: "minimum-project-source-requested-default-only".into(),
            selected_profile: prepared.selected.as_str().into(),
            active_occurrence_limit: Quantity::new(prepared.limits.max_active_occurrences.get()),
            active_occurrence_domain: [
                "source-default-when-present".into(),
                "every-project-requested-occurrence".into(),
                "required-project-requested-default".into(),
            ],
            requested_locale_limit: Quantity::new(prepared.limits.max_requested_locales.get()),
            requested_cardinality_domain: "unique-valid-canonical-project-requested-locales".into(),
            input_storage: "borrowed-selected-fields-of-resident-complete-config".into(),
            structural_preparation: "complete-model-selection-and-borrowed-view-before-interval"
                .into(),
            ordinary_output: "private-core-or-private-failure-with-corrections-and-counts".into(),
        }
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum CanonicalFailureKey {
    ByteLimit {
        spelling: u8,
        limit: u64,
        actual: u64,
    },
    InvalidIdentifier,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum SemanticIssueKey<'a> {
    Canonicalization {
        role: u8,
        reason: CanonicalFailureKey,
    },
    Duplicate {
        canonical: &'a str,
        multiplicity: u64,
    },
    RequestedLimit {
        limit: u64,
        actual: u64,
    },
    DefaultNotRequested {
        canonical: &'a str,
    },
}

fn count(value: usize) -> Result<u64, OutputFailure> {
    u64::try_from(value).map_err(|_| OutputFailure::LocaleCoreInvariant)
}

fn role(location: Location) -> u8 {
    match location {
        Location::SourceDefault => 0,
        Location::Requested(_) => 1,
        Location::RequestedDefault => 2,
    }
}

fn key(issue: &Issue) -> Result<SemanticIssueKey<'_>, OutputFailure> {
    Ok(match issue {
        Issue::Canonicalization { location, reason } => {
            let reason = match reason {
                CanonicalizationFailure::ByteLimit {
                    spelling,
                    limit,
                    actual,
                } => CanonicalFailureKey::ByteLimit {
                    spelling: match spelling {
                        Spelling::Raw => 0,
                        Spelling::Canonical => 1,
                    },
                    limit: limit.get(),
                    actual: *actual,
                },
                CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier) => {
                    CanonicalFailureKey::InvalidIdentifier
                }
                CanonicalizationFailure::Provider(
                    ProviderFailure::UnsupportedInput | ProviderFailure::Unavailable,
                ) => {
                    return Err(OutputFailure::LocaleProviderUnavailable);
                }
                _ => return Err(OutputFailure::LocaleProviderInvariant),
            };
            SemanticIssueKey::Canonicalization {
                role: role(*location),
                reason,
            }
        }
        Issue::Duplicate {
            locale,
            occurrences,
        } => SemanticIssueKey::Duplicate {
            canonical: locale.as_str(),
            multiplicity: count(occurrences.len())?,
        },
        Issue::RequestedLimit { limit, actual } => SemanticIssueKey::RequestedLimit {
            limit: limit.get(),
            actual: *actual,
        },
        Issue::DefaultNotRequested { locale } => SemanticIssueKey::DefaultNotRequested {
            canonical: locale.as_str(),
        },
    })
}

impl SemanticIssueKey<'_> {
    fn frame(&self, frame: &mut Frame) {
        match self {
            Self::Canonicalization { role, reason } => {
                frame.uint(0);
                frame.uint(u64::from(*role));
                match reason {
                    CanonicalFailureKey::ByteLimit {
                        spelling,
                        limit,
                        actual,
                    } => {
                        frame.uint(0);
                        frame.uint(u64::from(*spelling));
                        frame.uint(*limit);
                        frame.uint(*actual);
                    }
                    CanonicalFailureKey::InvalidIdentifier => frame.uint(1),
                }
            }
            Self::Duplicate {
                canonical,
                multiplicity,
            } => {
                frame.uint(1);
                frame.text(canonical);
                frame.uint(*multiplicity);
            }
            Self::RequestedLimit { limit, actual } => {
                frame.uint(2);
                frame.uint(*limit);
                frame.uint(*actual);
            }
            Self::DefaultNotRequested { canonical } => {
                frame.uint(3);
                frame.text(canonical);
            }
        }
    }
}

/// Missing coverage and internal errors are not completed expected failures.
/// Logical work uses this screen too without hashing or encoding the output.
pub(super) fn require_observable(result: &Resolution) -> Result<(), OutputFailure> {
    match result.value() {
        Err(Failure::AccountingOverflow) => return Err(OutputFailure::LocaleCoreInvariant),
        Err(Failure::Issues(issues)) => {
            for issue in issues {
                key(issue)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn location(frame: &mut Frame, value: Location) -> Result<(), OutputFailure> {
    frame.uint(u64::from(role(value)));
    if let Location::Requested(index) = value {
        frame.uint(count(index)?);
    }
    Ok(())
}

pub(super) fn observe(result: &Resolution) -> Result<Observation, OutputFailure> {
    require_observable(result)?;
    let mut shared = Frame::new("minimum-project-locale-core-result");
    let mut entry = Frame::new("minimum-project-locale-core-entry");
    match result.value() {
        Ok(core) => {
            shared.uint(0);
            entry.uint(0);
            shared.flag(core.source_default().is_some());
            if let Some(source) = core.source_default() {
                shared.text(source.as_str());
            }
            shared.uint(count(core.requested().len())?);
            for locale in core.requested() {
                shared.text(locale.as_str());
            }
            shared.text(core.requested_default().as_str());
        }
        Err(Failure::OccurrenceLimit { limit, actual }) => {
            shared.uint(1);
            shared.uint(limit.get());
            shared.uint(*actual);
            entry.uint(1);
        }
        Err(Failure::Issues(issues)) => {
            shared.uint(2);
            entry.uint(2);
            shared.uint(count(issues.len())?);
            entry.uint(count(issues.len())?);
            let mut keys = issues.iter().map(key).collect::<Result<Vec<_>, _>>()?;
            // Sort semantic fields, never their hashes or source indices. Keep
            // multiplicity, while permitting set-authoring permutations.
            keys.sort_unstable();
            for key in keys {
                key.frame(&mut shared);
            }
            for issue in issues {
                key(issue)?.frame(&mut entry);
                match issue {
                    Issue::Canonicalization {
                        location: value, ..
                    } => location(&mut entry, *value)?,
                    Issue::Duplicate { occurrences, .. } => {
                        for index in occurrences {
                            entry.uint(count(*index)?);
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(Failure::AccountingOverflow) => return Err(OutputFailure::LocaleCoreInvariant),
    }
    for count in [
        result.counts().active_occurrences,
        result.counts().canonical_requested,
    ] {
        shared.flag(count.is_some());
        if let Some(count) = count {
            shared.uint(count);
        }
    }
    entry.uint(count(result.corrections().len())?);
    for correction in result.corrections() {
        location(&mut entry, correction.location)?;
        entry.text(correction.replacement.as_str());
    }
    Ok(Observation {
        shared: shared.finish(),
        entry: Some(entry.finish()),
    })
}
