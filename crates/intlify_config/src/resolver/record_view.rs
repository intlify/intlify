// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's record views of what an evaluation records: a unit, a Finding
//! Key, a Finding, and an evaluation status (D15:2450, 2472-2492). Each view
//! is a closed number-free JSON value, and `C` of it is the record's canonical
//! bytes.
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
//! A Finding adds its code's severity and blocking state, as the Finding
//! Registry fixes them, details exactly when the code's payload strategy gives
//! them a member, and a suggestion exactly when the code's suggestion kind is
//! not `none` (Decision 015-181). Its key determines the details: the reason,
//! and the slot, component, or subject kind of the subject, come from the key,
//! and only a bound and its counts come with the result, equal for every
//! candidate of the key (D15:2381). A suggestion is a single-member object
//! keyed by its kind (Decision 015-180).
//!
//! Nothing outside the tests takes a view yet: the Findings and statuses of
//! Resolution Evidence come later (plan 025 PRs 5d to 5h).

use serde_json::{json, Map, Value};

use crate::construction::registry::check::{check_id, details_members, kind_id, DetailsMember};
use crate::construction::registry::finding::{blocking, severity, suggestion_kind, SuggestionKind};
use crate::input_limits::Bound;

use super::capability::CapabilityBound;
use super::evaluation::{
    Actual, Details, Evaluation, Finding, IssueKey, Status, Subject, Suggestion, Unit,
};
use super::specification::Slot;
use super::token::{Digest, Specification, Tokens};

/// A resolver bug that leaves a record without its view.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ViewInvariant {
    /// The evaluation recorded no unit over the unit's subject, so the subject
    /// has no token.
    SubjectWithoutToken(Unit),
    /// The unit's subject kind runs no such check, so the check has no ID.
    CheckOutsideKind(Unit),
    /// The code's payload strategy gives its details a member the result has
    /// no value for.
    MemberWithoutValue(IssueKey, DetailsMember),
    /// The result suggests nothing although its code's suggestion kind is not
    /// `none`, or suggests something of another kind.
    SuggestionOutsideKind(IssueKey),
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
        evaluation
            .findings()
            .iter()
            .map(|finding| self.finding_key(finding.key))
            .collect()
    }

    /// The view of `finding`: `{ key, severity, blocking, details?,
    /// suggestion? }`.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves `finding` without a view.
    pub(crate) fn finding(&self, finding: &Finding<'_>) -> Result<Value, ViewInvariant> {
        let code = finding.key.code();
        let mut view = Map::new();
        view.insert("key".to_owned(), self.finding_key(finding.key)?);
        view.insert("severity".to_owned(), json!(severity(code)));
        view.insert("blocking".to_owned(), json!(blocking(code)));
        if let Some(details) = details(finding)? {
            view.insert("details".to_owned(), details);
        }
        if let Some(suggestion) = suggestion(finding)? {
            view.insert("suggestion".to_owned(), suggestion);
        }
        Ok(Value::Object(view))
    }

    /// The Finding view of every result `evaluation` found, blocking or not,
    /// in 015's Finding order.
    ///
    /// # Errors
    ///
    /// Returns the first resolver bug that leaves a Finding without a view.
    pub(crate) fn findings(&self, evaluation: &Evaluation) -> Result<Vec<Value>, ViewInvariant> {
        evaluation
            .findings()
            .iter()
            .map(|finding| self.finding(finding))
            .collect()
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

/// The view of `finding`'s details: each member its code's payload strategy
/// gives them, or none when it gives none.
fn details(finding: &Finding<'_>) -> Result<Option<Value>, ViewInvariant> {
    let members = details_members(finding.key.code());
    if members.is_empty() {
        return Ok(None);
    }
    let mut view = Map::new();
    for &member in members {
        let value = member_value(finding, member)
            .ok_or_else(|| ViewInvariant::MemberWithoutValue(finding.key.clone(), member))?;
        view.insert(member_name(member), value);
    }
    Ok(Some(Value::Object(view)))
}

/// The value of `member` in `finding`'s details, from its key or from the
/// bound the result carries, or none when the finding has none. A count is
/// its decimal string. A structural code's field arrives with the structural
/// Findings (plan 025 PR 5f-2), so no result here has one.
fn member_value(finding: &Finding<'_>, member: DetailsMember) -> Option<Value> {
    use DetailsMember as M;
    let key = finding.key;
    let subject = &key.unit().subject;
    let value = match (member, finding.details) {
        (M::Reason, _) => key.reason()?.as_str().to_owned(),
        (M::Slot, _) => match subject {
            Subject::Slot(slot) => slot.name().to_owned(),
            _ => return None,
        },
        (M::Component, _) => match subject {
            Subject::Slot(slot @ (Slot::DeliveryGraph | Slot::DeliveryPlacement)) => {
                slot.name().to_owned()
            }
            _ => return None,
        },
        (M::Subject, _) => kind_id(subject.kind()),
        (M::ResourceGroup, Some(Details::Limit(exceeded))) => {
            exceeded.bound.group.path().to_owned()
        }
        (M::ResourceGroup, Some(Details::Shortfall(shortfall))) => {
            shortfall.bound.group.path().to_owned()
        }
        (M::Bound, Some(Details::Limit(exceeded))) => exceeded.bound.name.to_owned(),
        (M::Bound, Some(Details::Shortfall(shortfall))) => shortfall.bound.name.to_owned(),
        // A capability bound's path names its namespace and group.
        (M::Bound, Some(Details::BootstrapLimit(exceeded))) => {
            let bound = CapabilityBound::BootstrapOnly(exceeded.bound);
            format!("{}.{}", bound.namespace(), bound.path())
        }
        (M::Limit, details) => measured(details)?.0.get().to_string(),
        (M::Actual, details) => match measured(details)?.1 {
            Actual::Exact(total) | Actual::AtLeast(total) => total.to_string(),
        },
        (M::ActualRelation, details) => match measured(details)?.1 {
            Actual::Exact(_) => "exact",
            Actual::AtLeast(_) => "at-least",
        }
        .to_owned(),
        (M::Required, Some(Details::Shortfall(shortfall))) => shortfall.required.get().to_string(),
        (M::Available, Some(Details::Shortfall(shortfall))) => {
            shortfall.available.get().to_string()
        }
        _ => return None,
    };
    Some(Value::String(value))
}

/// The bound's value and the measured total of a result past a bound.
fn measured(details: Option<&Details>) -> Option<(Bound, Actual)> {
    match details? {
        Details::Limit(exceeded) => Some((exceeded.limit, exceeded.actual)),
        Details::BootstrapLimit(exceeded) => Some((exceeded.limit, exceeded.actual)),
        Details::Shortfall(_) => None,
    }
}

/// A member's name as 015 spells it.
fn member_name(member: DetailsMember) -> String {
    match serde_json::to_value(member) {
        Ok(Value::String(name)) => name,
        _ => unreachable!("a member serializes as its name"),
    }
}

/// The view of `finding`'s suggestion, a single-member object keyed by its
/// kind, or none when its code suggests nothing.
fn suggestion(finding: &Finding<'_>) -> Result<Option<Value>, ViewInvariant> {
    let kind = suggestion_kind(finding.key.code());
    match finding.suggestion {
        None if kind == SuggestionKind::None => Ok(None),
        Some(suggestion) if suggestion.kind() == kind => {
            let Suggestion::CanonicalLocaleReplacement(locale) = suggestion;
            let mut view = Map::new();
            view.insert(kind_name(kind), json!(locale.as_str()));
            Ok(Some(Value::Object(view)))
        }
        _ => Err(ViewInvariant::SuggestionOutsideKind(finding.key.clone())),
    }
}

/// A suggestion kind's name as the Finding Registry spells it.
fn kind_name(kind: SuggestionKind) -> String {
    match serde_json::to_value(kind) {
        Ok(Value::String(name)) => name,
        _ => unreachable!("a suggestion kind serializes as its name"),
    }
}

#[cfg(test)]
mod tests {
    use intlify_shared_json::encoding::{digest_bytes, encode};
    use intlify_shared_json::token::hex;
    use serde_json::{json, Value};

    use crate::construction::registry::check::DetailsMember;
    use crate::input_limits::Bound;
    use crate::resolver::capability::{BootstrapBoundId, BootstrapGroup};
    use crate::resolver::evaluation::{
        and, Actual, Admitted, Blocking, BootstrapLimitExceeded, Check, Code, Evaluation, IssueKey,
        LimitExceeded, Notice, Phase, Reason, Shortfall, Subject, Suggestion, Unit,
    };
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::locales::fixtures::{canonical, content, key};
    use crate::resolver::locales::{LocaleKey, LocaleSubject};
    use crate::resolver::resource_policy::{unit as policy, PolicySubject};
    use crate::resolver::specification::Slot;
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
    const COVERAGE: &str =
        "sha256:c42822f442508fbef584d1adeaa194c7d0828e70cb63942f96aae1b7697f3717";
    const REQUESTED_SET: &str =
        "sha256:cea65841b93edba82de912813fbd8cbb43318816cb695c703f2e8304210e81e7";
    const EN_US: &str = "sha256:08a241afab6780d61fc5c2f839853871d92567d275664122d5ce006525bf347b";

    const MAX_REQUESTED_LOCALES: BoundId = BoundId {
        group: Group::LocaleResolution,
        name: "maxRequestedLocales",
    };

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

    fn slot(slot: Slot) -> Unit {
        Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Slot(slot),
            check: Check::SupportedAssertion,
        }
    }

    fn requested_set() -> Unit {
        Unit {
            phase: Phase::LocaleResolution,
            subject: Subject::Locale(LocaleSubject::RequestedLocales),
            check: Check::WithinBound,
        }
    }

    /// Records `unit` as finding the blocking result `found`.
    fn blocked(evaluation: &mut Evaluation, unit: Unit, found: Blocking) {
        evaluation
            .check(unit, Ok(()), |()| Err::<(), _>(found))
            .unwrap()
            .unwrap_err();
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
    fn findings_order_blocking_and_non_blocking_results_together() {
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
        evaluation
            .notice(en, Notice::non_canonical(canonical("en")))
            .unwrap();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let mut non_canonical = invalid_key();
        non_canonical["code"] = json!("project-profile-locale-non-canonical");
        non_canonical["subject"] = json!({ "token": EN });
        // An identity orders before content, whichever result blocks.
        assert_eq!(
            views.finding_keys(&evaluation).unwrap(),
            [non_canonical.clone(), invalid_key()]
        );
        assert_eq!(
            views.findings(&evaluation).unwrap(),
            [
                json!({
                    "key": non_canonical,
                    "severity": "warning",
                    "blocking": false,
                    "suggestion": { "canonical-locale-replacement": "en" },
                }),
                json!({ "key": invalid_key(), "severity": "error", "blocking": true }),
            ]
        );
    }

    fn key_view(phase: &str, check: &str, code: &str, token: &str) -> Value {
        json!({
            "specification": { "identity": "intlify-project-profile", "revision": "0" },
            "phase": phase,
            "check": check,
            "code": code,
            "subject": { "token": token },
        })
    }

    /// Pinned against SHA-256 and `C` computed apart from this crate.
    #[test]
    fn findings_match_vectors_computed_independently() {
        let mut evaluation = Evaluation::default();
        block(
            &mut evaluation,
            slot(Slot::Coverage),
            Code::SpecificationInputIncomplete,
            Some(Reason::MissingRequiredSlot),
        )
        .unwrap_err();
        blocked(
            &mut evaluation,
            requested_set(),
            Blocking::limit(LimitExceeded {
                bound: MAX_REQUESTED_LOCALES,
                limit: Bound::new(2).unwrap(),
                actual: Actual::Exact(3),
            }),
        );
        let us = requested(key("en-US"), Check::CanonicalLocale);
        pass(&mut evaluation, us.clone());
        evaluation
            .notice(us, Notice::non_canonical(canonical("en-US")))
            .unwrap();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let mut missing = key_view(
            "specification-admission",
            "specification.coverage.supported",
            "project-profile-specification-input-incomplete",
            COVERAGE,
        );
        missing["reason"] = json!("missing-required-slot");
        let missing = json!({
            "key": missing,
            "severity": "error",
            "blocking": true,
            "details": { "reason": "missing-required-slot", "slot": "coverage" },
        });
        let exceeded = json!({
            "key": key_view(
                "locale-resolution",
                "locale.requested-set.within-bound",
                "project-profile-resource-limit-exceeded",
                REQUESTED_SET,
            ),
            "severity": "error",
            "blocking": true,
            "details": {
                "subject": "requested-locales",
                "resourceGroup": "localeResolution",
                "bound": "maxRequestedLocales",
                "limit": "2",
                "actual": "3",
                "actualRelation": "exact",
            },
        });
        let non_canonical = json!({
            "key": key_view(
                "locale-resolution",
                "locale.requested.canonical",
                "project-profile-locale-non-canonical",
                EN_US,
            ),
            "severity": "warning",
            "blocking": false,
            "suggestion": { "canonical-locale-replacement": "en-US" },
        });
        let views = views.findings(&evaluation).unwrap();
        assert_eq!(views.len(), 3);
        for (view, length, digest) in [
            (
                missing,
                667,
                "11e64a489a7ef7e8f964aeab31a1642969606f0105ba9d01b6d200460872be61",
            ),
            (
                exceeded,
                752,
                "c2b121a1b1712833f23bcbf8b5b6aea0304025e7c466e23b2b364d5158753c98",
            ),
            (
                non_canonical,
                581,
                "5f6c80c074ac610bbfcb4a2d442868e79056ce174bca670c234421536d914e8c",
            ),
        ] {
            assert!(views.contains(&view), "{view}");
            let bytes = encode(&view).unwrap();
            assert_eq!(
                (bytes.len(), hex(digest_bytes(&bytes))),
                (length, digest.to_owned()),
                "{view}"
            );
        }
    }

    #[test]
    fn a_finding_has_details_exactly_when_its_code_has_members_and_a_suggestion_exactly_when_it_suggests(
    ) {
        let mut evaluation = Evaluation::default();
        blocked(
            &mut evaluation,
            policy(PolicySubject::ResourceLimits, Check::WithinValidationWork),
            Blocking::bootstrap_limit(BootstrapLimitExceeded {
                bound: BootstrapBoundId {
                    group: BootstrapGroup::ArtifactBootstrap,
                    name: "maxValidationWorkUnits",
                },
                limit: Bound::new(5).unwrap(),
                actual: Actual::Exact(9),
            }),
        );
        blocked(
            &mut evaluation,
            policy(PolicySubject::ResourceLimits, Check::CompatiblePolicy),
            Blocking::shortfall(Shortfall {
                bound: MAX_REQUESTED_LOCALES,
                required: Bound::new(9).unwrap(),
                available: Bound::new(8).unwrap(),
            }),
        );
        block(
            &mut evaluation,
            vocabulary(),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::BodyIntegrityInvalid),
        )
        .unwrap_err();
        block(
            &mut evaluation,
            slot(Slot::DeliveryPlacement),
            Code::DeliverySpecificationNotAdmitted,
            Some(Reason::Incompatible),
        )
        .unwrap_err();
        block(
            &mut evaluation,
            requested(content("x!"), Check::CanonicalLocale),
            Code::LocaleInvalid,
            None,
        )
        .unwrap_err();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let shapes: Vec<(Value, Option<Value>, Option<Value>)> = views
            .findings(&evaluation)
            .unwrap()
            .into_iter()
            .map(|view| {
                (
                    view["key"]["code"].clone(),
                    view.get("details").cloned(),
                    view.get("suggestion").cloned(),
                )
            })
            .collect();
        assert_eq!(
            shapes,
            [
                (
                    json!("project-profile-bootstrap-limit-exceeded"),
                    // A capability bound's path names its namespace and group.
                    Some(json!({
                        "bound": "bootstrapOnly.artifactBootstrap.maxValidationWorkUnits",
                        "limit": "5",
                        "actual": "9",
                        "actualRelation": "exact",
                    })),
                    None,
                ),
                (
                    json!("project-profile-resource-policy-capability-incompatible"),
                    Some(json!({
                        "resourceGroup": "localeResolution",
                        "bound": "maxRequestedLocales",
                        "required": "9",
                        "available": "8",
                    })),
                    None,
                ),
                (
                    json!("project-profile-delivery-specification-not-admitted"),
                    Some(json!({ "reason": "incompatible", "component": "delivery-placement" })),
                    None,
                ),
                (
                    json!("project-profile-intent-surface-class-vocabulary-not-admitted"),
                    Some(json!({ "reason": "body-integrity-invalid" })),
                    None,
                ),
                (json!("project-profile-locale-invalid"), None, None),
            ]
        );
    }

    #[test]
    fn a_finding_without_a_value_its_details_need_or_with_a_foreign_suggestion_has_no_view() {
        let mut evaluation = Evaluation::fabricated();
        // A schema code's field arrives only with the structural Findings.
        block(
            &mut evaluation,
            vocabulary(),
            Code::RequiredFieldMissing,
            None,
        )
        .unwrap_err();
        // A slot's code over a subject that is no slot.
        let data = Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::CanonicalizationData,
            check: Check::MatchingData,
        };
        block(
            &mut evaluation,
            data,
            Code::SpecificationInputIncomplete,
            Some(Reason::MissingRequiredSlot),
        )
        .unwrap_err();
        // A delivery code over a slot that names no delivery component.
        block(
            &mut evaluation,
            slot(Slot::Coverage),
            Code::DeliverySpecificationNotAdmitted,
            Some(Reason::Unsupported),
        )
        .unwrap_err();
        // A bound's code without its bound.
        block(
            &mut evaluation,
            requested_set(),
            Code::ResourceLimitExceeded,
            None,
        )
        .unwrap_err();
        // A code that suggests, without a suggestion, and one that suggests
        // nothing, with one.
        let en = requested(key("en"), Check::CanonicalLocale);
        pass(&mut evaluation, en.clone());
        evaluation
            .notice(en, Notice::made_up(Code::LocaleNonCanonical))
            .unwrap();
        let ja = requested(key("ja"), Check::CanonicalLocale);
        pass(&mut evaluation, ja.clone());
        evaluation
            .notice(
                ja,
                Notice {
                    code: Code::LocaleDuplicate,
                    suggestion: Some(Suggestion::CanonicalLocaleReplacement(canonical("ja"))),
                },
            )
            .unwrap();
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let failures: Vec<ViewInvariant> = evaluation
            .findings()
            .iter()
            .map(|finding| views.finding(finding).unwrap_err())
            .collect();
        let keys: Vec<_> = evaluation
            .findings()
            .iter()
            .map(|finding| finding.key.clone())
            .collect();
        assert_eq!(
            failures,
            [
                ViewInvariant::MemberWithoutValue(keys[0].clone(), DetailsMember::Component),
                ViewInvariant::MemberWithoutValue(keys[1].clone(), DetailsMember::Field),
                ViewInvariant::MemberWithoutValue(keys[2].clone(), DetailsMember::Slot),
                ViewInvariant::SuggestionOutsideKind(keys[3].clone()),
                ViewInvariant::SuggestionOutsideKind(keys[4].clone()),
                ViewInvariant::MemberWithoutValue(keys[5].clone(), DetailsMember::ResourceGroup),
            ]
        );
        assert_eq!(
            keys.iter().map(IssueKey::code).collect::<Vec<_>>(),
            [
                Code::DeliverySpecificationNotAdmitted,
                Code::RequiredFieldMissing,
                Code::SpecificationInputIncomplete,
                Code::LocaleNonCanonical,
                Code::LocaleDuplicate,
                Code::ResourceLimitExceeded,
            ]
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
