// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Intent Surface-Class Vocabulary Artifact as 015's Phase 5 admits it
//! against the `intent-surface-class-vocabulary` slot, and the surface classes
//! it hands on: what coverage selectors may name and what the coverage
//! decision table spans. 017's reader, in `artifacts::vocabulary`, finds
//! whether the artifact is intact; Phase 5 reports what it found and compares
//! an intact artifact with the slot's assertion.

use intlify_shared_json::token::Token;
use serde_json::Value;

use crate::artifacts::vocabulary::{BodyFailure, FormalVocabulary, IntactVocabulary};
use crate::artifacts::SemanticDigest;

use super::evaluation::{
    both, Admitted, Blocking, Causes, Check, Code, Evaluation, Found, Observation, Phase, Reason,
    Subject, Unit,
};
use super::evidence::EvidenceProjection;
use super::evidence_component::{ComponentInvariant, ComponentPath, Key};
use super::evidence_token::JsonType;
use super::specification::{Slot, SpecificationInvariant, VocabularyAssertion};

/// The unit that reports what 017's reader found in the artifact itself.
pub(crate) const INTACT: Unit = Unit {
    phase: Phase::SpecificationAdmission,
    subject: Subject::Vocabulary,
    check: Check::IntactArtifact,
};

/// The unit that compares an intact artifact with the slot's assertion.
pub(crate) const ASSERTED: Unit = Unit {
    phase: Phase::SpecificationAdmission,
    subject: Subject::Vocabulary,
    check: Check::AssertedArtifact,
};

/// An admitted vocabulary: the identity, revision, and semantic digest its
/// slot asserts, and its members, `IdentityToken`s unique and in ascending
/// unsigned UTF-8 byte order. The set may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SurfaceVocabulary {
    identity: Token,
    revision: Token,
    semantic_digest: SemanticDigest,
    members: Vec<Token>,
}

impl SurfaceVocabulary {
    /// An intact artifact, once found to be the one its slot asserts.
    fn admitted(intact: IntactVocabulary) -> Self {
        Self {
            identity: intact.identity().clone(),
            revision: intact.revision().clone(),
            semantic_digest: intact.semantic_digest().clone(),
            members: intact.into_members(),
        }
    }

    pub(crate) const fn identity(&self) -> &Token {
        &self.identity
    }

    pub(crate) const fn revision(&self) -> &Token {
        &self.revision
    }

    pub(crate) const fn semantic_digest(&self) -> &SemanticDigest {
        &self.semantic_digest
    }

    pub(crate) fn members(&self) -> &[Token] {
        &self.members
    }

    /// The member `spelling` names, when it names one exactly.
    pub(crate) fn member(&self, spelling: &str) -> Option<&Token> {
        self.members
            .binary_search_by(|member| member.as_str().cmp(spelling))
            .ok()
            .map(|index| &self.members[index])
    }

    /// The fixture vocabulary with `members`, given in any order and each
    /// once, standing in for one Phase 5 admitted: sealed under the identity
    /// and revision the vocabulary vectors give it, read, and found intact.
    #[cfg(any(test, feature = "benchmark"))]
    pub(crate) fn fixture_owned(members: &[&str]) -> Self {
        use crate::artifacts::vocabulary::{
            read, IntentSurfaceClassVocabularyArtifact, VocabularyLimits,
        };
        let mut members = members.to_vec();
        members.sort_unstable();
        assert!(
            members.windows(2).all(|pair| pair[0] < pair[1]),
            "a fixture names each member once"
        );
        let artifact =
            IntentSurfaceClassVocabularyArtifact::sealed("intlify-test-vocabulary", "0", &members)
                .expect("a fixture vocabulary has an integrity digest");
        let room = VocabularyLimits {
            artifact_bytes: u64::MAX,
            depth: u64::MAX,
            members: u64::MAX,
            member_bytes: u64::MAX,
            total_member_bytes: u64::MAX,
        };
        let bytes = serde_json::to_vec(&artifact).expect("a sealed vocabulary serializes");
        let intact = read(Some(&bytes), &room)
            .expect("a fixture vocabulary is within its room")
            .admit()
            .expect("a fixture member is an IdentityToken");
        Self::admitted(intact)
    }
}

/// 015's reason for each failure of 017's reader. A digest the shared encoding
/// cannot compute leaves the integrity unverified, which 015 reports as the
/// integrity's failure: it gives no other reason.
pub(crate) const fn body_reason(failure: &BodyFailure) -> Reason {
    match failure {
        BodyFailure::Integrity | BodyFailure::Unframable => Reason::BodyIntegrityInvalid,
        BodyFailure::MemberInvalid(_) => Reason::MemberInvalid,
        BodyFailure::MemberDuplicate(_) => Reason::MemberDuplicate,
        BodyFailure::NonCanonicalOrder => Reason::NonCanonicalOrder,
    }
}

/// What 017's reader found, as the candidates of the artifact's own check
/// (D15:832, 1569): each member that is not an `IdentityToken` at the member,
/// a string under a key that keeps only its JSON type (plan 025
/// interpretation #200); each occurrence of a repeated member at the member,
/// under its canonical vocabulary identity; and a failure of the integrity or
/// the order at the artifact's root.
///
/// # Errors
///
/// Returns the resolver bug of a member without a component path.
fn intact_failure(failure: &BodyFailure) -> Result<Found, ComponentInvariant> {
    let blocking = not_admitted(body_reason(failure));
    let at = |primary| {
        Found::observed(
            blocking,
            Observation {
                primary,
                related: Vec::new(),
            },
        )
    };
    let members = match failure {
        BodyFailure::MemberInvalid(invalid) => {
            let member = EvidenceProjection::resolver_input(ComponentPath::vocabulary_member(
                Key::Unsafe(JsonType::String),
            )?);
            invalid.iter().map(|_| member.clone()).collect()
        }
        BodyFailure::MemberDuplicate(repeated) => repeated
            .iter()
            .map(|member| {
                ComponentPath::vocabulary_member(Key::Identity(Value::from(member.as_str())))
                    .map(EvidenceProjection::resolver_input)
            })
            .collect::<Result<Vec<_>, _>>()?,
        BodyFailure::Integrity | BodyFailure::Unframable | BodyFailure::NonCanonicalOrder => {
            vec![EvidenceProjection::resolver_input(
                ComponentPath::vocabulary_root(),
            )]
        }
    };
    Ok(members
        .into_iter()
        .map(at)
        .reduce(Found::and)
        .unwrap_or_else(|| Found::from(blocking)))
}

const fn not_admitted(reason: Reason) -> Blocking {
    Blocking {
        code: Code::IntentSurfaceClassVocabularyNotAdmitted,
        reason: Some(reason),
        details: None,
    }
}

/// Phase 5's admission of the vocabulary artifact, under the Resource Limit
/// Policy Phase 4 confirmed. The artifact's own failures need nothing but the
/// artifact, so they are reported whether or not the slot holds an assertion.
/// Only an intact artifact is compared with the assertion: its identity, then
/// its revision, then its semantic digest, each spelled exactly.
pub(crate) fn admit_vocabulary<Id: AsRef<str>>(
    evaluation: &mut Evaluation,
    policy: Admitted<()>,
    assertion: Result<&VocabularyAssertion<Id>, &Causes>,
    artifact: &FormalVocabulary,
) -> Result<Admitted<SurfaceVocabulary>, SpecificationInvariant> {
    // The reader runs under the policy, and what it found is observed before
    // the check records it, which cannot return the resolver's own bugs.
    let read = match policy {
        Ok(()) => Ok(match artifact.admit() {
            Ok(intact) => Ok(intact),
            Err(failure) => Err(intact_failure(&failure)?),
        }),
        Err(causes) => Err(causes),
    };
    let intact = evaluation.check_found(INTACT, read, |read| read)?;
    // A mismatch is observed at the artifact, beside the slot's assertion it
    // was compared with (D15:832, 2249).
    let mismatch = |reason| {
        Found::observed(
            not_admitted(reason),
            Observation {
                primary: EvidenceProjection::resolver_input(ComponentPath::vocabulary_root()),
                related: vec![EvidenceProjection::resolver_input(
                    ComponentPath::specification(Slot::IntentSurfaceClassVocabulary),
                )],
            },
        )
    };
    Ok(evaluation.check_found(
        ASSERTED,
        both(intact, assertion.map_err(Causes::clone)),
        |(intact, assertion)| {
            if assertion.identity.as_ref() != intact.identity().as_str() {
                return Err(mismatch(Reason::IdentityMismatch));
            }
            if assertion.revision.as_ref() != intact.revision().as_str() {
                return Err(mismatch(Reason::RevisionMismatch));
            }
            if assertion.semantic_digest.as_ref() != intact.semantic_digest().as_str() {
                return Err(mismatch(Reason::DigestMismatch));
            }
            Ok(SurfaceVocabulary::admitted(intact))
        },
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::vectors::vocabulary::{
        formal, IDENTITY, MEMBERS, REVISION, SEMANTIC_DIGEST,
    };
    use crate::artifacts::vocabulary::{
        read, IntentSurfaceClassVocabularyArtifact, VocabularyLimits,
    };
    use crate::resolver::evaluation::Status;
    use crate::resolver::locales::fixtures::{stand_in, units};
    use crate::resolver::specification::Slot;

    const ASSERTION: VocabularyAssertion<&str> = VocabularyAssertion {
        identity: IDENTITY,
        revision: REVISION,
        semantic_digest: SEMANTIC_DIGEST,
    };

    fn sealed(members: &[&str]) -> FormalVocabulary {
        let artifact =
            IntentSurfaceClassVocabularyArtifact::sealed(IDENTITY, REVISION, members).unwrap();
        let limits = VocabularyLimits {
            artifact_bytes: u64::MAX,
            depth: u64::MAX,
            members: u64::MAX,
            member_bytes: u64::MAX,
            total_member_bytes: u64::MAX,
        };
        read(Some(&serde_json::to_vec(&artifact).unwrap()), &limits).unwrap()
    }

    fn admit(
        evaluation: &mut Evaluation,
        policy: Admitted<()>,
        assertion: Result<&VocabularyAssertion<&str>, &Causes>,
        artifact: &FormalVocabulary,
    ) -> Admitted<SurfaceVocabulary> {
        admit_vocabulary(evaluation, policy, assertion, artifact).unwrap()
    }

    /// Every issue as its unit, code, and reason.
    fn issues(evaluation: &Evaluation) -> Vec<(Unit, &'static str, Option<&'static str>)> {
        evaluation
            .issues()
            .map(|key| {
                (
                    key.unit().clone(),
                    key.code().as_str(),
                    key.reason().map(Reason::as_str),
                )
            })
            .collect()
    }

    #[test]
    fn members_are_sorted_by_their_bytes() {
        let vocabulary = SurfaceVocabulary::fixture_owned(&["nav", "checkout", "internal-tool"]);
        assert_eq!(
            vocabulary
                .members()
                .iter()
                .map(Token::as_str)
                .collect::<Vec<_>>(),
            ["checkout", "internal-tool", "nav"]
        );
        assert!(SurfaceVocabulary::fixture_owned(&[]).members().is_empty());
    }

    #[test]
    fn a_member_is_found_only_by_its_exact_spelling() {
        let vocabulary = SurfaceVocabulary::fixture_owned(&["checkout", "nav"]);
        assert_eq!(vocabulary.member("nav").map(Token::as_str), Some("nav"));
        for other in ["Nav", "na", "nav ", "help", ""] {
            assert_eq!(vocabulary.member(other), None);
        }
    }

    #[test]
    fn the_vocabulary_the_slot_asserts_is_admitted_with_its_members() {
        let mut evaluation = Evaluation::default();
        let admitted = admit(&mut evaluation, Ok(()), Ok(&ASSERTION), &formal()).unwrap();
        assert_eq!(admitted, SurfaceVocabulary::fixture_owned(&MEMBERS));
        assert!(issues(&evaluation).is_empty());
        for unit in [INTACT, ASSERTED] {
            assert_eq!(evaluation.status(&unit), Some(&Status::Evaluated));
        }
    }

    #[test]
    fn each_reader_failure_is_its_015_reason() {
        let reasons = [
            (BodyFailure::Integrity, "body-integrity-invalid"),
            (BodyFailure::Unframable, "body-integrity-invalid"),
            (BodyFailure::MemberInvalid(vec![0]), "member-invalid"),
            (BodyFailure::MemberDuplicate(Vec::new()), "member-duplicate"),
            (BodyFailure::NonCanonicalOrder, "non-canonical-order"),
        ];
        for (failure, reason) in reasons {
            assert_eq!(body_reason(&failure).as_str(), reason, "{failure:?}");
        }
    }

    /// The intact check's occurrences over `artifact`, each as the
    /// component of its primary Evidence and its ordinal, and the component
    /// a string member no Set admitted takes under the same tokens.
    fn intact_occurrences(
        artifact: &FormalVocabulary,
    ) -> (Vec<(serde_json::Value, u64)>, serde_json::Value) {
        use crate::resolver::evidence_token::EvidenceTokens;
        use crate::resolver::occurrence::{occurrences, redacted};
        use crate::resolver::token::{Sha256, Specification};
        const PROFILE: Specification<'static> = Specification {
            identity: "intlify-project-profile",
            revision: "0",
        };
        let mut evaluation = Evaluation::default();
        admit(&mut evaluation, Ok(()), Ok(&ASSERTION), artifact).unwrap_err();
        let string = EvidenceProjection::resolver_input(
            ComponentPath::vocabulary_member(Key::Unsafe(JsonType::String)).unwrap(),
        );
        let candidates = evaluation.occurrence_candidates();
        let tokens = EvidenceTokens::assign(
            PROFILE,
            redacted(candidates).chain(string.redacted()),
            &Sha256,
        );
        let found = occurrences(candidates, PROFILE, &tokens)
            .unwrap()
            .iter()
            .map(|occurrence| {
                assert_eq!(occurrence.owner(), &INTACT);
                assert!(occurrence.related().is_empty());
                let view = occurrence.primary().view();
                assert_eq!(view["kind"], "artifact-input", "{view}");
                assert_eq!(view["origin"], "intent-surface-class-vocabulary", "{view}");
                (view["component"].clone(), occurrence.ordinal())
            })
            .collect();
        let string = string.retained(PROFILE, &tokens).unwrap().view()["component"].clone();
        (found, string)
    }

    /// What the reader found is observed where it is: each invalid member at
    /// the member, a string under a key that keeps only its JSON type; each
    /// occurrence of a repeated member at the member, under its identity; and
    /// a broken integrity or order at the artifact's root (D15:832, 1569; plan
    /// 025 interpretation #200).
    #[test]
    fn what_the_reader_found_is_observed_where_it_is() {
        use crate::resolver::evidence_source::hex;
        use intlify_shared_json::encoding::encode;
        use serde_json::json;
        // Two invalid members are one redacted member, told apart by their
        // ordinals.
        let (invalid, string) = intact_occurrences(&sealed(&["Nav", "checkout", "nav "]));
        assert_eq!(invalid, [(string.clone(), 0), (string.clone(), 1)]);
        assert_eq!(string[0], json!({ "fixed-role": "vocabulary-member" }));
        assert!(string[1].get("redacted-key").is_some(), "{string}");
        // Every occurrence of each repeated member, and no other member.
        let identity = |member: &str| {
            json!([
                { "fixed-role": "vocabulary-member" },
                { "safe-key": hex(&encode(&json!(member)).unwrap()) },
            ])
        };
        let (mut repeated, _) = intact_occurrences(&sealed(&[
            "nav",
            "checkout",
            "nav",
            "checkout",
            "internal-tool",
        ]));
        repeated.sort_by(|left, right| {
            left.0
                .to_string()
                .cmp(&right.0.to_string())
                .then(left.1.cmp(&right.1))
        });
        let mut expected = vec![
            (identity("checkout"), 0),
            (identity("checkout"), 1),
            (identity("nav"), 0),
            (identity("nav"), 1),
        ];
        expected.sort_by(|left, right| {
            left.0
                .to_string()
                .cmp(&right.0.to_string())
                .then(left.1.cmp(&right.1))
        });
        assert_eq!(repeated, expected);
        // The order and the integrity are the artifact's, at its root.
        assert_eq!(
            intact_occurrences(&sealed(&["nav", "checkout"])).0,
            [(json!([]), 0)]
        );
        let mut stale = serde_json::to_value(
            IntentSurfaceClassVocabularyArtifact::sealed(IDENTITY, REVISION, &["checkout"])
                .unwrap(),
        )
        .unwrap();
        stale["members"] = json!(["nav"]);
        let limits = VocabularyLimits {
            artifact_bytes: u64::MAX,
            depth: u64::MAX,
            members: u64::MAX,
            member_bytes: u64::MAX,
            total_member_bytes: u64::MAX,
        };
        let stale = read(Some(&serde_json::to_vec(&stale).unwrap()), &limits).unwrap();
        assert_eq!(intact_occurrences(&stale).0, [(json!([]), 0)]);
    }

    #[test]
    fn an_artifact_that_is_not_intact_is_reported_and_never_compared() {
        let cases: [(&[&str], &str); 3] = [
            (&["checkout", "Nav"], "member-invalid"),
            (&["nav", "checkout", "nav"], "member-duplicate"),
            (&["nav", "checkout"], "non-canonical-order"),
        ];
        for (members, reason) in cases {
            let mut evaluation = Evaluation::default();
            let causes =
                admit(&mut evaluation, Ok(()), Ok(&ASSERTION), &sealed(members)).unwrap_err();
            assert_eq!(
                issues(&evaluation),
                [(
                    INTACT,
                    "project-profile-intent-surface-class-vocabulary-not-admitted",
                    Some(reason)
                )],
                "{members:?}"
            );
            assert_eq!(units(&causes), [INTACT]);
            assert_eq!(evaluation.status(&INTACT), Some(&Status::Evaluated));
            assert_eq!(
                evaluation.status(&ASSERTED),
                Some(&Status::NotEvaluated(causes))
            );
        }
    }

    #[test]
    fn the_assertion_is_compared_by_identity_then_revision_then_digest() {
        let cases = [
            (
                VocabularyAssertion {
                    identity: "intlify-other-vocabulary",
                    revision: "1",
                    semantic_digest: "sha256:other",
                },
                "identity-mismatch",
            ),
            (
                VocabularyAssertion {
                    identity: "Intlify-Test-Vocabulary",
                    ..ASSERTION
                },
                "identity-mismatch",
            ),
            (
                VocabularyAssertion {
                    revision: "1",
                    semantic_digest: "sha256:other",
                    ..ASSERTION
                },
                "revision-mismatch",
            ),
            (
                VocabularyAssertion {
                    semantic_digest: "sha256:other",
                    ..ASSERTION
                },
                "digest-mismatch",
            ),
        ];
        for (assertion, reason) in cases {
            let mut evaluation = Evaluation::default();
            let causes = admit(&mut evaluation, Ok(()), Ok(&assertion), &formal()).unwrap_err();
            assert_eq!(
                issues(&evaluation),
                [(
                    ASSERTED,
                    "project-profile-intent-surface-class-vocabulary-not-admitted",
                    Some(reason)
                )],
                "{assertion:?}"
            );
            assert_eq!(units(&causes), [ASSERTED]);
        }
    }

    #[test]
    fn a_missing_slot_hides_only_the_comparison() {
        let missing = stand_in(Slot::IntentSurfaceClassVocabulary);
        let mut evaluation = Evaluation::default();
        let causes = admit(&mut evaluation, Ok(()), Err(&missing), &formal()).unwrap_err();
        assert_eq!(causes, missing);
        assert!(issues(&evaluation).is_empty());
        assert_eq!(evaluation.status(&INTACT), Some(&Status::Evaluated));
        assert_eq!(
            evaluation.status(&ASSERTED),
            Some(&Status::NotEvaluated(missing.clone()))
        );
        // The artifact's own failure is still found, and the comparison waits
        // on both.
        let mut evaluation = Evaluation::default();
        let causes = admit(
            &mut evaluation,
            Ok(()),
            Err(&missing),
            &sealed(&["nav", "checkout"]),
        )
        .unwrap_err();
        assert_eq!(
            issues(&evaluation),
            [(
                INTACT,
                "project-profile-intent-surface-class-vocabulary-not-admitted",
                Some("non-canonical-order")
            )]
        );
        assert_eq!(units(&causes), [stand_in_unit(), INTACT]);
    }

    fn stand_in_unit() -> Unit {
        Unit {
            phase: Phase::SpecificationAdmission,
            subject: Subject::Slot(Slot::IntentSurfaceClassVocabulary),
            check: Check::SupportedAssertion,
        }
    }

    #[test]
    fn an_unconfirmed_policy_leaves_both_units_not_evaluated() {
        let unconfirmed = stand_in(Slot::Coverage);
        let mut evaluation = Evaluation::default();
        let causes = admit(
            &mut evaluation,
            Err(unconfirmed.clone()),
            Ok(&ASSERTION),
            &sealed(&["nav", "checkout"]),
        )
        .unwrap_err();
        assert_eq!(causes, unconfirmed);
        assert!(issues(&evaluation).is_empty());
        for unit in [INTACT, ASSERTED] {
            assert_eq!(
                evaluation.status(&unit),
                Some(&Status::NotEvaluated(unconfirmed.clone()))
            );
        }
    }
}
