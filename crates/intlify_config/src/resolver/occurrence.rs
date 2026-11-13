// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Finding Occurrences: where, and through which admitted input
//! Evidence, an ordinary Finding was observed (D15:253-256, 2385-2389).
//!
//! Every Finding candidate but the terminal reporting Finding contributes one
//! occurrence candidate, from the unit that found it, with its primary Evidence
//! and its related Evidence as projected before the invocation's evidence
//! tokens are assigned (D15:2385). The evaluation keeps the candidate of every
//! blocking result that carried what it observed. The caller assigns those
//! tokens once, to every candidate's redacted components together with those of
//! the invocation's other final Evidence projections, such as Coverage Decision
//! Evidence's, so that one component keeps one token in every record
//! (D15:2194). The occurrences retain each record under them, and then group
//! the candidates that share a Finding Key and a primary Evidence, whose view
//! is its identity (D15:2505). Within a group, related Evidence is merged and
//! deduplicated in Evidence order without any record equal to the primary one
//! (Decision 015-193), and the candidates' multiplicity becomes the ordinals
//! `0..n-1`, which follow neither physical nor worker order (D15:2387). Each
//! ordinal is one record, `{ finding, primary, ordinal, related }` (D15:2479),
//! whose ordinal is a decimal string (D15:2452). The Finding Occurrence Key is
//! the Finding Key, the primary Evidence, and the ordinal, without the related
//! Evidence (D15:256, 2703). Occurrences order by Finding, then by primary
//! Evidence, then by ordinal (D15:2389).
//!
//! An occurrence's Evidence record owner is the unit of its Finding
//! (D15:2205). Under that unit, the occurrence refers to its primary and
//! related Evidence, each a record of the evidence index (D15:1972), and
//! accounting charges the unit the occurrence's accounting view (D15:2518).
//!
//! A stage builds a selector's Evidence under the final selector authority
//! before it makes the candidate (D15:819). The reporting slice's bounds come
//! before grouping (D15:2209) and come with their own stage. Every occurrence
//! a test builds is checked against the Finding Registry: its primary and
//! related Evidence must have the kinds, origins, and shapes that its check
//! and code's pair gives them.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value};

use crate::construction::registry::check::RecordKind;

use super::evaluation::{Code, IssueKey, Unit};
use super::evidence::{Evidence, EvidenceProjection};
use super::evidence_token::{EvidenceTokens, Redacted};
use super::record_view::{RecordViews, ViewInvariant};
use super::token::Specification;

/// A Finding Occurrence candidate: the Finding its unit found, with the
/// primary and related Evidence the unit observed it through.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OccurrenceCandidate {
    pub(crate) finding: IssueKey,
    pub(crate) primary: EvidenceProjection,
    pub(crate) related: Vec<EvidenceProjection>,
}

/// A resolver bug the occurrences refuse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OccurrenceInvariant {
    /// The terminal reporting Finding has no occurrence (D15:2385).
    TerminalOccurrence(IssueKey),
    /// The evidence tokens leave out a component of the Finding's candidate.
    UnassignedToken(IssueKey),
}

/// One Finding Occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FindingOccurrence {
    finding: IssueKey,
    primary: Evidence,
    ordinal: u64,
    related: Vec<Evidence>,
}

impl FindingOccurrence {
    pub(crate) const fn finding(&self) -> &IssueKey {
        &self.finding
    }

    pub(crate) const fn primary(&self) -> &Evidence {
        &self.primary
    }

    pub(crate) const fn ordinal(&self) -> u64 {
        self.ordinal
    }

    pub(crate) fn related(&self) -> &[Evidence] {
        &self.related
    }

    /// The Finding Occurrence Key: the Finding Key, the primary Evidence,
    /// and the ordinal (D15:256).
    pub(crate) const fn key(&self) -> (&IssueKey, &Evidence, u64) {
        (&self.finding, &self.primary, self.ordinal)
    }

    /// The occurrence's Evidence record owner: the unit of its Finding.
    pub(crate) const fn owner(&self) -> &Unit {
        self.finding.unit()
    }

    /// The Evidence the occurrence refers to, its primary record and then its
    /// related ones, each under its owner.
    pub(crate) fn references(&self) -> impl Iterator<Item = (&Unit, &Evidence)> {
        std::iter::once(&self.primary)
            .chain(&self.related)
            .map(|evidence| (self.owner(), evidence))
    }

    /// `{ finding, primary, ordinal, related }` (D15:2479).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a Finding Key `views` cannot present.
    pub(crate) fn view(&self, views: &RecordViews<'_>) -> Result<Value, ViewInvariant> {
        let mut view = Map::new();
        view.insert("finding".to_owned(), views.finding_key(&self.finding)?);
        view.insert("primary".to_owned(), self.primary.view());
        view.insert("ordinal".to_owned(), Value::from(self.ordinal.to_string()));
        view.insert(
            "related".to_owned(),
            Value::from(self.related.iter().map(Evidence::view).collect::<Vec<_>>()),
        );
        Ok(Value::Object(view))
    }

    /// The occurrence's accounting view, charged to its owner (D15:2518).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a Finding Key or owner `views` cannot
    /// present.
    pub(crate) fn accounting(&self, views: &RecordViews<'_>) -> Result<Value, ViewInvariant> {
        views.accounting(
            self.owner(),
            RecordKind::FindingOccurrence,
            self.view(views)?,
        )
    }
}

/// Every redacted component of `candidates`' records, for the invocation's
/// evidence tokens.
pub(crate) fn redacted(candidates: &[OccurrenceCandidate]) -> impl Iterator<Item = &Redacted> {
    candidates
        .iter()
        .flat_map(|candidate| std::iter::once(&candidate.primary).chain(&candidate.related))
        .flat_map(EvidenceProjection::redacted)
}

/// The occurrences of `candidates`, in 015's order, under the invocation's
/// evidence tokens `tokens`.
///
/// # Errors
///
/// Returns the resolver bug of a candidate of the terminal reporting
/// Finding, or of a component `tokens` leaves out.
pub(crate) fn occurrences(
    candidates: &[OccurrenceCandidate],
    specification: Specification<'_>,
    tokens: &EvidenceTokens,
) -> Result<Vec<FindingOccurrence>, OccurrenceInvariant> {
    let mut groups: BTreeMap<(IssueKey, Evidence), (u64, BTreeSet<Evidence>)> = BTreeMap::new();
    for candidate in candidates {
        let finding = &candidate.finding;
        if finding.code() == Code::ReportingLimitExceeded {
            return Err(OccurrenceInvariant::TerminalOccurrence(finding.clone()));
        }
        let retain = |projection: &EvidenceProjection| {
            projection
                .retained(specification, tokens)
                .ok_or_else(|| OccurrenceInvariant::UnassignedToken(finding.clone()))
        };
        let primary = retain(&candidate.primary)?;
        let related = candidate
            .related
            .iter()
            .map(retain)
            .collect::<Result<BTreeSet<Evidence>, _>>()?;
        #[cfg(test)]
        registered(candidate, &primary, &related);
        let group = groups.entry((finding.clone(), primary)).or_default();
        group.0 += 1;
        group.1.extend(related);
    }
    Ok(groups
        .into_iter()
        .flat_map(|((finding, primary), (count, related))| {
            let related: Vec<Evidence> = related
                .into_iter()
                .filter(|record| *record != primary)
                .collect();
            (0..count).map(move |ordinal| FindingOccurrence {
                finding: finding.clone(),
                primary: primary.clone(),
                ordinal,
                related: related.clone(),
            })
        })
        .collect())
}

/// Asserts that the Finding Registry's pair of `candidate`'s check and code
/// lets it carry `primary` and `related`.
#[cfg(test)]
fn registered(candidate: &OccurrenceCandidate, primary: &Evidence, related: &BTreeSet<Evidence>) {
    use crate::construction::registry::check::check_id;
    use crate::construction::registry::occurrence::testing::refusal;
    let unit = candidate.finding.unit();
    let code = candidate.finding.code();
    let check = check_id(unit.subject.kind(), unit.check)
        .unwrap_or_else(|| panic!("{unit:?} has no Check Registry row"));
    let related: Vec<&Evidence> = related.iter().collect();
    if let Some(refusal) = refusal(check, code, primary, &related) {
        panic!(
            "{check} reports {} through Evidence its Finding Registry row does not allow: {refusal}",
            code.as_str()
        );
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::boundary::source::{file_source_revision, read_pair, ExactIdentity, SourceLimits};
    use crate::construction::registry::occurrence::testing::refusal;
    use crate::input_limits::Bound;
    use crate::resolver::capability::CapabilityBound;
    use crate::resolver::evaluation::{
        Blocking, Check, Code, Evaluation, IssueKey, Phase, Reason, Subject, Unit,
    };
    use crate::resolver::evidence::{Evidence, EvidenceProjection, Source};
    use crate::resolver::evidence_component::{ComponentPath, Key, VerificationRole};
    use crate::resolver::evidence_path::Step;
    use crate::resolver::evidence_token::{EvidenceTokens, JsonType};
    use crate::resolver::locales::fixtures::key;
    use crate::resolver::locales::LocaleSubject;
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::resource_policy::{unit as policy, PolicySubject};
    use crate::resolver::specification::Slot;
    use crate::resolver::token::{Sha256, Specification};

    use super::{
        occurrences, redacted, FindingOccurrence, OccurrenceCandidate, OccurrenceInvariant,
    };

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };
    const FILE: &str = "intlify-file-source.v0:storefront/intlify.config.json";

    fn vocabulary(check: Check) -> Unit {
        Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Vocabulary,
            check,
        }
    }

    fn unique(locale: &str) -> Unit {
        Unit {
            phase: Phase::LocaleResolution,
            subject: Subject::Locale(LocaleSubject::RequestedLocale(key(locale))),
            check: Check::UniqueLocale,
        }
    }

    /// Records `unit` as finding `code` with `reason`, and returns the
    /// Finding's key.
    fn found(
        evaluation: &mut Evaluation,
        unit: &Unit,
        code: Code,
        reason: Option<Reason>,
    ) -> IssueKey {
        let found = Blocking {
            code,
            reason,
            details: None,
        };
        evaluation
            .check(unit.clone(), Ok(()), |()| Err::<(), _>(found))
            .unwrap()
            .unwrap_err();
        evaluation
            .findings()
            .into_iter()
            .find(|finding| finding.key.unit() == unit && finding.key.code() == code)
            .unwrap()
            .key
            .clone()
    }

    fn exact(text: &str) -> ExactIdentity {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        let limits = SourceLimits {
            identity_bytes: largest,
            revision_bytes: largest,
        };
        read_pair(Some(text), Some(&file_source_revision(b"")), b"", limits)
            .unwrap()
            .identity()
            .clone()
    }

    fn input(path: Result<ComponentPath, impl std::fmt::Debug>) -> EvidenceProjection {
        EvidenceProjection::resolver_input(path.unwrap())
    }

    fn vocabulary_root() -> EvidenceProjection {
        input(ComponentPath::root(O::IntentSurfaceClassVocabulary))
    }

    fn invalid_member() -> EvidenceProjection {
        input(ComponentPath::vocabulary_member(Key::Unsafe(
            JsonType::Object,
        )))
    }

    fn slot() -> EvidenceProjection {
        EvidenceProjection::resolver_input(ComponentPath::specification(
            Slot::IntentSurfaceClassVocabulary,
        ))
    }

    /// Materialized Value Evidence of `steps` through `origin`, whose source
    /// identity its owner marks non-secret.
    fn value(origin: O, steps: &[Step<'_>]) -> EvidenceProjection {
        marked_value(origin, true, steps)
    }

    fn marked_value(origin: O, marked: bool, steps: &[Step<'_>]) -> EvidenceProjection {
        let identity = exact(FILE);
        EvidenceProjection::configuration_value(
            origin,
            Source {
                identity: &identity,
                marked,
                locator: None,
            },
            steps,
            None,
        )
        .unwrap()
    }

    /// Entry Source Evidence of the whole entry through `origin`.
    fn entry(origin: O) -> EvidenceProjection {
        let identity = exact(FILE);
        EvidenceProjection::entry_source(
            origin,
            Source {
                identity: &identity,
                marked: true,
                locator: None,
            },
            None,
            None,
        )
        .unwrap()
    }

    /// Implementation-capability Evidence of the bound at `path`.
    fn bound(path: &str) -> EvidenceProjection {
        let (bound, _) = CapabilityBound::inventory()
            .into_iter()
            .find(|(bound, _)| bound.to_string() == path)
            .unwrap();
        input(ComponentPath::capability(bound))
    }

    fn requested(index: u64) -> EvidenceProjection {
        value(
            O::ConfigurationFile,
            &[
                Step::Safe("profiles"),
                Step::Safe("web"),
                Step::Safe("requestedLocales"),
                Step::Index(index),
            ],
        )
    }

    fn candidate(
        finding: &IssueKey,
        primary: EvidenceProjection,
        related: Vec<EvidenceProjection>,
    ) -> OccurrenceCandidate {
        OccurrenceCandidate {
            finding: finding.clone(),
            primary,
            related,
        }
    }

    /// The invocation's evidence tokens of `candidates`.
    fn tokens(candidates: &[OccurrenceCandidate]) -> EvidenceTokens {
        EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256)
    }

    fn built(candidates: &[OccurrenceCandidate]) -> Vec<FindingOccurrence> {
        occurrences(candidates, PROFILE, &tokens(candidates)).unwrap()
    }

    /// The `(code, ordinal, related count)` of each occurrence, in order.
    fn summary(occurrences: &[FindingOccurrence]) -> Vec<(&'static str, u64, usize)> {
        occurrences
            .iter()
            .map(|occurrence| {
                (
                    occurrence.finding().code().as_str(),
                    occurrence.ordinal(),
                    occurrence.related().len(),
                )
            })
            .collect()
    }

    /// Candidates that share a Finding Key and a primary Evidence form one
    /// group whose ordinals count them, whatever order they come in; a
    /// different primary Evidence forms another group.
    #[test]
    fn candidates_group_by_finding_and_primary_with_ordinals_for_their_multiplicity() {
        let mut evaluation = Evaluation::default();
        let invalid = found(
            &mut evaluation,
            &vocabulary(Check::IntactArtifact),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::MemberInvalid),
        );
        let candidates = [
            candidate(&invalid, invalid_member(), Vec::new()),
            candidate(&invalid, vocabulary_root(), Vec::new()),
            candidate(&invalid, invalid_member(), Vec::new()),
        ];
        let forward = built(&candidates);
        let mut reversed = candidates.to_vec();
        reversed.reverse();
        assert_eq!(built(&reversed), forward);
        // The root's Evidence orders before a member's, each group counting
        // its own candidates from zero.
        let groups: Vec<(serde_json::Value, u64)> = forward
            .iter()
            .map(|occurrence| {
                (
                    occurrence.primary().view()["component"].clone(),
                    occurrence.ordinal(),
                )
            })
            .collect();
        assert_eq!(groups.len(), 3);
        assert_eq!(groups[0], (json!([]), 0));
        assert_eq!(groups[1].1, 0);
        assert_eq!(groups[2].1, 1);
        assert_eq!(groups[1].0, groups[2].0);
        assert_ne!(forward[1].key(), forward[2].key());
    }

    /// A group merges its candidates' related Evidence, each record once
    /// and in Evidence order, and leaves out any equal to its primary
    /// Evidence (Decision 015-193).
    #[test]
    fn related_evidence_is_merged_once_in_order_without_the_primary() {
        let mut evaluation = Evaluation::default();
        let duplicate = found(&mut evaluation, &unique("en"), Code::LocaleDuplicate, None);
        let candidates = [
            candidate(&duplicate, requested(0), vec![requested(2), requested(0)]),
            candidate(&duplicate, requested(0), vec![requested(1), requested(2)]),
        ];
        let built = built(&candidates);
        assert_eq!(
            summary(&built),
            [
                ("project-profile-locale-duplicate", 0, 2),
                ("project-profile-locale-duplicate", 1, 2)
            ]
        );
        let related: Vec<serde_json::Value> = built[0]
            .related()
            .iter()
            .map(|record| record.view()["value"].clone())
            .collect();
        assert_eq!(
            related,
            [
                json!({ "exact-json-pointer": "/profiles/web/requestedLocales/1" }),
                json!({ "exact-json-pointer": "/profiles/web/requestedLocales/2" }),
            ]
        );
        assert_eq!(built[0].related(), built[1].related());
    }

    /// Related Evidence does not identify an occurrence: changing it changes
    /// the record but not the key (D15:2385).
    #[test]
    fn the_key_leaves_related_evidence_out() {
        let mut evaluation = Evaluation::default();
        let asserted = found(
            &mut evaluation,
            &vocabulary(Check::AssertedArtifact),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::IdentityMismatch),
        );
        let with = built(&[candidate(&asserted, vocabulary_root(), vec![slot()])]);
        let without = built(&[candidate(&asserted, vocabulary_root(), Vec::new())]);
        assert_eq!(with[0].key(), without[0].key());
        assert_ne!(with[0], without[0]);
    }

    /// An occurrence is owned by the unit of its Finding, refers under that
    /// unit to its primary and then its related Evidence, and charges it its
    /// accounting view.
    #[test]
    fn an_occurrence_is_owned_by_its_findings_unit_and_refers_to_its_evidence() {
        let mut evaluation = Evaluation::default();
        let asserted = found(
            &mut evaluation,
            &vocabulary(Check::AssertedArtifact),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::IdentityMismatch),
        );
        let built = built(&[candidate(&asserted, vocabulary_root(), vec![slot()])]);
        let occurrence = &built[0];
        let unit = asserted.unit();
        assert_eq!(occurrence.owner(), unit);
        assert_eq!(
            occurrence.references().collect::<Vec<_>>(),
            [
                (unit, occurrence.primary()),
                (unit, &occurrence.related()[0])
            ]
        );
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        assert_eq!(
            occurrence.accounting(&views).unwrap(),
            json!({
                "owner": views.unit(unit).unwrap(),
                "record": { "finding-occurrence": occurrence.view(&views).unwrap() },
            })
        );
    }

    /// Occurrences order by Finding, then by primary Evidence, then by
    /// ordinal; each record's view is `{ finding, primary, ordinal, related }`
    /// with a decimal ordinal.
    #[test]
    fn occurrences_order_by_finding_primary_and_ordinal_and_present_their_view() {
        let mut evaluation = Evaluation::default();
        let asserted = found(
            &mut evaluation,
            &vocabulary(Check::AssertedArtifact),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::IdentityMismatch),
        );
        let invalid = found(
            &mut evaluation,
            &vocabulary(Check::IntactArtifact),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::MemberInvalid),
        );
        let built = built(&[
            candidate(&asserted, vocabulary_root(), vec![slot()]),
            candidate(&invalid, invalid_member(), Vec::new()),
            candidate(&invalid, invalid_member(), Vec::new()),
        ]);
        // The intact check ranks before the asserted one.
        assert_eq!(
            built
                .iter()
                .map(|occurrence| (occurrence.finding().unit().check, occurrence.ordinal()))
                .collect::<Vec<_>>(),
            [
                (Check::IntactArtifact, 0),
                (Check::IntactArtifact, 1),
                (Check::AssertedArtifact, 0)
            ]
        );
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        let view = built[1].view(&views).unwrap();
        assert_eq!(view["ordinal"], json!("1"));
        assert_eq!(
            view["finding"],
            views.finding_key(built[1].finding()).unwrap()
        );
        assert_eq!(view["primary"], built[1].primary().view());
        assert_eq!(view["related"], json!([]));
        let asserted_view = built[2].view(&views).unwrap();
        assert_eq!(
            asserted_view["related"],
            json!([{
                "kind": "specification-input",
                "origin": "profile-resolution-specification-input",
                "component": [{ "fixed-role": "intent-surface-class-vocabulary" }]
            }])
        );
    }

    /// One redacted component keeps one token in every occurrence, so
    /// candidates whose members carry equal redacted keys share a primary
    /// Evidence and one group.
    #[test]
    fn tokens_are_assigned_once_for_all_candidates() {
        let mut evaluation = Evaluation::default();
        let invalid = found(
            &mut evaluation,
            &vocabulary(Check::IntactArtifact),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::MemberInvalid),
        );
        let body = found(
            &mut evaluation,
            &policy(PolicySubject::ResourceLimits, Check::CandidateBody),
            Code::ResourcePolicyNotAdmitted,
            Some(Reason::BodyIntegrityInvalid),
        );
        let reference = value(
            O::ConfigurationFile,
            &[
                Step::Safe("profiles"),
                Step::Safe("web"),
                Step::Safe("policies"),
                Step::Safe("resourceLimits"),
            ],
        );
        let pins =
            EvidenceProjection::resolver_input(ComponentPath::verification(VerificationRole::Pins));
        let candidate_member = input(ComponentPath::artifact_member(Key::Unsafe(
            JsonType::Object,
        )));
        let candidates = [
            candidate(&invalid, invalid_member(), Vec::new()),
            candidate(&body, candidate_member, vec![reference, pins]),
            candidate(&invalid, invalid_member(), Vec::new()),
        ];
        let tokens = tokens(&candidates);
        let built = occurrences(&candidates, PROFILE, &tokens).unwrap();
        // The resource policy's phase comes first. The invalid vocabulary
        // member's two candidates share one primary Evidence, and the redacted
        // keys of the two origins have two tokens.
        assert_eq!(
            summary(&built),
            [
                ("project-profile-resource-policy-not-admitted", 0, 2),
                (
                    "project-profile-intent-surface-class-vocabulary-not-admitted",
                    0,
                    0
                ),
                (
                    "project-profile-intent-surface-class-vocabulary-not-admitted",
                    1,
                    0
                ),
            ]
        );
        assert_eq!(built[1].primary(), built[2].primary());
        assert_eq!(tokens.comparisons(), 0);
        let token = |occurrence: &FindingOccurrence| {
            occurrence.primary().view()["component"][1]["redacted-key"].clone()
        };
        assert_ne!(token(&built[0]), token(&built[1]));
    }

    /// The same failure through a file and through a programmatic entry has
    /// one Finding Key and different occurrence keys (D15:2389).
    #[test]
    fn file_and_programmatic_failures_share_the_finding_key_but_not_the_occurrence_key() {
        let mut evaluation = Evaluation::default();
        let duplicate = found(&mut evaluation, &unique("en"), Code::LocaleDuplicate, None);
        let steps = [
            Step::Safe("profiles"),
            Step::Safe("web"),
            Step::Safe("requestedLocales"),
            Step::Index(0),
        ];
        let built = built(&[
            candidate(&duplicate, value(O::ConfigurationFile, &steps), Vec::new()),
            candidate(
                &duplicate,
                value(O::ConfigurationProgrammatic, &steps),
                Vec::new(),
            ),
        ]);
        assert_eq!(built.len(), 2);
        assert_eq!(built[0].finding(), built[1].finding());
        assert_ne!(built[0].key(), built[1].key());
        assert_eq!((built[0].ordinal(), built[1].ordinal()), (0, 0));
    }

    /// A related record's redacted components take their tokens with every
    /// other record's, even when no primary record has them.
    #[test]
    fn related_records_take_tokens_with_the_primary_ones() {
        let mut evaluation = Evaluation::default();
        let duplicate = found(&mut evaluation, &unique("en"), Code::LocaleDuplicate, None);
        let unmarked = marked_value(
            O::ConfigurationFile,
            false,
            &[
                Step::Safe("profiles"),
                Step::Safe("web"),
                Step::Safe("requestedLocales"),
                Step::Index(1),
            ],
        );
        let built = built(&[candidate(&duplicate, requested(0), vec![unmarked])]);
        assert!(built[0].related()[0].view()["source"]
            .get("redacted")
            .is_some());
        assert!(built[0].primary().view()["source"]
            .get("safe-opaque")
            .is_some());
    }

    /// Tokens that leave out a component of a candidate's records are the
    /// caller's bug.
    #[test]
    fn tokens_that_leave_out_a_component_are_refused() {
        let mut evaluation = Evaluation::default();
        let duplicate = found(&mut evaluation, &unique("en"), Code::LocaleDuplicate, None);
        let unmarked = marked_value(
            O::ConfigurationFile,
            false,
            &[
                Step::Safe("profiles"),
                Step::Safe("web"),
                Step::Safe("requestedLocales"),
                Step::Index(1),
            ],
        );
        let candidates = [candidate(&duplicate, requested(0), vec![unmarked])];
        let primaries = EvidenceTokens::assign(
            PROFILE,
            candidates
                .iter()
                .flat_map(|candidate| candidate.primary.redacted()),
            &Sha256,
        );
        assert_eq!(
            occurrences(&candidates, PROFILE, &primaries).unwrap_err(),
            OccurrenceInvariant::UnassignedToken(duplicate)
        );
    }

    /// A test that builds an occurrence through Evidence its pair does not
    /// allow fails.
    #[test]
    #[should_panic(expected = "does not allow")]
    fn an_occurrence_its_pair_does_not_allow_fails_the_test() {
        let mut evaluation = Evaluation::default();
        let invalid = found(
            &mut evaluation,
            &vocabulary(Check::IntactArtifact),
            Code::IntentSurfaceClassVocabularyNotAdmitted,
            Some(Reason::MemberInvalid),
        );
        built(&[candidate(&invalid, slot(), Vec::new())]);
    }

    /// The terminal reporting Finding has no occurrence.
    #[test]
    fn the_terminal_reporting_finding_has_no_occurrence() {
        let mut evaluation = Evaluation::fabricated();
        let terminal = found(
            &mut evaluation,
            &vocabulary(Check::IntactArtifact),
            Code::ReportingLimitExceeded,
            None,
        );
        let candidates = [candidate(&terminal, vocabulary_root(), Vec::new())];
        assert_eq!(
            occurrences(&candidates, PROFILE, &tokens(&candidates)).unwrap_err(),
            OccurrenceInvariant::TerminalOccurrence(terminal)
        );
    }

    /// Every occurrence a test builds is checked against the Finding
    /// Registry: Evidence of a kind, an origin, or a shape that the pair of
    /// its check and code does not allow is refused.
    #[test]
    fn the_registry_check_refuses_evidence_its_pair_does_not_allow() {
        let retained = |projection: &EvidenceProjection| -> Evidence {
            let tokens = crate::resolver::evidence_token::EvidenceTokens::assign(
                PROFILE,
                projection.redacted(),
                &Sha256,
            );
            projection.retained(PROFILE, &tokens).unwrap()
        };
        let check = "specification.vocabulary.intact";
        let code = Code::IntentSurfaceClassVocabularyNotAdmitted;
        let root = retained(&vocabulary_root());
        let member = retained(&invalid_member());
        assert_eq!(refusal(check, code, &root, &[]), None);
        assert_eq!(refusal(check, code, &member, &[]), None);
        // A related kind the pair names none of, a primary kind it does not
        // name, a shape it does not carry, and another origin.
        assert!(refusal(check, code, &root, &[&retained(&slot())]).is_some());
        assert!(refusal(check, code, &retained(&slot()), &[]).is_some());
        let data = retained(&input(ComponentPath::root(O::LocaleCanonicalizationData)));
        assert!(refusal(check, code, &data, &[]).is_some());
        let asserted = "specification.vocabulary.asserted";
        assert!(refusal(asserted, code, &member, &[]).is_some());
        assert!(refusal(asserted, code, &root, &[&retained(&slot())]).is_none());
        // A check that does not report the code.
        assert!(refusal(check, Code::LocaleDuplicate, &root, &[]).is_some());
        let file = retained(&requested(0));
        let duplicate = "locale.requested.unique";
        assert_eq!(
            refusal(duplicate, Code::LocaleDuplicate, &file, &[&file]),
            None
        );
        // A primary kind the pair does not name, though its origin is one any
        // configuration Evidence may use.
        assert!(refusal(
            duplicate,
            Code::LocaleDuplicate,
            &retained(&entry(O::ConfigurationFile)),
            &[]
        )
        .is_some());
        // A related kind the pair does not name, though its origin is one any
        // configuration Evidence may use.
        assert!(refusal(check, code, &root, &[&file]).is_some());
        // An origin the pair does not allow its kind: a raw-file check's
        // Entry Source Evidence comes only through a file.
        let syntax = "entry.input.well-formed-json";
        assert_eq!(
            refusal(
                syntax,
                Code::JsonInvalid,
                &retained(&entry(O::ConfigurationFile)),
                &[]
            ),
            None
        );
        assert!(refusal(
            syntax,
            Code::JsonInvalid,
            &retained(&entry(O::ConfigurationProgrammatic)),
            &[]
        )
        .is_some());
        // A key the pair's shape does not end with: the bootstrap's candidate
        // is not admitted yet.
        let body = "resource-policy.resource-limits.candidate-body";
        let member = |key| retained(&input(ComponentPath::artifact_member(key)));
        assert_eq!(
            refusal(
                body,
                Code::ResourcePolicyNotAdmitted,
                &member(Key::Unsafe(JsonType::Object)),
                &[]
            ),
            None
        );
        assert!(refusal(
            body,
            Code::ResourcePolicyNotAdmitted,
            &member(Key::Identity(json!(""))),
            &[]
        )
        .is_some());
        // Roles the pair's shape does not have: another bound of the group.
        let nodes = "entry.input.within-nodes";
        let source = retained(&entry(O::ConfigurationFile));
        let capability = |path| retained(&bound(path));
        assert_eq!(
            refusal(
                nodes,
                Code::ResourceLimitExceeded,
                &source,
                &[&capability("policyComparable.configurationInput.maxNodes")]
            ),
            None
        );
        assert!(refusal(
            nodes,
            Code::ResourceLimitExceeded,
            &source,
            &[&capability("policyComparable.configurationInput.maxDepth")]
        )
        .is_some());
    }
}
