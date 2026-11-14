// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Phase 1, `entry-admission`, over strict file entry. The
//! phase's first kind owns the entry dependency records and waits on nothing
//! (D15:2516). The complete input is one singleton subject whose ten checks
//! run in check-rank order, each waiting on the checks whose result it
//! needs: the parser's tokens on the file's bytes, strict JSON on the tokens
//! (D15:1794, 1795), duplicate members and the JSON-compatible domain on a
//! syntactically parsed file (D15:802, 2289), and the five root-value bounds
//! on both, since they bound the complete root that entry materializes
//! (D15:1876). A check whose prerequisite failed is not evaluated, with the
//! causes of the failure (D15:2149), and a value exists only when every check
//! admits it; otherwise there is no Shared projection, and the causes are
//! why (D15:2369).
//!
//! Each check reports what strict file entry found: the file's bytes exactly,
//! the parser's tokens as their first-over witness, one Finding for a syntax or
//! UTF-8 failure, one for the duplicate members, one for each reason a value
//! is outside the JSON-compatible domain, and each root-value bound the
//! complete value exceeds, the single-string bound as its first-over witness
//! (D15:1884, 1999-2006, 2203, 2287-2289, 2356).
//!
//! Every candidate is observed through the file's Entry Source Evidence
//! (D15:780-786): the file's bytes with neither path nor span, the parser's
//! tokens and a syntax or UTF-8 failure at the span that stopped parsing, a
//! duplicate member at its key, beside every other member of its name
//! (D15:802), a value outside the domain at its token, a total at the root,
//! and a string past the single-string bound at its token. A path is the
//! fact's internal path with each member classified, so no unsafe name is
//! retained (D15:768, 776), and a bound is observed beside the capability's
//! bound it exceeds.

use std::sync::Arc;

use crate::input_limits::{CountRelation, InputBound, InputLimits, LimitViolation};
use crate::materialize::{
    materialize_file, member_name, ByteSpan, Inadmissible, Incompatibility, InputFailure,
    MaterializedDocument, PathStep,
};
use crate::structural::Edge;

use super::capability::{BootstrapBoundId, BootstrapGroup, CapabilityBound};
use super::evaluation::{
    and, Actual, Admitted, Blocking, BootstrapLimitExceeded, Check, Code, Evaluation,
    EvaluationInvariant, Found, LimitExceeded, Observation, Phase, Reason, Subject, Unit,
};
use super::evidence::{EvidenceInvariant, EvidenceProjection, Source};
use super::evidence_component::{ComponentInvariant, ComponentPath};
use super::evidence_path::Step;
use super::evidence_source::SourceSpan;
use super::limits::{BoundId, Group};
use super::origin::SafeOriginKind;

/// The unit that owns the entry dependency records.
pub(crate) const OWNER: Unit = Unit {
    phase: Phase::EntryAdmission,
    subject: Subject::EntryDependencies,
    check: Check::EstablishedSlots,
};

/// The unit of the complete input's `check`.
pub(crate) const fn input(check: Check) -> Unit {
    Unit {
        phase: Phase::EntryAdmission,
        subject: Subject::CompleteInput,
        check,
    }
}

/// The root-value bounds' checks, in check-rank order, with the bound each
/// applies.
const ROOT_VALUE_BOUNDS: [(Check, InputBound, &str); 5] = [
    (Check::WithinNodes, InputBound::Nodes, "maxNodes"),
    (Check::WithinDepth, InputBound::Depth, "maxDepth"),
    (
        Check::WithinCollectionEntries,
        InputBound::CollectionEntries,
        "maxCollectionEntries",
    ),
    (
        Check::WithinTotalStringBytes,
        InputBound::TotalStringBytes,
        "maxTotalStringBytes",
    ),
    (
        Check::WithinSingleStringBytes,
        InputBound::SingleStringBytes,
        "maxSingleStringBytes",
    ),
];

/// The internal path of the root, which has no step.
const ROOT: &[PathStep] = &[];

/// A resolver bug Phase 1 found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EntryInvariant {
    Evaluation(EvaluationInvariant),
    /// Strict file entry counted past `u64`, which the file's length bounds.
    AccountingOverflow,
    /// Strict file entry found a failure that no check reported.
    UnreportedFailure,
    /// A fact's Entry Source Evidence could not be projected.
    Evidence(EvidenceInvariant),
    /// A capability bound has no component path.
    Component(ComponentInvariant),
}

impl From<EvaluationInvariant> for EntryInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

/// The classification of the members of an internal path from the
/// configuration root (D15:776).
pub(crate) type Classify = dyn for<'e> Fn(&[Edge<'e>]) -> Vec<Step<'e>>;

/// The file Phase 1 reads, and what its Evidence names: the file's bytes, the
/// source the entry admitted them under, and the classification of the
/// members on a fact's internal path, which the admitted configuration schema
/// and the capability's `policyComparable` bounds fix (D15:776, 780-786).
pub(crate) struct EntryFile<'a> {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) source: Source<'a>,
    pub(crate) classify: &'a Classify,
}

/// The Safe Origin Kind of a file entry's External Profile Selector Input:
/// the entry's `profile-selector-cli` slot (D15:804, 015-172).
pub(crate) const SELECTOR_ORIGIN: SafeOriginKind = SafeOriginKind::ProfileSelectorCli;

impl EntryFile<'_> {
    /// Materialized Value Evidence in the file: the value at the internal
    /// path `edges` from the root, each member classified, at the token
    /// `span` (D15:776, 790).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the value without Evidence.
    pub(crate) fn value_evidence(
        &self,
        edges: &[Edge<'_>],
        span: ByteSpan,
    ) -> Result<EvidenceProjection, EvidenceInvariant> {
        let steps = (self.classify)(edges);
        let span = SourceSpan::admit(span.start_byte(), span.end_byte(), &self.bytes);
        EvidenceProjection::configuration_value(
            SafeOriginKind::ConfigurationFile,
            self.source,
            &steps,
            span,
        )
    }

    /// Entry Source Evidence at the internal path `path` when one was
    /// established, the root when it has no step, and at `span` (D15:768,
    /// 780-786).
    fn evidence(
        &self,
        path: Option<&[PathStep]>,
        span: Option<ByteSpan>,
    ) -> Result<EvidenceProjection, EntryInvariant> {
        let steps = path.unwrap_or_default();
        let names: Vec<String> = steps
            .iter()
            .filter_map(|step| match *step {
                PathStep::Member(key) => Some(member_name(&self.bytes, key)),
                PathStep::Index(_) => None,
            })
            .collect();
        let mut names = names.iter();
        let edges: Vec<Edge<'_>> = steps
            .iter()
            .map(|step| match *step {
                PathStep::Member(_) => {
                    Edge::Member(names.next().expect("one name for each member"))
                }
                PathStep::Index(index) => Edge::Index(index),
            })
            .collect();
        let classified = (self.classify)(&edges);
        let span = span
            .and_then(|span| SourceSpan::admit(span.start_byte(), span.end_byte(), &self.bytes));
        EvidenceProjection::entry_source(
            SafeOriginKind::ConfigurationFile,
            self.source,
            path.map(|_| classified.as_slice()),
            span,
        )
        .map_err(EntryInvariant::Evidence)
    }
}

/// Phase 1 over `file`, under the bounds `limits` the Admitted Implementation
/// Capability gives `bootstrapOnly.rawInput` and
/// `policyComparable.configurationInput`: the materialized document, or the
/// causes of its absence, which are the blocking Findings of Phase 1 the
/// Shared projection is unavailable for (D15:2369).
///
/// # Errors
///
/// Returns the resolver bug Phase 1 found.
pub(crate) fn admit_entry(
    evaluation: &mut Evaluation,
    file: &EntryFile<'_>,
    limits: InputLimits,
) -> Result<Admitted<MaterializedDocument>, EntryInvariant> {
    // The owner records the entry dependencies, and nothing waits on it.
    let _: Admitted<()> = evaluation.check(OWNER, Ok(()), |()| Ok::<(), Blocking>(()))?;
    let entry = materialize_file(Arc::clone(&file.bytes), limits);
    let found = Findings::of(file, entry.as_ref().err().map(|error| &error.reason))?;
    let file_bytes = evaluation.check_found(input(Check::WithinFileBytes), Ok(()), |()| {
        found.file_bytes.map_or(Ok(()), Err)
    })?;
    let tokens = evaluation.check_found(input(Check::WithinParserTokens), file_bytes, |()| {
        found.tokens.map_or(Ok(()), Err)
    })?;
    let syntax = evaluation.check_found(input(Check::WellFormedJson), tokens, |()| {
        found.syntax.map_or(Ok(()), Err)
    })?;
    let unique = evaluation.check_found(input(Check::UniqueMembers), syntax.clone(), |()| {
        found.unique.map_or(Ok(()), Err)
    })?;
    let compatible = evaluation.check_found(input(Check::JsonCompatible), syntax, |()| {
        found.compatible.map_or(Ok(()), Err)
    })?;
    let materializable = and(unique, compatible);
    let mut bounded = Ok(());
    for ((check, _, _), within) in ROOT_VALUE_BOUNDS.into_iter().zip(found.bounds) {
        let within = evaluation.check_found(input(check), materializable.clone(), |()| {
            within.map_or(Ok(()), Err)
        })?;
        bounded = and(bounded, within);
    }
    match (bounded, entry) {
        (Err(causes), _) => Ok(Err(causes)),
        (Ok(()), Ok(document)) => Ok(Ok(document)),
        (Ok(()), Err(_)) => Err(EntryInvariant::UnreportedFailure),
    }
}

/// What each of the complete input's checks finds, each candidate observed,
/// before the check is evaluated: nothing when strict file entry found
/// nothing for it.
#[derive(Default)]
struct Findings {
    file_bytes: Option<Found>,
    tokens: Option<Found>,
    syntax: Option<Found>,
    unique: Option<Found>,
    compatible: Option<Found>,
    /// The root-value bounds', in check-rank order.
    bounds: [Option<Found>; 5],
}

impl Findings {
    /// What the checks find in `file`, from what stopped strict file entry
    /// or kept its complete value from being materialized.
    fn of(file: &EntryFile<'_>, failure: Option<&InputFailure>) -> Result<Self, EntryInvariant> {
        let mut found = Self::default();
        match failure {
            None => {}
            Some(InputFailure::AccountingOverflow { .. }) => {
                return Err(EntryInvariant::AccountingOverflow);
            }
            Some(InputFailure::FileBytes(violation)) => {
                let observation = Observation {
                    primary: file.evidence(None, None)?,
                    related: vec![capability(raw("maxFileBytes"))?],
                };
                found.file_bytes = Some(Found::observed(
                    raw_bound("maxFileBytes", violation),
                    observation,
                ));
            }
            Some(InputFailure::ParserTokens { violation, span }) => {
                let observation = Observation {
                    primary: file.evidence(None, Some(*span))?,
                    related: vec![capability(raw("maxParserTokens"))?],
                };
                found.tokens = Some(Found::observed(
                    raw_bound("maxParserTokens", violation),
                    observation,
                ));
            }
            Some(InputFailure::InvalidUtf8 { span } | InputFailure::Syntax { span }) => {
                let observation = Observation {
                    primary: file.evidence(None, Some(*span))?,
                    related: Vec::new(),
                };
                found.syntax = Some(Found::observed(
                    blocking(Code::JsonInvalid, None),
                    observation,
                ));
            }
            Some(InputFailure::Inadmissible(facts)) => {
                found.unique = duplicates(file, facts)?;
                found.compatible = incompatible(file, facts)?;
                found.bounds = bounds(file, facts)?;
            }
        }
        Ok(found)
    }
}

/// The duplicate members' Finding, with a candidate for each at its key,
/// beside every other member of its name (D15:802, Decision 015-194). The
/// others are in the same object under the same name, so at the same Logical
/// input path, and only their keys' spans differ.
fn duplicates(file: &EntryFile<'_>, facts: &Inadmissible) -> Result<Option<Found>, EntryInvariant> {
    let mut candidates = Vec::new();
    for duplicate in &facts.duplicates {
        let path = facts.paths.steps(Some(duplicate.path));
        let related = duplicate
            .others
            .iter()
            .map(|&other| file.evidence(Some(&path), Some(other)))
            .collect::<Result<_, _>>()?;
        let observation = Observation {
            primary: file.evidence(Some(&path), Some(duplicate.key))?,
            related,
        };
        candidates.push(Found::observed(
            blocking(Code::JsonMemberDuplicate, None),
            observation,
        ));
    }
    Ok(candidates.into_iter().reduce(Found::and))
}

/// The Findings of the values outside the JSON-compatible domain, one for
/// each reason, with a candidate for each value at its token (D15:255,
/// 2289).
fn incompatible(
    file: &EntryFile<'_>,
    facts: &Inadmissible,
) -> Result<Option<Found>, EntryInvariant> {
    let mut candidates = Vec::new();
    for value in &facts.incompatible {
        let reason = match value.reason {
            Incompatibility::NonPortableNumber => Reason::NonPortableNumber,
            Incompatibility::InvalidUnicode => Reason::InvalidUnicode,
        };
        let observation = Observation {
            primary: file.evidence(Some(&facts.paths.steps(value.path)), Some(value.span))?,
            related: Vec::new(),
        };
        candidates.push(Found::observed(
            blocking(Code::InputNotJsonCompatible, Some(reason)),
            observation,
        ));
    }
    Ok(candidates.into_iter().reduce(Found::and))
}

/// The root-value bounds' Findings, in check-rank order: each total the
/// complete value exceeds, observed at the root, and the single-string bound
/// at every string past it, each beside the capability's bound (D15:1876,
/// 1884, 2203).
fn bounds(
    file: &EntryFile<'_>,
    facts: &Inadmissible,
) -> Result<[Option<Found>; 5], EntryInvariant> {
    let mut found: [Option<Found>; 5] = Default::default();
    for (slot, (_, bound, name)) in found.iter_mut().zip(ROOT_VALUE_BOUNDS) {
        let Some(violation) = facts
            .violations
            .iter()
            .find(|violation| violation.bound == bound)
        else {
            continue;
        };
        let exceeded = Blocking::limit(LimitExceeded {
            bound: root_value(name),
            limit: violation.limit,
            actual: actual(violation),
        });
        let beside = capability(CapabilityBound::PolicyComparable(root_value(name)))?;
        let observed = |path: &[PathStep], span: Option<ByteSpan>| {
            let observation = Observation {
                primary: file.evidence(Some(path), span)?,
                related: vec![beside.clone()],
            };
            Ok::<_, EntryInvariant>(Found::observed(exceeded, observation))
        };
        let candidates = if bound == InputBound::SingleStringBytes {
            facts
                .long_strings
                .iter()
                .map(|string| observed(&facts.paths.steps(string.path), Some(string.span)))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![observed(ROOT, None)?]
        };
        *slot = Some(
            candidates
                .into_iter()
                .reduce(Found::and)
                .ok_or(EntryInvariant::UnreportedFailure)?,
        );
    }
    Ok(found)
}

/// The blocking result `code` with `reason`, which has no details.
const fn blocking(code: Code, reason: Option<Reason>) -> Blocking {
    Blocking {
        code,
        reason,
        details: None,
    }
}

/// The `bootstrapOnly.rawInput` bound `name`.
const fn raw(name: &'static str) -> CapabilityBound {
    CapabilityBound::BootstrapOnly(BootstrapBoundId {
        group: BootstrapGroup::RawInput,
        name,
    })
}

/// The `policyComparable.configurationInput` bound `name`.
const fn root_value(name: &'static str) -> BoundId {
    BoundId {
        group: Group::ConfigurationInput,
        name,
    }
}

/// Resolver-input Evidence of the Implementation Capability's `bound`
/// (D15:836).
fn capability(bound: CapabilityBound) -> Result<EvidenceProjection, EntryInvariant> {
    ComponentPath::capability(bound)
        .map(EvidenceProjection::resolver_input)
        .map_err(EntryInvariant::Component)
}

/// A `bootstrapOnly.rawInput` bound's Finding (D15:2356).
fn raw_bound(name: &'static str, violation: &LimitViolation) -> Blocking {
    Blocking::bootstrap_limit(BootstrapLimitExceeded {
        bound: BootstrapBoundId {
            group: BootstrapGroup::RawInput,
            name,
        },
        limit: violation.limit,
        actual: actual(violation),
    })
}

fn actual(violation: &LimitViolation) -> Actual {
    match violation.relation {
        CountRelation::Exact => Actual::Exact(violation.actual),
        CountRelation::AtLeast => Actual::AtLeast(violation.actual),
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::Arc;

    use crate::boundary::source::{file_source_revision, read_pair, ExactIdentity, SourceLimits};
    use crate::input_limits::Bound;
    use crate::locale::fixtures::FixtureProvider;
    use crate::references::{PolicyReference, TargetProfileReference};
    use crate::resolver::evidence::Source;
    use crate::resolver::evidence_path::KeyFacts;
    use crate::resolver::limits::ResolutionLimits;
    use crate::structural::AuthoringSchema;

    use super::{Classify, EntryFile};

    /// The source identity the test files are admitted under.
    pub(crate) const FILE: &str = "intlify-file-source.v0:storefront/intlify.config.json";

    /// What a test file is admitted under: the source identity [`FILE`], which
    /// its owner marks non-secret, and the classification of the members on a
    /// fact's path in the model's configuration schema under the capability's
    /// bounds.
    pub(crate) struct FileFixture {
        identity: ExactIdentity,
        classify: Box<Classify>,
    }

    impl FileFixture {
        pub(crate) fn new(comparable: &ResolutionLimits) -> Self {
            let largest = Bound::new(u64::MAX - 1).unwrap();
            let limits = SourceLimits {
                identity_bytes: largest,
                revision_bytes: largest,
            };
            let identity = read_pair(Some(FILE), Some(&file_source_revision(b"")), b"", limits)
                .unwrap()
                .identity()
                .clone();
            let comparable = comparable.clone();
            let schema =
                AuthoringSchema::<PolicyReference, TargetProfileReference>::for_model().unwrap();
            Self {
                identity,
                classify: Box::new(move |edges| {
                    KeyFacts::<FixtureProvider> {
                        comparable: &comparable,
                        locales: None,
                    }
                    .steps(&schema, edges)
                }),
            }
        }

        /// The file of `bytes`.
        pub(crate) fn file(&self, bytes: Arc<[u8]>) -> EntryFile<'_> {
            EntryFile {
                bytes,
                source: Source {
                    identity: &self.identity,
                    marked: true,
                    locator: None,
                },
                classify: &*self.classify,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use crate::input_limits::{Bound, InputBound, InputLimits};
    use crate::materialize_tests::limits;
    use crate::resolver::capability::{BootstrapBoundId, BootstrapGroup};
    use crate::resolver::evaluation::{
        Actual, BootstrapLimitExceeded, Check, Code, Evaluation, IssueKey, LimitExceeded, Reason,
        Status, Unit,
    };
    use crate::resolver::limits::{fixture_limits, Group};

    use serde_json::{json, Value};

    use crate::construction::registry::check::RecordKind;
    use crate::resolver::capability::CapabilityBound;
    use crate::resolver::evidence::Evidence;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::occurrence::{occurrences, redacted, FindingOccurrence};
    use crate::resolver::record_view::RecordViews;
    use crate::resolver::token::{Sha256, Specification};

    use super::fixtures::FileFixture;
    use super::{admit_entry, capability, input, raw, root_value, OWNER, ROOT_VALUE_BOUNDS};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// The Finding Occurrences of what Phase 1 found in `source` under
    /// `limits`, in 015's order, each checked against its Finding Registry
    /// row.
    fn observed(source: &[u8], limits: InputLimits) -> Vec<FindingOccurrence> {
        let (evaluation, _) = entered(source, limits);
        let candidates = evaluation.occurrence_candidates();
        let tokens = EvidenceTokens::assign(PROFILE, redacted(candidates), &Sha256);
        occurrences(candidates, PROFILE, &tokens).unwrap()
    }

    /// The view of `evidence` without the source identity every test file
    /// shares.
    fn located(evidence: &Evidence) -> Value {
        let mut view = evidence.view();
        view.as_object_mut().unwrap().remove("source");
        view
    }

    /// The Evidence of the capability's `bound`, which carries no token.
    fn beside(bound: CapabilityBound) -> Evidence {
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&[]), &Sha256);
        capability(bound)
            .unwrap()
            .retained(PROFILE, &tokens)
            .unwrap()
    }

    /// The view of the span `text` occupies in `source`, at its `nth`
    /// occurrence.
    fn span(source: &[u8], text: &[u8], nth: usize) -> Value {
        let start = source
            .windows(text.len())
            .enumerate()
            .filter(|(_, window)| *window == text)
            .nth(nth)
            .map(|(start, _)| start)
            .unwrap();
        json!({ "start": start.to_string(), "end": (start + text.len()).to_string() })
    }

    /// Phase 1 over `source` under `limits`: the evaluation, and whether a
    /// document was admitted.
    fn entered(source: &[u8], limits: InputLimits) -> (Evaluation, bool) {
        let fixture = FileFixture::new(&fixture_limits(100));
        let mut evaluation = Evaluation::default();
        let admitted = admit_entry(&mut evaluation, &fixture.file(Arc::from(source)), limits)
            .unwrap()
            .is_ok();
        (evaluation, admitted)
    }

    /// Every Finding, as its check, code, and reason.
    fn found(evaluation: &Evaluation) -> Vec<(Check, Code, Option<Reason>)> {
        evaluation
            .findings()
            .iter()
            .map(|finding| {
                (
                    finding.key.unit().check,
                    finding.key.code(),
                    finding.key.reason(),
                )
            })
            .collect()
    }

    /// Each check's status, with the checks of its causes when it was not
    /// evaluated.
    fn statuses(evaluation: &Evaluation) -> BTreeMap<Check, Option<Vec<Check>>> {
        evaluation
            .statuses()
            .map(|(unit, status)| {
                let causes = match status {
                    Status::Evaluated => None,
                    Status::NotEvaluated(causes) => {
                        Some(causes.keys().map(|key| key.unit().check).collect())
                    }
                };
                (unit.check, causes)
            })
            .collect()
    }

    /// The checks after `failed` in check-rank order, each with what it then
    /// waits on.
    fn waiting(evaluation: &Evaluation) -> Vec<(Check, Vec<Check>)> {
        statuses(evaluation)
            .into_iter()
            .filter_map(|(check, causes)| causes.map(|causes| (check, causes)))
            .collect()
    }

    #[test]
    fn a_strict_file_passes_every_check_and_admits_its_document() {
        let (evaluation, admitted) = entered(br#"{"a":[1,"b"]}"#, limits());
        assert!(admitted);
        assert_eq!(found(&evaluation), []);
        let units: Vec<Unit> = evaluation
            .statuses()
            .map(|(unit, _)| unit.clone())
            .collect();
        assert_eq!(units.len(), 11);
        assert_eq!(units[0], OWNER);
        assert!(units[1..]
            .iter()
            .zip(crate::resolver::subject::SubjectKind::CompleteInput.checks())
            .all(|(unit, &check)| *unit == input(check)));
        assert!(statuses(&evaluation).values().all(Option::is_none));
    }

    /// A failure of each stage leaves the checks that need its result not
    /// evaluated, through the chain of prerequisites, and no document; the
    /// owner of the entry dependencies waits on nothing.
    #[test]
    fn each_stage_holds_back_the_checks_that_need_its_result() {
        let mut small = limits();
        small.raw.max_file_bytes = Bound::new(4).unwrap();
        let (file, admitted) = entered(b"[1, 2]", small);
        assert!(!admitted);
        assert_eq!(
            found(&file),
            [(Check::WithinFileBytes, Code::BootstrapLimitExceeded, None)]
        );
        assert_eq!(statuses(&file)[&Check::EstablishedSlots], None);
        assert_eq!(
            waiting(&file),
            [
                Check::WithinParserTokens,
                Check::WellFormedJson,
                Check::UniqueMembers,
                Check::JsonCompatible,
                Check::WithinNodes,
                Check::WithinDepth,
                Check::WithinCollectionEntries,
                Check::WithinTotalStringBytes,
                Check::WithinSingleStringBytes,
            ]
            .map(|check| (check, vec![Check::WithinFileBytes]))
        );
        let mut few = limits();
        few.raw.max_parser_tokens = Bound::new(2).unwrap();
        let (tokens, _) = entered(b"[1, 2]", few);
        assert_eq!(
            found(&tokens),
            [(
                Check::WithinParserTokens,
                Code::BootstrapLimitExceeded,
                None
            )]
        );
        assert_eq!(waiting(&tokens).len(), 8);
        let (syntax, admitted) = entered(br#"{"x":1,"x":2,]"#, limits());
        assert!(!admitted);
        assert_eq!(
            found(&syntax),
            [(Check::WellFormedJson, Code::JsonInvalid, None)]
        );
        // A duplicate before the syntax failure is not reported.
        assert_eq!(
            waiting(&syntax)
                .iter()
                .map(|(check, causes)| (*check, causes.clone()))
                .collect::<Vec<_>>()[..2],
            [
                (Check::UniqueMembers, vec![Check::WellFormedJson]),
                (Check::JsonCompatible, vec![Check::WellFormedJson])
            ]
        );
        let (utf8, _) = entered(b"[\"\xff\"]", limits());
        assert_eq!(
            found(&utf8),
            [(Check::WellFormedJson, Code::JsonInvalid, None)]
        );
    }

    /// A document exists only when every check admits it: a check a test
    /// makes block holds the document back as a failure that strict file
    /// entry found would (D15:2369).
    #[test]
    fn a_document_needs_every_check_to_admit_it() {
        let fixture = FileFixture::new(&fixture_limits(100));
        for &check in crate::resolver::subject::SubjectKind::CompleteInput.checks() {
            let mut evaluation = Evaluation::blocking(input(check));
            let file = fixture.file(Arc::from(&br#"{"a":1}"#[..]));
            let admitted = admit_entry(&mut evaluation, &file, limits()).unwrap();
            assert!(admitted.is_err(), "{check:?}");
        }
    }

    /// D15:780-786, 802: what stopped strict file entry is observed through
    /// the file: its bytes with neither path nor span, beside the capability's
    /// bound, and the parser's tokens or a syntax or UTF-8 failure at the span
    /// that stopped parsing.
    #[test]
    fn what_stopped_parsing_is_observed_where_it_stopped() {
        let at = |start: u64, end: u64| {
            json!({
                "kind": "entry-source",
                "origin": "configuration-file",
                "span": { "start": start.to_string(), "end": end.to_string() },
            })
        };
        let mut small = limits();
        small.raw.max_file_bytes = Bound::new(4).unwrap();
        let file = observed(b"[1, 2]", small);
        assert_eq!(file.len(), 1);
        assert_eq!(
            located(file[0].primary()),
            json!({ "kind": "entry-source", "origin": "configuration-file" })
        );
        assert_eq!(file[0].related(), [beside(raw("maxFileBytes"))]);
        let mut few = limits();
        few.raw.max_parser_tokens = Bound::new(2).unwrap();
        let tokens = observed(b"[1, 2]", few);
        assert_eq!(located(tokens[0].primary()), at(2, 3));
        assert_eq!(tokens[0].related(), [beside(raw("maxParserTokens"))]);
        let syntax = observed(br#"{"x":1,]"#, limits());
        assert_eq!(located(syntax[0].primary()), at(7, 8));
        assert!(syntax[0].related().is_empty());
        let utf8 = observed(b"[\"\xff\"]", limits());
        assert_eq!(located(utf8[0].primary()), at(2, 3));
    }

    /// D15:776, 802, Decision 015-194: a duplicate member is observed at its
    /// key and path, beside every other member of its name at theirs; a
    /// member the schema defines, or a valid Profile ID, keeps its name, and
    /// any other is redacted.
    #[test]
    fn a_duplicate_is_observed_at_its_key_beside_the_other_members_of_its_name() {
        let source = br#"{"profiles":{"app":{"x":1,"x":2,"x":3}}}"#;
        let duplicates = observed(source, limits());
        assert_eq!(duplicates.len(), 2);
        for (occurrence, nth) in duplicates.iter().zip([1, 2]) {
            let primary = located(occurrence.primary());
            let path = primary["path"].as_array().unwrap();
            assert_eq!(
                path[..2],
                [
                    json!({ "safe-member": "profiles" }),
                    json!({ "safe-member": "app" })
                ]
            );
            assert!(path[2].get("redacted-member").is_some());
            assert_eq!(primary["span"], span(source, br#""x""#, nth));
            assert_eq!(occurrence.ordinal(), 0);
            let others: Vec<Value> = occurrence
                .related()
                .iter()
                .map(|related| {
                    let related = located(related);
                    assert_eq!(related["path"], primary["path"]);
                    related["span"].clone()
                })
                .collect();
            let expected: Vec<Value> = [0, 1, 2]
                .into_iter()
                .filter(|&other| other != nth)
                .map(|other| span(source, br#""x""#, other))
                .collect();
            assert_eq!(others, expected);
        }
    }

    /// D15:2289: a value outside the JSON-compatible domain is observed at its
    /// token and path, each reason's Finding with an occurrence for every
    /// value.
    #[test]
    fn each_incompatible_value_is_an_occurrence_at_its_token() {
        let source = br#"[1e999,"\ud800",1e999]"#;
        let values: Vec<(Option<Reason>, Value)> = observed(source, limits())
            .iter()
            .map(|occurrence| (occurrence.finding().reason(), located(occurrence.primary())))
            .collect();
        let at = |index: &str, text: &[u8], nth| {
            json!({
                "kind": "entry-source",
                "origin": "configuration-file",
                "path": [{ "array-index": index }],
                "span": span(source, text, nth),
            })
        };
        assert_eq!(
            values,
            [
                (Some(Reason::InvalidUnicode), at("1", br#""\ud800""#, 0)),
                (Some(Reason::NonPortableNumber), at("0", b"1e999", 0)),
                (Some(Reason::NonPortableNumber), at("2", b"1e999", 1)),
            ]
        );
    }

    /// D15:768, 1876, 1884, 2203: a total is observed at the root and the
    /// single-string bound at every string past it, each beside the
    /// capability's bound.
    #[test]
    fn the_root_value_bounds_are_observed_at_the_root_or_at_each_long_string() {
        let source = br#"{"abc":["def",true]}"#;
        let mut over = limits();
        over.value.max_nodes = Bound::new(3).unwrap();
        over.value.max_single_string_bytes = Bound::new(2).unwrap();
        let bounds = observed(source, over);
        assert_eq!(bounds.len(), 3);
        assert_eq!(
            located(bounds[0].primary()),
            json!({ "kind": "entry-source", "origin": "configuration-file", "path": [] })
        );
        assert_eq!(
            bounds[0].related(),
            [beside(CapabilityBound::PolicyComparable(root_value(
                "maxNodes"
            )))]
        );
        let single = beside(CapabilityBound::PolicyComparable(root_value(
            "maxSingleStringBytes",
        )));
        let strings: Vec<(usize, Value)> = bounds[1..]
            .iter()
            .map(|occurrence| {
                assert_eq!(occurrence.related(), std::slice::from_ref(&single));
                let primary = located(occurrence.primary());
                (
                    primary["path"].as_array().unwrap().len(),
                    primary["span"].clone(),
                )
            })
            .collect();
        // The member name, at its member, then the value in its array.
        assert_eq!(
            strings,
            [
                (1, span(source, br#""abc""#, 0)),
                (2, span(source, br#""def""#, 0))
            ]
        );
    }

    /// D15:2207, 2369, 2488: without a document, the Shared projection is
    /// unavailable for Phase 1's blocking Findings, and each cause is owned
    /// by the unit of the Finding it names.
    #[test]
    fn the_shared_projection_is_unavailable_for_the_blocking_findings() {
        let fixture = FileFixture::new(&fixture_limits(100));
        let file = fixture.file(Arc::from(&br#"{"x":1,"x":1e999}"#[..]));
        let mut evaluation = Evaluation::default();
        let Ok(Err(causes)) = admit_entry(&mut evaluation, &file, limits()) else {
            panic!("the file has a duplicate member");
        };
        let causes: Vec<IssueKey> = causes.keys().cloned().collect();
        assert_eq!(
            causes
                .iter()
                .map(|key| (key.unit().check, key.code()))
                .collect::<Vec<_>>(),
            [
                (Check::UniqueMembers, Code::JsonMemberDuplicate),
                (Check::JsonCompatible, Code::InputNotJsonCompatible)
            ]
        );
        assert_eq!(causes, evaluation.issues().cloned().collect::<Vec<_>>());
        let views = RecordViews::assign(PROFILE, &evaluation, &Sha256);
        for key in &causes {
            let view = views.shared_unavailability_cause(key).unwrap();
            assert_eq!(view, json!({ "cause": views.finding_key(key).unwrap() }));
            views
                .accounting(key.unit(), RecordKind::SharedUnavailabilityCause, view)
                .unwrap();
        }
    }

    /// Duplicate members and values outside the JSON-compatible domain are
    /// both reported from one parsed file, each reason a Finding of its own,
    /// and the root-value bounds wait on both (D15:802, 2289).
    #[test]
    fn duplicates_and_incompatible_values_are_reported_together_and_hold_back_the_bounds() {
        let (evaluation, admitted) =
            entered(br#"{"x":1,"x":1e999,"y":"\ud800","z":1e999}"#, limits());
        assert!(!admitted);
        assert_eq!(
            found(&evaluation),
            [
                (Check::UniqueMembers, Code::JsonMemberDuplicate, None),
                (
                    Check::JsonCompatible,
                    Code::InputNotJsonCompatible,
                    Some(Reason::InvalidUnicode)
                ),
                (
                    Check::JsonCompatible,
                    Code::InputNotJsonCompatible,
                    Some(Reason::NonPortableNumber)
                ),
            ]
        );
        let held: Vec<(Check, Vec<Check>)> = ROOT_VALUE_BOUNDS
            .iter()
            .map(|&(check, _, _)| {
                (
                    check,
                    vec![
                        Check::UniqueMembers,
                        Check::JsonCompatible,
                        Check::JsonCompatible,
                    ],
                )
            })
            .collect();
        assert_eq!(waiting(&evaluation), held);
        // Either alone holds the bounds back too.
        let (duplicate, _) = entered(br#"{"x":1,"x":2}"#, limits());
        assert_eq!(waiting(&duplicate).len(), 5);
        let (incompatible, _) = entered(b"[1e999]", limits());
        assert_eq!(
            found(&incompatible),
            [(
                Check::JsonCompatible,
                Code::InputNotJsonCompatible,
                Some(Reason::NonPortableNumber)
            )]
        );
        assert_eq!(waiting(&incompatible).len(), 5);
    }

    /// The raw bounds report the file's bytes exactly and the parser's tokens
    /// as their first-over witness, each under its `bootstrapOnly.rawInput`
    /// bound (D15:1999-2006, 2356).
    #[test]
    fn the_raw_bounds_report_their_bootstrap_details() {
        let key =
            |evaluation: &Evaluation| -> IssueKey { evaluation.issues().next().unwrap().clone() };
        let mut small = limits();
        small.raw.max_file_bytes = Bound::new(4).unwrap();
        let (file, _) = entered(b"[1, 2]", small);
        assert_eq!(
            file.bootstrap_limit(&key(&file)),
            Some(BootstrapLimitExceeded {
                bound: BootstrapBoundId {
                    group: BootstrapGroup::RawInput,
                    name: "maxFileBytes",
                },
                limit: Bound::new(4).unwrap(),
                actual: Actual::Exact(6),
            })
        );
        let mut few = limits();
        few.raw.max_parser_tokens = Bound::new(2).unwrap();
        let (tokens, _) = entered(b"[1, 2]", few);
        assert_eq!(
            tokens.bootstrap_limit(&key(&tokens)),
            Some(BootstrapLimitExceeded {
                bound: BootstrapBoundId {
                    group: BootstrapGroup::RawInput,
                    name: "maxParserTokens",
                },
                limit: Bound::new(2).unwrap(),
                actual: Actual::AtLeast(3),
            })
        );
    }

    /// The root-value bounds report every total the complete value exceeds
    /// exactly, and the single-string bound its first-over witness, each under
    /// its `policyComparable.configurationInput` bound (D15:1876, 1884, 2203).
    #[test]
    fn the_root_value_bounds_report_exact_totals_and_the_single_string_witness() {
        let mut over = limits();
        over.value.max_nodes = Bound::new(3).unwrap();
        over.value.max_depth = Bound::new(2).unwrap();
        over.value.max_collection_entries = Bound::new(2).unwrap();
        over.value.max_total_string_bytes = Bound::new(5).unwrap();
        over.value.max_single_string_bytes = Bound::new(2).unwrap();
        let (evaluation, admitted) = entered(br#"{"abc":["def",true]}"#, over);
        assert!(!admitted);
        let limits: Vec<(&str, u64, Actual)> = evaluation
            .issues()
            .map(|key| {
                let LimitExceeded {
                    bound,
                    limit,
                    actual,
                } = evaluation.limit(key).unwrap();
                assert_eq!(bound.group, Group::ConfigurationInput);
                (bound.name, limit.get(), actual)
            })
            .collect();
        assert_eq!(
            limits,
            [
                ("maxNodes", 3, Actual::Exact(4)),
                ("maxDepth", 2, Actual::Exact(3)),
                ("maxCollectionEntries", 2, Actual::Exact(3)),
                ("maxTotalStringBytes", 5, Actual::Exact(6)),
                ("maxSingleStringBytes", 2, Actual::AtLeast(3)),
            ]
        );
    }

    /// Any one root-value bound the complete value exceeds holds the
    /// document back on its own.
    #[test]
    fn each_root_value_bound_alone_holds_the_document_back() {
        for (check, bound, name) in ROOT_VALUE_BOUNDS {
            let mut tight = limits();
            let value = &mut tight.value;
            let limit = match bound {
                InputBound::Nodes => &mut value.max_nodes,
                InputBound::Depth => &mut value.max_depth,
                InputBound::CollectionEntries => &mut value.max_collection_entries,
                InputBound::TotalStringBytes => &mut value.max_total_string_bytes,
                InputBound::SingleStringBytes => &mut value.max_single_string_bytes,
                InputBound::FileBytes | InputBound::ParserTokens => unreachable!("{name}"),
            };
            *limit = Bound::new(1).unwrap();
            let (evaluation, admitted) = entered(br#"{"abc":["def",true]}"#, tight);
            assert!(!admitted, "{name}");
            let checks: Vec<Check> = evaluation.issues().map(|key| key.unit().check).collect();
            assert_eq!(checks, [check], "{name}");
        }
    }
}
