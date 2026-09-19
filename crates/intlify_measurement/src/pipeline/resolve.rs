// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Selecting the owner document this run's records are projected from.
//!
//! Missing, unsupported, malformed, and self-rehashed altered documents remain
//! distinct inputs. None of them is a measured result, and none of them lets
//! the pipeline continue as though a document had resolved.

use crate::measurement::{InputResolution, InputState, UnavailableKind};
use crate::owner::{OwnerRun, Rejection};
use crate::reason::{CommonCode, Detail, Reason, Selector, Stage};
use crate::record::Reference;

/// Which owner document resolved, and what to say when none did.
pub(crate) struct Resolved<O: OwnerRun> {
    pub(crate) admitted: Option<O::Admitted>,
    pub(crate) resolution: InputResolution,
    pub(crate) absence: Option<UnavailableKind>,
    pub(crate) failure_code: Option<CommonCode>,
}

pub(crate) fn resolve<O: OwnerRun>(run: &O, inputs: &[&[u8]]) -> Resolved<O> {
    let expected = run.expected();
    let selector = || Selector::Record {
        reference: Reference::top(&expected.record_identity),
    };
    let error = |code: CommonCode,
                 detail: Detail,
                 absence: Option<UnavailableKind>,
                 submitted: Option<Reference>| {
        let reasons = vec![Reason::new(code, Stage::InputAdmission, selector(), detail)];
        let result = if absence.is_some() {
            InputState::Unavailable {
                source_evaluations: Vec::new(),
                reasons,
            }
        } else {
            InputState::Invalid {
                submitted_input: submitted,
                reasons,
            }
        };
        Resolved {
            admitted: None,
            resolution: InputResolution {
                expected: expected.clone(),
                result,
            },
            absence,
            failure_code: Some(code),
        }
    };

    if inputs.is_empty() {
        return error(
            CommonCode::MissingEvidence,
            Detail::MissingInput {},
            Some(UnavailableKind::Missing),
            None,
        );
    }
    if inputs.len() != 1 {
        // Two documents for one run is not twice the evidence: it is an
        // ambiguous binding, and neither one is selected.
        return error(
            CommonCode::AmbiguousBinding,
            Detail::DuplicateInput {},
            None,
            None,
        );
    }
    match run.admit(inputs[0]) {
        Ok(admitted) => {
            let resolution = InputResolution {
                result: InputState::Resolved {
                    reference: Reference::top(&expected.record_identity),
                },
                expected,
            };
            Resolved {
                admitted: Some(admitted),
                resolution,
                absence: None,
                failure_code: None,
            }
        }
        Err(Rejection::Unreadable) => error(
            CommonCode::SchemaInvalid,
            Detail::InvalidInput {},
            None,
            None,
        ),
        Err(Rejection::UnsupportedCodec) => error(
            CommonCode::UnsupportedMeasurement,
            Detail::UnsupportedTuple {},
            Some(UnavailableKind::Unsupported),
            None,
        ),
        Err(Rejection::Integrity(submitted)) => error(
            CommonCode::IntegrityDigestMismatch,
            Detail::InvalidInput {},
            None,
            Some(Reference::top(&submitted)),
        ),
        Err(Rejection::Binding(submitted)) => error(
            CommonCode::InconsistentRecord,
            Detail::BindingMismatch {},
            None,
            submitted.map(|identity| Reference::top(&identity)),
        ),
    }
}
