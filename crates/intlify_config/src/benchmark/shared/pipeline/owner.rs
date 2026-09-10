// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exact native adapter selected before common projection. Missing, unsupported,
//! malformed, and self-rehashed altered owner documents remain distinct inputs.

use crate::benchmark::run::{CheckedOwnerRecord, OwnerRecord, RecordedRun, RunIssue};
use crate::benchmark::shared::decode;
use crate::benchmark::shared::measurement::{
    ExpectedOwner, InputResolution, InputState, UnavailableKind,
};
use crate::benchmark::shared::reason::{CommonCode, Detail, Reason, Selector, Stage};
use crate::benchmark::shared::record::Reference;

pub(super) struct OwnerInput {
    pub(super) checked: Option<CheckedOwnerRecord>,
    pub(super) resolution: InputResolution,
    pub(super) absence: Option<UnavailableKind>,
    pub(super) failure_code: Option<CommonCode>,
}

impl OwnerInput {
    pub(super) fn resolve(run: &RecordedRun, inputs: &[&[u8]]) -> Self {
        let expected = ExpectedOwner::from_plan(run.plan_record(), run.expected_owner_identity());
        let selector = || Selector::Record {
            reference: Reference::top(&expected.record_identity),
        };
        let error = |code: CommonCode,
                     detail: Detail,
                     absence: Option<UnavailableKind>,
                     submitted_input: Option<Reference>| {
            let reasons = vec![Reason::new(code, Stage::InputAdmission, selector(), detail)];
            let result = if absence.is_some() {
                InputState::Unavailable {
                    source_evaluations: Vec::new(),
                    reasons,
                }
            } else {
                InputState::Invalid {
                    submitted_input,
                    reasons,
                }
            };
            Self {
                checked: None,
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
            return error(
                CommonCode::AmbiguousBinding,
                Detail::DuplicateInput {},
                None,
                None,
            );
        }
        let Ok(value) = decode::value(inputs[0], false) else {
            return error(
                CommonCode::SchemaInvalid,
                Detail::InvalidInput {},
                None,
                None,
            );
        };
        match value
            .pointer("/result/codec")
            .and_then(serde_json::Value::as_str)
        {
            Some("intlify-config-owner-run-result/1") => {}
            Some(_) => {
                return error(
                    CommonCode::UnsupportedMeasurement,
                    Detail::UnsupportedTuple {},
                    Some(UnavailableKind::Unsupported),
                    None,
                )
            }
            None => {
                return error(
                    CommonCode::SchemaInvalid,
                    Detail::InvalidInput {},
                    None,
                    None,
                )
            }
        }
        let Ok(document) = decode::typed::<OwnerRecord>(value) else {
            return error(
                CommonCode::SchemaInvalid,
                Detail::InvalidInput {},
                None,
                None,
            );
        };
        let submitted = Some(Reference::top(&document.result().record_identity));
        if document.result().record_identity != expected.record_identity
            || document.plan_reference() != run.plan_record().identity()
        {
            return error(
                CommonCode::InconsistentRecord,
                Detail::BindingMismatch {},
                None,
                submitted,
            );
        }
        match run.admit_owned(document) {
            Ok(checked) => Self {
                checked: Some(checked),
                resolution: InputResolution {
                    result: InputState::Resolved {
                        reference: Reference::top(&expected.record_identity),
                    },
                    expected,
                },
                absence: None,
                failure_code: None,
            },
            Err(issues) if issues.contains(&RunIssue::Integrity) => error(
                CommonCode::IntegrityDigestMismatch,
                Detail::InvalidInput {},
                None,
                submitted,
            ),
            Err(_) => error(
                CommonCode::InconsistentRecord,
                Detail::BindingMismatch {},
                None,
                submitted,
            ),
        }
    }
}
