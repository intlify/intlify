// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's record views of what an evaluation records: a unit, a Finding
//! Key, and an evaluation status (D15:2450, 2472-2490). Each view is a closed
//! number-free JSON value, and `C` of it is the record's canonical bytes.
//!
//! A view writes a unit's subject as its Redacted subject token. The tokens of
//! one evaluation are assigned together, over the subject of every unit it
//! recorded, so each collision ordinal is fixed before any view carries it
//! (Decision 015-176). A Finding Key names the Profile Specification, the
//! phase, the check, the code, the subject, and the reason when there is one.
//! It leaves the subject kind out, since each check belongs to one kind. An
//! evaluation status lists its causes, the Finding Keys of the blocking results
//! its prerequisites lacked, exactly when it is not evaluated, in 015's Finding
//! order.
//!
//! Nothing outside the tests takes a view yet: the Findings and statuses of
//! Resolution Evidence come later (plan 025 PRs 5c to 5h).

use serde_json::{json, Map, Value};

use crate::construction::registry::check::{check_id, kind_id};

use super::evaluation::{Evaluation, IssueKey, Status, Unit};
use super::token::{Digest, Specification, Tokens};

/// A resolver bug that leaves a record without its view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewInvariant {
    /// The evaluation recorded no unit over the unit's subject, so the subject
    /// has no token.
    SubjectWithoutToken(Unit),
    /// The unit's subject kind runs no such check, so the check has no ID.
    CheckOutsideKind(Unit),
}

/// The record views of one evaluation, with the token of every subject it
/// recorded a unit over.
#[derive(Debug)]
pub(crate) struct RecordViews<'a> {
    specification: Specification<'a>,
    tokens: Tokens,
}

impl<'a> RecordViews<'a> {
    /// Assigns, under `digest`, the token of the subject of every unit
    /// `evaluation` recorded.
    pub(crate) fn assign(
        specification: Specification<'a>,
        evaluation: &Evaluation,
        digest: &impl Digest,
    ) -> Self {
        let subjects = evaluation.statuses().map(|(unit, _)| &unit.subject);
        Self {
            specification,
            tokens: Tokens::assign(specification, subjects, digest),
        }
    }

    /// The view of `unit`: `{ phase, subjectKind, subject, check }`.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves `unit` without a view.
    pub(crate) fn unit(&self, unit: &Unit) -> Result<Value, ViewInvariant> {
        Ok(json!({
            "phase": unit.phase.id(),
            "subjectKind": kind_id(unit.subject.kind()),
            "subject": self.subject(unit)?,
            "check": check(unit)?,
        }))
    }

    /// The view of the Finding Key `key`:
    /// `{ specification: { identity, revision }, phase, check, code, subject,
    /// reason? }`.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the unit of `key` without a view.
    pub(crate) fn finding_key(&self, key: &IssueKey) -> Result<Value, ViewInvariant> {
        let unit = key.unit();
        let mut view = Map::new();
        view.insert(
            "specification".to_owned(),
            json!({
                "identity": self.specification.identity,
                "revision": self.specification.revision,
            }),
        );
        view.insert("phase".to_owned(), json!(unit.phase.id()));
        view.insert("check".to_owned(), json!(check(unit)?));
        view.insert("code".to_owned(), json!(key.code().as_str()));
        view.insert("subject".to_owned(), self.subject(unit)?);
        if let Some(reason) = key.reason() {
            view.insert("reason".to_owned(), json!(reason.as_str()));
        }
        Ok(Value::Object(view))
    }

    /// The view of `unit`'s evaluation status, `{ unit, state, causes? }`,
    /// with causes exactly when it is not evaluated.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves `unit` or a cause without a view.
    pub(crate) fn status(&self, unit: &Unit, status: &Status) -> Result<Value, ViewInvariant> {
        let mut view = Map::new();
        view.insert("unit".to_owned(), self.unit(unit)?);
        match status {
            Status::Evaluated => {
                view.insert("state".to_owned(), json!("evaluated"));
            }
            Status::NotEvaluated(causes) => {
                view.insert("state".to_owned(), json!("not-evaluated"));
                let causes = causes
                    .keys()
                    .map(|key| self.finding_key(key))
                    .collect::<Result<Vec<_>, _>>()?;
                view.insert("causes".to_owned(), Value::Array(causes));
            }
        }
        Ok(Value::Object(view))
    }

    /// The status view of every unit `evaluation` recorded, in canonical unit
    /// order.
    ///
    /// # Errors
    ///
    /// Returns the first resolver bug that leaves a status without a view.
    pub(crate) fn statuses(&self, evaluation: &Evaluation) -> Result<Vec<Value>, ViewInvariant> {
        evaluation
            .statuses()
            .map(|(unit, status)| self.status(unit, status))
            .collect()
    }

    /// The Finding Key view of every result `evaluation` found, blocking or
    /// not, in 015's Finding order.
    ///
    /// # Errors
    ///
    /// Returns the first resolver bug that leaves a key without a view.
    pub(crate) fn finding_keys(
        &self,
        evaluation: &Evaluation,
    ) -> Result<Vec<Value>, ViewInvariant> {
        let mut keys: Vec<&IssueKey> = evaluation.issues().chain(evaluation.notices()).collect();
        keys.sort();
        keys.into_iter().map(|key| self.finding_key(key)).collect()
    }

    /// The token view of `unit`'s subject.
    fn subject(&self, unit: &Unit) -> Result<Value, ViewInvariant> {
        self.tokens
            .token(self.specification, &unit.subject)
            .map(|token| token.view())
            .ok_or_else(|| ViewInvariant::SubjectWithoutToken(unit.clone()))
    }
}

/// The ID of `unit`'s check.
fn check(unit: &Unit) -> Result<&'static str, ViewInvariant> {
    check_id(unit.subject.kind(), unit.check)
        .ok_or_else(|| ViewInvariant::CheckOutsideKind(unit.clone()))
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::encoding::{digest_bytes, encode};
    use intlify_shared_json::token::hex;
    use serde_json::{json, Value};

    use crate::resolver::evaluation::{
        and, Admitted, Blocking, Check, Code, Evaluation, Phase, Reason, Subject, Unit,
    };
    use crate::resolver::locales::fixtures::{content, key};
    use crate::resolver::locales::{LocaleKey, LocaleSubject};
    use crate::resolver::token::{Digest, Sha256, Specification};

    use super::{RecordViews, ViewInvariant};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    const VOCABULARY: &str =
        "sha256:33db40b6ca29583cfa4f0593235495fbe2246d5e162654e5b18e7f792706c1c5";
    const EN: &str = "sha256:e35e4a12d32518f6b6381c1e067c9a72413635878e1d45cea653102676711227";
    const REJECTED: &str =
        "sha256:ac7304d853d1a86b4cb42389ca5415bbe64ea2c7d2fbbc1422f84d0b38d06b4c";

    /// A digest under which every record collides.
    struct Constant;

    impl Digest for Constant {
        fn digest(&self, _: &[u8]) -> [u8; 32] {
            [7; 32]
        }
    }

    fn vocabulary() -> Unit {
        Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Vocabulary,
            check: Check::IntactArtifact,
        }
    }

    fn requested(key: LocaleKey, check: Check) -> Unit {
        Unit {
            phase: Phase::LocaleResolution,
            subject: Subject::Locale(LocaleSubject::RequestedLocale(key)),
            check,
        }
    }

    /// Records `unit` as finding `code` with `reason`, and returns what waits
    /// on its result.
    fn block(
        evaluation: &mut Evaluation,
        unit: Unit,
        code: Code,
        reason: Option<Reason>,
    ) -> Admitted<()> {
        let found = Blocking {
            code,
            reason,
            details: None,
        };
        evaluation
            .check(unit, Ok(()), |()| Err::<(), _>(found))
            .unwrap()
    }

    /// Records `unit` as finding nothing.
    fn pass(evaluation: &mut Evaluation, unit: Unit) {
        evaluation
            .check(unit, Ok(()), Ok::<(), _>)
            .unwrap()
            .unwrap();
    }

    /// The vocabulary's body is not intact and the requested locale `x!` is
    /// invalid, so whether `x!` is unique waits on both. `en` is canonical.
    fn waiting() -> Evaluation {
        let mut evaluation = Evaluation::default();
        let vocabulary = block(
            &mut evaluation,
            vocabulary(),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::BodyIntegrityInvalid),
        );
        let invalid = block(
            &mut evaluation,
            requested(content("x!"), Check::CanonicalLocale),
            Code::LocaleInvalid,
            None,
        );
        // The causes arrive in the reverse of their Finding order.
        evaluation
            .check(
                requested(content("x!"), Check::UniqueLocale),
                and(invalid, vocabulary),
                Ok::<(), _>,
            )
            .unwrap()
            .unwrap_err();
        pass(
            &mut evaluation,
            requested(key("en"), Check::CanonicalLocale),
        );
        evaluation
    }

    fn unit_view(phase: &str, kind: &str, token: &str, check: &str) -> Value {
        json!({
            "phase": phase,
            "subjectKind": kind,
            "subject": { "token": token },
            "check": check,
        })
    }

    fn vocabulary_key() -> Value {
        json!({
            "specification": { "identity": "intlify-project-profile", "revision": "0" },
            "phase": "specification-admission",
            "check": "specification.vocabulary.intact",
            "code": "project-profile-intent-surface-class-vocabulary-not-admitted",
            "subject": { "token": VOCABULARY },
            "reason": "body-integrity-invalid",
        })
    }

    fn invalid_key() -> Value {
        json!({
            "specification": { "identity": "intlify-project-profile", "revision": "0" },
            "phase": "locale-resolution",
            "check": "locale.requested.canonical",
            "code": "project-profile-locale-invalid",
            "subject": { "token": REJECTED },
        })
    }

    /// Pinned against SHA-256 and `C` computed apart from this crate.
    #[test]
    fn views_match_vectors_computed_independently() {
        let evaluation = waiting();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let statuses = views.statuses(&evaluation).unwrap();
        let evaluated = json!({
            "unit": unit_view(
                "locale-resolution",
                "requested-locale",
                EN,
                "locale.requested.canonical",
            ),
            "state": "evaluated",
        });
        let waiting = json!({
            "unit": unit_view(
                "locale-resolution",
                "requested-locale",
                REJECTED,
                "locale.requested.unique",
            ),
            "state": "not-evaluated",
            "causes": [vocabulary_key(), invalid_key()],
        });
        assert_eq!(statuses[1], evaluated);
        assert_eq!(statuses[3], waiting);
        for (view, length, digest) in [
            (
                vocabulary_key(),
                487,
                "cdb4c617137f1e0171d6a4c17205410c6cc844d5ab9bda0a964b4ddc7df50d71",
            ),
            (
                invalid_key(),
                400,
                "3ccfd49c7dc131e8aef23d37142830976c819a58c9d3b1e0e42ad72cbcc43eaf",
            ),
            (
                waiting,
                1260,
                "7d9b61783c4d4eff74f21aa56c6b63d600784b7745758492fb226975405cf5db",
            ),
            (
                evaluated,
                340,
                "24d016ae5f42c902ddc7252d4d328d6ecbd2b968559f979f1f8a6bc89957d94b",
            ),
        ] {
            let bytes = encode(&view).unwrap();
            assert_eq!(
                (bytes.len(), hex(digest_bytes(&bytes))),
                (length, digest.to_owned()),
                "{view}"
            );
        }
    }

    #[test]
    fn a_status_has_causes_only_when_not_evaluated_and_in_finding_order() {
        let evaluation = waiting();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        assert_eq!(
            views.statuses(&evaluation).unwrap(),
            [
                json!({
                    "unit": unit_view(
                        "specification-admission",
                        "vocabulary",
                        VOCABULARY,
                        "specification.vocabulary.intact",
                    ),
                    "state": "evaluated",
                }),
                json!({
                    "unit": unit_view(
                        "locale-resolution",
                        "requested-locale",
                        EN,
                        "locale.requested.canonical",
                    ),
                    "state": "evaluated",
                }),
                json!({
                    "unit": unit_view(
                        "locale-resolution",
                        "requested-locale",
                        REJECTED,
                        "locale.requested.canonical",
                    ),
                    "state": "evaluated",
                }),
                json!({
                    "unit": unit_view(
                        "locale-resolution",
                        "requested-locale",
                        REJECTED,
                        "locale.requested.unique",
                    ),
                    "state": "not-evaluated",
                    "causes": [vocabulary_key(), invalid_key()],
                }),
            ]
        );
    }

    #[test]
    fn finding_keys_order_blocking_and_non_blocking_results_together() {
        let mut evaluation = Evaluation::default();
        block(
            &mut evaluation,
            requested(content("x!"), Check::CanonicalLocale),
            Code::LocaleInvalid,
            None,
        )
        .unwrap_err();
        let en = requested(key("en"), Check::CanonicalLocale);
        pass(&mut evaluation, en.clone());
        evaluation.notice(en, Code::LocaleNonCanonical).unwrap();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let mut non_canonical = invalid_key();
        non_canonical["code"] = json!("project-profile-locale-non-canonical");
        non_canonical["subject"] = json!({ "token": EN });
        // An identity orders before content, whichever result blocks.
        assert_eq!(
            views.finding_keys(&evaluation).unwrap(),
            [non_canonical, invalid_key()]
        );
    }

    #[test]
    fn colliding_subjects_carry_their_ordinals_in_every_view() {
        let mut evaluation = Evaluation::default();
        pass(&mut evaluation, vocabulary());
        pass(
            &mut evaluation,
            requested(key("en"), Check::CanonicalLocale),
        );
        pass(
            &mut evaluation,
            requested(key("ja"), Check::CanonicalLocale),
        );
        block(
            &mut evaluation,
            requested(key("ja"), Check::UniqueLocale),
            Code::LocaleDuplicate,
            None,
        )
        .unwrap_err();
        let views = RecordViews::assign(PROFILE, &evaluation, &Constant);
        let constant = format!("sha256:{}", "07".repeat(32));
        let subjects: Vec<Value> = views
            .statuses(&evaluation)
            .unwrap()
            .into_iter()
            .map(|status| status["unit"]["subject"].clone())
            .collect();
        // The vocabulary is alone in its kind; en's record orders before ja's.
        assert_eq!(
            subjects,
            [
                json!({ "token": constant }),
                json!({ "token": constant, "collision": "0" }),
                json!({ "token": constant, "collision": "1" }),
                json!({ "token": constant, "collision": "1" }),
            ]
        );
        assert_eq!(
            views.finding_keys(&evaluation).unwrap()[0]["subject"],
            json!({ "token": constant, "collision": "1" })
        );
    }

    #[test]
    fn a_unit_over_an_unrecorded_subject_or_a_foreign_check_has_no_view() {
        let mut evaluation = Evaluation::default();
        pass(&mut evaluation, vocabulary());
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let unrecorded = requested(key("en"), Check::CanonicalLocale);
        assert_eq!(
            views.unit(&unrecorded),
            Err(ViewInvariant::SubjectWithoutToken(unrecorded))
        );
        let foreign = Unit {
            check: Check::CanonicalLocale,
            ..vocabulary()
        };
        assert_eq!(
            views.unit(&foreign),
            Err(ViewInvariant::CheckOutsideKind(foreign))
        );
    }
}
