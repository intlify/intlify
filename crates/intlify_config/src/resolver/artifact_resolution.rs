// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 9, `artifact-reference-resolution`. Under the confirmed
//! Resource Limit Policy, the complete Submitted Profile Resolution Artifact
//! Collection is admitted into one duplicate-free, conflict-free, canonically
//! ordered Profile Resolution Artifact Set. Every member is checked whether
//! or not a reference reaches it, and the collection's checks are not the
//! bootstrap's, even where they share a code. Ordinary exact resolution of the
//! declaration's references against the Set comes after it.
//!
//! The collection keeps every submitted occurrence for Finding Occurrence
//! multiplicity (D15:1663), so a collection check's Finding is observed once
//! for each occurrence that found it (D15:2387): a member key's checks see
//! every occurrence of every body under the key, and a conflict group's check
//! every occurrence of every body in the group. No Set admitted the member,
//! so each is observed at a `redacted-key` that keeps only a JSON type, and
//! the occurrences of one type are one presentation component, which the
//! ordinals count (D15:834, 838, 2468; plan 025 interpretation #200).

use std::collections::BTreeMap;

use intlify_shared_json::token::Token;

use crate::artifacts::collection::FormalCollection;
use crate::artifacts::content::Content;
use crate::artifacts::envelope::{
    ArtifactKind, Declaration, Envelope, ExactTuple, FormalMember, MemberKey,
};

use super::evaluation::{
    and, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Found,
    Observation, Phase, Reason, Subject, Unit,
};
use super::evidence::EvidenceProjection;
use super::evidence_component::{ComponentInvariant, ComponentPath, Key};
use super::evidence_token::JsonType;
use super::limits::ConfirmedPolicy;
use super::origin::SafeOriginKind;

/// What one Phase 9 collection unit checks. Every member key comes before
/// the conflict groups.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CollectionSubject {
    /// The submitted bodies of one member key: the exact tuple they
    /// establish, or, without one, the body itself.
    Member(MemberKey),
    /// The members sharing a kind, identity, revision, and specification
    /// revision: a conflict group, by those four parts.
    Group(ConflictKey),
}

/// The parts of a tuple its conflict group shares: everything but the
/// semantic digest, in the tuple's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ConflictKey {
    pub(crate) kind: ArtifactKind,
    pub(crate) identity: Token,
    pub(crate) revision: Token,
    pub(crate) specification_revision: Token,
}

impl ConflictKey {
    pub(crate) fn of(tuple: &ExactTuple) -> Self {
        Self {
            kind: tuple.kind,
            identity: tuple.identity.clone(),
            revision: tuple.revision.clone(),
            specification_revision: tuple.specification_revision.clone(),
        }
    }
}

/// The unit Phase 9 records for `check` over a collection `subject`.
pub(crate) const fn unit(subject: CollectionSubject, check: Check) -> Unit {
    Unit {
        phase: Phase::ArtifactReferenceResolution,
        subject: Subject::ArtifactCollection(subject),
        check,
    }
}

/// One admitted artifact of the Set: the submitted member and its body.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AdmittedArtifact<'a> {
    pub(crate) member: &'a FormalMember,
    pub(crate) content: Content<'a>,
}

/// The Profile Resolution Artifact Set: exactly one admitted artifact for each
/// exact tuple, in the tuples' order. Only a complete collection admission
/// creates it, so a later exact lookup can match at most one artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactSet<'a>(BTreeMap<ExactTuple, AdmittedArtifact<'a>>);

impl<'a> ArtifactSet<'a> {
    /// The artifact with `tuple`, and the Set's own copy of the tuple, which
    /// a resolution can then hold for as long as the artifact.
    pub(crate) fn get(&self, tuple: &ExactTuple) -> Option<(&ExactTuple, &AdmittedArtifact<'a>)> {
        self.0.get_key_value(tuple)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (&ExactTuple, &AdmittedArtifact<'a>)> {
        self.0.iter()
    }

    pub(crate) fn len(&self) -> usize {
        self.0.len()
    }
}

/// One distinct submitted body: the member that has it, its envelope when it
/// establishes a tuple, and how many times it was submitted.
struct Distinct<'a> {
    member: &'a FormalMember,
    envelope: Option<Envelope<'a>>,
    occurrences: usize,
}

/// A resolver bug collection admission refuses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CollectionInvariant {
    Evaluation(EvaluationInvariant),
    /// A member's Evidence named a component without a path.
    Component(ComponentInvariant),
}

impl From<EvaluationInvariant> for CollectionInvariant {
    fn from(invariant: EvaluationInvariant) -> Self {
        Self::Evaluation(invariant)
    }
}

impl From<ComponentInvariant> for CollectionInvariant {
    fn from(invariant: ComponentInvariant) -> Self {
        Self::Component(invariant)
    }
}

/// Resolver-input Evidence of a collection member no Set admitted, under a
/// `redacted-key` that keeps only the member's JSON type: for an envelope
/// that establishes a tuple, its tuple view's, an object (D15:834, 2468; plan
/// 025 interpretation #200).
///
/// # Errors
///
/// Returns the resolver bug of a component without a token.
pub(crate) fn unadmitted(member: &FormalMember) -> Result<EvidenceProjection, ComponentInvariant> {
    ComponentPath::artifact_member(Key::Unsafe(JsonType::of(member.value())))
        .map(EvidenceProjection::resolver_input)
}

/// Resolver-input Evidence of an admitted envelope, a member of the Set or
/// the Resource Limit Policy bootstrap admitted, under the canonical
/// exact-reference identity that keys it (D15:824, 834; plan 025
/// interpretation #200).
///
/// # Errors
///
/// Returns the resolver bug of an identity view without canonical bytes.
pub(crate) fn admitted(tuple: &ExactTuple) -> Result<EvidenceProjection, ComponentInvariant> {
    ComponentPath::artifact_member(Key::Identity(tuple.view()))
        .map(EvidenceProjection::resolver_input)
}

/// Resolver-input Evidence of the submitted collection at its root: the one
/// location of every member together (D15:834).
///
/// # Errors
///
/// Returns the resolver bug of a root the origin does not permit.
pub(crate) fn collection_root() -> Result<EvidenceProjection, ComponentInvariant> {
    ComponentPath::root(SafeOriginKind::ProfileResolutionArtifactCollection)
        .map(EvidenceProjection::resolver_input)
}

/// One candidate of `blocking` for each of `occurrences`, each at `member`
/// (D15:2387).
fn each_at(blocking: Blocking, member: &EvidenceProjection, occurrences: usize) -> Found {
    let observed = Observation {
        primary: member.clone(),
        related: Vec::new(),
    };
    Found::each(blocking, vec![Some(observed); occurrences])
}

/// `found` and then `more`.
fn joined(found: Option<Found>, more: Found) -> Found {
    match found {
        Some(found) => found.and(more),
        None => more,
    }
}

const fn not_admitted(reason: Reason) -> Blocking {
    Blocking {
        code: Code::ArtifactCollectionNotAdmitted,
        reason: Some(reason),
        details: None,
    }
}

/// 017's checks of one member, in their order: a member declaring a kind or
/// schema revision the collection does not admit is unsupported; one that
/// establishes no tuple, or whose body admission fails, is an invalid
/// envelope.
fn admit_member<'a>(
    member: &FormalMember,
    envelope: Option<&Envelope<'a>>,
) -> Result<Content<'a>, Blocking> {
    if member.declaration() == Declaration::Unsupported {
        return Err(not_admitted(Reason::KindUnsupported));
    }
    envelope
        .ok_or(not_admitted(Reason::EnvelopeInvalid))?
        .admit()
        .map_err(|_| not_admitted(Reason::EnvelopeInvalid))
}

/// Phase 9's complete collection admission. Each member key is checked once,
/// over every distinct body under it: whether its bodies are admissible
/// members, and, for a tuple, whether each was submitted only once. Each
/// conflict group is checked for one body. The Set exists only when every
/// unit holds, so it then has one body for each tuple.
///
/// # Errors
///
/// Returns the resolver bug of a unit recorded against its phase's rules, or
/// of a member's Evidence without a component path.
pub(crate) fn admit_collection<'a>(
    evaluation: &mut Evaluation,
    confirmed: Result<&ConfirmedPolicy, &Causes>,
    collection: &'a FormalCollection,
) -> Result<Admitted<ArtifactSet<'a>>, CollectionInvariant> {
    let mut keyed: BTreeMap<MemberKey, BTreeMap<&'a [u8], Distinct<'a>>> = BTreeMap::new();
    for member in collection.members() {
        let (key, envelope) = member.keyed();
        keyed
            .entry(key)
            .or_default()
            .entry(member.canonical_body())
            .or_insert(Distinct {
                member,
                envelope,
                occurrences: 0,
            })
            .occurrences += 1;
    }
    let prerequisite = confirmed.map(|_| ()).map_err(Causes::clone);
    // The members of a conflict group establish a tuple, so each is an
    // envelope, a JSON object.
    let envelope = ComponentPath::artifact_member(Key::Unsafe(JsonType::Object))
        .map(EvidenceProjection::resolver_input)?;
    let mut holds: Admitted<()> = Ok(());
    let mut set = BTreeMap::new();
    let mut groups: BTreeMap<ConflictKey, BTreeMap<&[u8], usize>> = BTreeMap::new();
    for (key, bodies) in &keyed {
        let member = |check| unit(CollectionSubject::Member(key.clone()), check);
        // Every body under one key has one JSON type: a body that
        // establishes a tuple is an envelope, and a key without a tuple holds
        // one body.
        let Some(first) = bodies.values().next() else {
            continue;
        };
        let at = unadmitted(first.member)?;
        // Every body is checked, and each that fails is found once for each
        // of its occurrences. Bodies under a tuple fail only as invalid
        // envelopes, so they share one key (plan 025 interpretation #99).
        let admitted = evaluation.check_found(
            member(Check::AdmissibleMember),
            prerequisite.clone(),
            |()| {
                let mut admitted = Vec::new();
                let mut failed = None;
                for distinct in bodies.values() {
                    match admit_member(distinct.member, distinct.envelope.as_ref()) {
                        Ok(content) => admitted.push((distinct.member, content)),
                        Err(blocking) => {
                            let found = each_at(blocking, &at, distinct.occurrences);
                            failed = Some(joined(failed, found));
                        }
                    }
                }
                failed.map_or(Ok(admitted), Err)
            },
        )?;
        let MemberKey::Tuple(tuple) = key else {
            holds = and(holds, admitted.map(|_| ()));
            continue;
        };
        // Each occurrence of a body submitted more than once is a duplicate.
        let single = evaluation.check_found(
            member(Check::SingleOccurrence),
            prerequisite.clone(),
            |()| {
                let duplicate = not_admitted(Reason::DuplicateEnvelope);
                bodies
                    .values()
                    .filter(|distinct| distinct.occurrences > 1)
                    .map(|distinct| each_at(duplicate, &at, distinct.occurrences))
                    .reduce(Found::and)
                    .map_or(Ok(()), Err)
            },
        )?;
        holds = and(holds, single);
        match admitted {
            Ok(mut admitted) if admitted.len() == 1 => {
                let (member, content) = admitted.remove(0);
                set.insert(
                    ExactTuple::clone(tuple),
                    AdmittedArtifact { member, content },
                );
            }
            // Several bodies under one tuple conflict, which their group
            // reports, so none of them enters the Set.
            Ok(_) => {}
            Err(causes) => holds = and(holds, Err(causes)),
        }
        groups.entry(ConflictKey::of(tuple)).or_default().extend(
            bodies
                .iter()
                .map(|(&body, distinct)| (body, distinct.occurrences)),
        );
    }
    for (group, bodies) in &groups {
        // Each occurrence of every body of a group with several conflicts.
        let consistent = evaluation.check_found(
            unit(
                CollectionSubject::Group(group.clone()),
                Check::ConsistentContent,
            ),
            prerequisite.clone(),
            |()| {
                if bodies.len() == 1 {
                    Ok(())
                } else {
                    let occurrences = bodies.values().sum();
                    Err(each_at(
                        not_admitted(Reason::ConflictingContent),
                        &envelope,
                        occurrences,
                    ))
                }
            },
        )?;
        holds = and(holds, consistent);
    }
    Ok(holds.map(|()| ArtifactSet(set)))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, sealed};
    use crate::resolver::evaluation::{IssueKey, Status};
    use crate::resolver::limits::{fixture_limits, ConfirmedPolicy};
    use crate::resolver::token::logical_member;

    const ROOM: CollectionLimits = CollectionLimits {
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    /// A sealed envelope of `kind` with an admitted content.
    fn valid(kind: ArtifactKind, identity: &str) -> Value {
        envelope_of(kind, identity, "1", sample(kind))
    }

    fn collection(values: &[Value]) -> FormalCollection {
        let texts: Vec<String> = values.iter().map(Value::to_string).collect();
        let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
        read_collection(Some(&bytes), &ROOM).unwrap()
    }

    fn confirmed() -> ConfirmedPolicy {
        ConfirmedPolicy::fixture_owned(fixture_limits(1))
    }

    /// Phase 9's collection admission over `values`, the evaluation, and the
    /// tuples of the Set, if there is one.
    fn admit(values: &[Value]) -> (Evaluation, Option<Vec<ExactTuple>>) {
        let collection = collection(values);
        let mut evaluation = Evaluation::default();
        let set = admit_collection(&mut evaluation, Ok(&confirmed()), &collection).unwrap();
        let tuples = set
            .ok()
            .map(|set| set.iter().map(|(tuple, _)| tuple.clone()).collect());
        (evaluation, tuples)
    }

    fn tuple_of(value: &Value) -> ExactTuple {
        FormalMember::new(value.clone())
            .unwrap()
            .envelope()
            .unwrap()
            .tuple()
            .clone()
    }

    fn issues(evaluation: &Evaluation) -> Vec<(CollectionSubject, Check, Reason)> {
        evaluation
            .issues()
            .map(|key: &IssueKey| {
                assert_eq!(key.unit().phase, Phase::ArtifactReferenceResolution);
                let Subject::ArtifactCollection(subject) = key.unit().subject.clone() else {
                    panic!("only Phase 9 collection units")
                };
                assert_eq!(key.code(), Code::ArtifactCollectionNotAdmitted);
                (subject, key.unit().check, key.reason().unwrap())
            })
            .collect()
    }

    fn statuses(evaluation: &Evaluation) -> Vec<(CollectionSubject, Check)> {
        evaluation
            .statuses()
            .map(|(unit, status)| {
                assert_eq!(*status, Status::Evaluated);
                assert_eq!(unit.phase, Phase::ArtifactReferenceResolution);
                let Subject::ArtifactCollection(subject) = unit.subject.clone() else {
                    panic!("only Phase 9 collection units")
                };
                (subject, unit.check)
            })
            .collect()
    }

    /// What each collection Finding candidate was observed at, in the order
    /// the units ran: the unit's subject and check, the reason, and the
    /// primary Evidence. None has related Evidence.
    fn observed(
        evaluation: &Evaluation,
    ) -> Vec<(CollectionSubject, Check, Reason, EvidenceProjection)> {
        evaluation
            .occurrence_candidates()
            .iter()
            .map(|candidate| {
                assert!(candidate.related.is_empty());
                let unit = candidate.finding.unit();
                let Subject::ArtifactCollection(subject) = unit.subject.clone() else {
                    panic!("only Phase 9 collection units")
                };
                let reason = candidate.finding.reason().unwrap();
                (subject, unit.check, reason, candidate.primary.clone())
            })
            .collect()
    }

    /// A member no Set admitted, at a key that keeps only its JSON type.
    fn at(value: &Value) -> EvidenceProjection {
        EvidenceProjection::resolver_input(
            ComponentPath::artifact_member(Key::Unsafe(JsonType::of(value))).unwrap(),
        )
    }

    /// `n` candidates of `check` with `reason` over `subject`, each at a
    /// member of `value`'s JSON type.
    fn times(
        n: usize,
        (subject, check, reason): (&CollectionSubject, Check, Reason),
        value: &Value,
    ) -> Vec<(CollectionSubject, Check, Reason, EvidenceProjection)> {
        vec![(subject.clone(), check, reason, at(value)); n]
    }

    use CollectionSubject::{Group, Member};

    /// The member key the unit of `value` records: its tuple without the
    /// semantic digest, which every body of its conflict group shares, or the
    /// key every member of its JSON type without a tuple shares (Decisions
    /// 015-174 and 015-177).
    fn key(value: &Value) -> MemberKey {
        logical_member(&FormalMember::new(value.clone()).unwrap().key())
    }

    /// The conflict group of `value`, which establishes a tuple.
    fn group(value: &Value) -> ConflictKey {
        ConflictKey::of(&tuple_of(value))
    }

    #[test]
    fn admissible_single_members_form_the_set_in_tuple_order() {
        let members = [
            valid(ArtifactKind::TargetProfile, "storefront-web"),
            valid(ArtifactKind::ApprovalPolicy, "storefront-approval"),
            valid(ArtifactKind::ResourceLimitPolicy, "storefront-limits"),
            valid(ArtifactKind::TrustPolicy, "storefront-trust"),
        ];
        let (evaluation, set) = admit(&members);
        assert!(issues(&evaluation).is_empty());
        let mut tuples = members.iter().map(tuple_of).collect::<Vec<_>>();
        tuples.sort();
        assert_eq!(set, Some(tuples));
        // Each member has both of its checks, and each group one, in the
        // tuples' order.
        let mut sorted = members.to_vec();
        sorted.sort_by_key(tuple_of);
        let expected = sorted
            .iter()
            .flat_map(|value| {
                [
                    (Member(key(value)), Check::AdmissibleMember),
                    (Member(key(value)), Check::SingleOccurrence),
                ]
            })
            .chain(
                sorted
                    .iter()
                    .map(|value| (Group(group(value)), Check::ConsistentContent)),
            )
            .collect::<Vec<_>>();
        assert_eq!(statuses(&evaluation), expected);
        // Submission order is not part of the input.
        let mut reversed = members.to_vec();
        reversed.reverse();
        let (again, set_again) = admit(&reversed);
        assert_eq!(statuses(&again), expected);
        assert_eq!(set_again, set);
    }

    /// The member `value` alone, re-sealed after `edit`.
    fn edited(value: Value, edit: impl FnOnce(&mut Value)) -> Value {
        let mut value = value;
        edit(&mut value);
        sealed(value)
    }

    #[test]
    fn each_member_failure_has_its_reason_and_leaves_no_set() {
        let approval = || valid(ArtifactKind::ApprovalPolicy, "storefront-approval");
        let mut tampered = approval();
        tampered["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        let unsupported = Reason::KindUnsupported;
        let invalid = Reason::EnvelopeInvalid;
        for (member, reason, has_tuple) in [
            // An unknown kind, and a known kind under another schema revision.
            (
                json!({"kind": "localized-message", "schemaRevision": "0"}),
                unsupported,
                false,
            ),
            (
                edited(approval(), |value| value["schemaRevision"] = json!("1")),
                unsupported,
                false,
            ),
            // No kind, a member that is no object, a wrong governing
            // specification, and a body without an object content.
            (json!({"schemaRevision": "0"}), invalid, false),
            (json!("storefront-approval"), invalid, false),
            (
                edited(approval(), |value| {
                    value["specification"]["identity"] = json!("intlify-design-024");
                }),
                invalid,
                false,
            ),
            (
                edited(approval(), |value| value["body"]["content"] = json!([])),
                invalid,
                false,
            ),
            // Body admission: integrity, the specification revision, and the
            // content, each failing after the tuple is established.
            (tampered, invalid, true),
            (
                edited(approval(), |value| {
                    value["specification"]["revision"] = json!("1");
                }),
                invalid,
                true,
            ),
            (
                edited(approval(), |value| {
                    value["body"]["content"] = json!({"localizedApproval": "sometimes"});
                }),
                invalid,
                true,
            ),
            // 018's closed bodies and 024's roles the same way: a Trust
            // Policy naming another authority, a Resource Limit Policy with
            // a member beside its section, and a role 024 does not name.
            (
                edited(
                    valid(ArtifactKind::TrustPolicy, "storefront-trust"),
                    |value| value["body"]["content"] = json!({"authority": "signed-publisher"}),
                ),
                invalid,
                true,
            ),
            (
                edited(
                    valid(ArtifactKind::ResourceLimitPolicy, "storefront-limits"),
                    |value| value["body"]["content"]["translationProvider"] = json!({}),
                ),
                invalid,
                true,
            ),
            (
                edited(
                    valid(ArtifactKind::TargetProfile, "storefront-web"),
                    |value| {
                        value["body"]["content"]["hydration"]["roles"] = json!(["edge-renderer"]);
                    },
                ),
                invalid,
                true,
            ),
        ] {
            let (evaluation, set) = admit(std::slice::from_ref(&member));
            assert_eq!(
                issues(&evaluation),
                [(Member(key(&member)), Check::AdmissibleMember, reason)],
                "{member}"
            );
            // Observed once, at the member's JSON type.
            assert_eq!(
                observed(&evaluation),
                times(
                    1,
                    (&Member(key(&member)), Check::AdmissibleMember, reason),
                    &member
                ),
                "{member}"
            );
            assert_eq!(set, None, "{member}");
            // Only a member with a tuple is checked for repetition and
            // belongs to a group.
            let expected = if has_tuple {
                vec![
                    (Member(key(&member)), Check::AdmissibleMember),
                    (Member(key(&member)), Check::SingleOccurrence),
                    (Group(group(&member)), Check::ConsistentContent),
                ]
            } else {
                vec![(Member(key(&member)), Check::AdmissibleMember)]
            };
            assert_eq!(statuses(&evaluation), expected, "{member}");
        }
    }

    #[test]
    fn a_repeated_body_is_a_duplicate_whether_or_not_it_is_admissible() {
        let approval = valid(ArtifactKind::ApprovalPolicy, "storefront-approval");
        let named = Member(key(&approval));
        let (evaluation, set) = admit(&[approval.clone(), approval.clone()]);
        assert_eq!(
            issues(&evaluation),
            [(
                named.clone(),
                Check::SingleOccurrence,
                Reason::DuplicateEnvelope
            )]
        );
        // Each occurrence is a duplicate, at the envelope's JSON type.
        let duplicate = (&named, Check::SingleOccurrence, Reason::DuplicateEnvelope);
        assert_eq!(observed(&evaluation), times(2, duplicate, &approval));
        assert_eq!(set, None);
        // A repeated invalid body is still a duplicate envelope.
        let mut tampered = approval;
        tampered["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        let (evaluation, _) = admit(&[tampered.clone(), tampered.clone()]);
        assert_eq!(
            issues(&evaluation),
            [
                (
                    named.clone(),
                    Check::AdmissibleMember,
                    Reason::EnvelopeInvalid
                ),
                (
                    named.clone(),
                    Check::SingleOccurrence,
                    Reason::DuplicateEnvelope
                ),
            ]
        );
        let invalid = (&named, Check::AdmissibleMember, Reason::EnvelopeInvalid);
        assert_eq!(
            observed(&evaluation),
            [times(2, invalid, &tampered), times(2, duplicate, &tampered)].concat()
        );
        // A repeated member without a tuple is an invalid envelope, once.
        let lone = json!({"schemaRevision": "0"});
        let (evaluation, _) = admit(&[lone.clone(), lone.clone()]);
        assert_eq!(
            issues(&evaluation),
            [(
                Member(key(&lone)),
                Check::AdmissibleMember,
                Reason::EnvelopeInvalid
            )]
        );
        assert_eq!(
            statuses(&evaluation),
            [(Member(key(&lone)), Check::AdmissibleMember)]
        );
        // Its one key holds the body twice, each occurrence observed.
        let lone_invalid = (
            &Member(key(&lone)),
            Check::AdmissibleMember,
            Reason::EnvelopeInvalid,
        );
        assert_eq!(observed(&evaluation), times(2, lone_invalid, &lone));
    }

    #[test]
    fn members_sharing_all_but_the_digest_or_the_body_conflict() {
        let first = valid(ArtifactKind::ApprovalPolicy, "storefront-approval");
        let other = envelope_of(
            ArtifactKind::ApprovalPolicy,
            "storefront-approval",
            "1",
            json!({"localizedApproval": "not-required"}),
        );
        assert_ne!(tuple_of(&first), tuple_of(&other));
        let (evaluation, set) = admit(&[first.clone(), other]);
        assert_eq!(
            issues(&evaluation),
            [(
                Group(group(&first)),
                Check::ConsistentContent,
                Reason::ConflictingContent
            )]
        );
        // Each member of the group is a conflicting occurrence.
        let conflict = (
            &Group(group(&first)),
            Check::ConsistentContent,
            Reason::ConflictingContent,
        );
        assert_eq!(observed(&evaluation), times(2, conflict, &first));
        assert_eq!(set, None);
        // The same tuple with another body: the digest pins the content, not
        // the integrity value, which here no longer holds. Both bodies are
        // the tuple's one unit, which the invalid one fails.
        let mut tampered = first.clone();
        tampered["integrityDigest"] = json!(format!("sha256:{}", "1".repeat(64)));
        assert_eq!(tuple_of(&first), tuple_of(&tampered));
        let (evaluation, _) = admit(&[first.clone(), tampered]);
        assert_eq!(
            issues(&evaluation),
            [
                (
                    Member(key(&first)),
                    Check::AdmissibleMember,
                    Reason::EnvelopeInvalid
                ),
                (
                    Group(group(&first)),
                    Check::ConsistentContent,
                    Reason::ConflictingContent
                ),
            ]
        );
        let invalid = (
            &Member(key(&first)),
            Check::AdmissibleMember,
            Reason::EnvelopeInvalid,
        );
        assert_eq!(
            observed(&evaluation),
            [times(1, invalid, &first), times(2, conflict, &first)].concat()
        );
        // Another revision is another group, not a conflict.
        let next = envelope_of(
            ArtifactKind::ApprovalPolicy,
            "storefront-approval",
            "2",
            sample(ArtifactKind::ApprovalPolicy),
        );
        let (evaluation, set) = admit(&[
            valid(ArtifactKind::ApprovalPolicy, "storefront-approval"),
            next,
        ]);
        assert!(issues(&evaluation).is_empty());
        assert_eq!(set.map(|tuples| tuples.len()), Some(2));
        // So is another specification revision, even one no body is admitted
        // under: only that member is invalid.
        let later = edited(
            valid(ArtifactKind::ApprovalPolicy, "storefront-approval"),
            |value| value["specification"]["revision"] = json!("1"),
        );
        let (evaluation, _) = admit(&[
            later.clone(),
            valid(ArtifactKind::ApprovalPolicy, "storefront-approval"),
        ]);
        assert_eq!(
            issues(&evaluation),
            [(
                Member(key(&later)),
                Check::AdmissibleMember,
                Reason::EnvelopeInvalid
            )]
        );
        assert!(statuses(&evaluation).contains(&(Group(group(&later)), Check::ConsistentContent)));
    }

    #[test]
    fn members_with_a_tuple_come_before_members_without_one() {
        let lone = json!({"schemaRevision": "0"});
        let trust = valid(ArtifactKind::TrustPolicy, "storefront-trust");
        let (evaluation, _) = admit(&[lone.clone(), trust.clone()]);
        assert_eq!(
            statuses(&evaluation),
            [
                (Member(key(&trust)), Check::AdmissibleMember),
                (Member(key(&trust)), Check::SingleOccurrence),
                (Member(key(&lone)), Check::AdmissibleMember),
                (Group(group(&trust)), Check::ConsistentContent),
            ]
        );
        assert_eq!(
            issues(&evaluation),
            [(
                Member(key(&lone)),
                Check::AdmissibleMember,
                Reason::EnvelopeInvalid
            )]
        );
    }

    #[test]
    fn the_bodies_of_one_tuple_share_its_units() {
        // One body twice and another once, under one tuple: the invalid body,
        // the repeated one, and the conflict each fail one unit. The invalid
        // body is found on either side of the valid one in the bodies' order.
        let first = valid(ArtifactKind::ApprovalPolicy, "storefront-approval");
        let named = Member(key(&first));
        for digit in ["0", "f"] {
            let mut tampered = first.clone();
            tampered["integrityDigest"] = json!(format!("sha256:{}", digit.repeat(64)));
            assert_eq!(key(&tampered), key(&first));
            let (evaluation, set) = admit(&[first.clone(), tampered, first.clone()]);
            // The invalid body once, the repeated one twice, and all three
            // occurrences in the group.
            assert_eq!(
                observed(&evaluation),
                [
                    times(
                        1,
                        (&named, Check::AdmissibleMember, Reason::EnvelopeInvalid),
                        &first
                    ),
                    times(
                        2,
                        (&named, Check::SingleOccurrence, Reason::DuplicateEnvelope),
                        &first
                    ),
                    times(
                        3,
                        (
                            &Group(group(&first)),
                            Check::ConsistentContent,
                            Reason::ConflictingContent
                        ),
                        &first
                    ),
                ]
                .concat(),
                "{digit}"
            );
            assert_eq!(
                statuses(&evaluation),
                [
                    (named.clone(), Check::AdmissibleMember),
                    (named.clone(), Check::SingleOccurrence),
                    (Group(group(&first)), Check::ConsistentContent),
                ],
                "{digit}"
            );
            assert_eq!(
                issues(&evaluation),
                [
                    (
                        named.clone(),
                        Check::AdmissibleMember,
                        Reason::EnvelopeInvalid
                    ),
                    (
                        named.clone(),
                        Check::SingleOccurrence,
                        Reason::DuplicateEnvelope
                    ),
                    (
                        Group(group(&first)),
                        Check::ConsistentContent,
                        Reason::ConflictingContent
                    ),
                ],
                "{digit}"
            );
            assert_eq!(set, None, "{digit}");
        }
        // Two invalid bodies under one tuple: each fails the tuple's one
        // unit, and each is observed, not only the first in the bodies'
        // order.
        let tampered = |digit: &str| {
            let mut value = first.clone();
            value["integrityDigest"] = json!(format!("sha256:{}", digit.repeat(64)));
            value
        };
        let (evaluation, _) = admit(&[tampered("0"), tampered("f")]);
        let conflict = (
            &Group(group(&first)),
            Check::ConsistentContent,
            Reason::ConflictingContent,
        );
        assert_eq!(
            observed(&evaluation),
            [
                times(
                    2,
                    (&named, Check::AdmissibleMember, Reason::EnvelopeInvalid),
                    &first
                ),
                times(2, conflict, &first),
            ]
            .concat()
        );
        // The two orders really differ: the valid body sorts between them.
        let body = |digit: &str| {
            let mut value = first.clone();
            value["integrityDigest"] = json!(format!("sha256:{}", digit.repeat(64)));
            FormalMember::new(value).unwrap().canonical_body().to_vec()
        };
        let valid_body = FormalMember::new(first.clone())
            .unwrap()
            .canonical_body()
            .to_vec();
        assert!(body("0") < valid_body && valid_body < body("f"));
    }

    #[test]
    fn without_a_confirmed_policy_nothing_is_admitted() {
        use crate::resolver::resource_policy::{unit as policy_unit, PolicySubject};

        // What Phase 4 leaves when no submitted artifact is the policy, made
        // up without what its check observed.
        let mut other = Evaluation::fabricated();
        let causes = other
            .check(
                policy_unit(PolicySubject::ResourceLimits, Check::BootstrapCandidate),
                Ok(()),
                |()| {
                    Err::<(), _>(Blocking {
                        code: Code::ArtifactReferenceNotAdmitted,
                        reason: Some(Reason::NotFound),
                        details: None,
                    })
                },
            )
            .unwrap()
            .unwrap_err();
        let collection = collection(&[
            valid(ArtifactKind::TrustPolicy, "storefront-trust"),
            json!({"schemaRevision": "0"}),
        ]);
        let mut evaluation = Evaluation::default();
        let set = admit_collection(&mut evaluation, Err(&causes), &collection).unwrap();
        assert_eq!(set, Err(causes.clone()));
        assert_eq!(evaluation.issues().count(), 0);
        assert_eq!(evaluation.statuses().count(), 4);
        assert!(evaluation
            .statuses()
            .all(|(_, status)| *status == Status::NotEvaluated(causes.clone())));
    }

    #[test]
    fn the_set_holds_each_admitted_body_under_its_tuple() {
        use crate::artifacts::content::LocalizedApproval;

        let approval = valid(ArtifactKind::ApprovalPolicy, "storefront-approval");
        let limits = valid(ArtifactKind::ResourceLimitPolicy, "storefront-limits");
        let collection = collection(&[limits.clone(), approval.clone()]);
        let mut evaluation = Evaluation::default();
        let set = admit_collection(&mut evaluation, Ok(&confirmed()), &collection)
            .unwrap()
            .unwrap();
        assert_eq!(set.len(), 2);
        let (held, admitted) = set.get(&tuple_of(&approval)).unwrap();
        assert_eq!(held, &tuple_of(&approval));
        assert_eq!(
            admitted.content,
            Content::Approval(LocalizedApproval::Required)
        );
        assert_eq!(admitted.member.value(), &approval);
        // A Resource Limit Policy's content is 018's common body: its
        // section, whose interior only 015 reads.
        let (_, admitted) = set.get(&tuple_of(&limits)).unwrap();
        let sample = sample(ArtifactKind::ResourceLimitPolicy);
        assert_eq!(
            admitted.content,
            Content::ResourceLimitPolicy(sample["projectProfileResolution"].as_object())
        );
        assert_eq!(
            set.get(&tuple_of(&valid(ArtifactKind::TrustPolicy, "x"))),
            None
        );
    }
}
