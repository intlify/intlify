// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The Intent Surface-Class Vocabulary Artifact as 015's Phase 5 admits it
//! against the `intent-surface-class-vocabulary` slot, and the surface classes
//! it hands on: what coverage selectors may name and what the coverage
//! decision table spans. 017's reader, in `artifacts::vocabulary`, finds
//! whether the artifact is intact; Phase 5 reports what it found and compares
//! an intact artifact with the slot's assertion.

use intlify_shared_json::token::Token;

use crate::artifacts::vocabulary::{BodyFailure, FormalVocabulary};

use super::evaluation::{
    both, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Phase, Reason,
    Subject, Unit,
};
use super::specification::VocabularyAssertion;

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

/// An admitted vocabulary's members: `IdentityToken`s, unique and in
/// ascending unsigned UTF-8 byte order. The set may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SurfaceVocabulary(Vec<Token>);

impl SurfaceVocabulary {
    pub(crate) fn members(&self) -> &[Token] {
        &self.0
    }

    /// The member `spelling` names, when it names one exactly.
    pub(crate) fn member(&self, spelling: &str) -> Option<&Token> {
        self.0
            .binary_search_by(|member| member.as_str().cmp(spelling))
            .ok()
            .map(|index| &self.0[index])
    }

    /// The vocabulary of `members`, given in any order and each once, standing
    /// in for one Phase 5 admitted.
    #[cfg(test)]
    pub(crate) fn fixture_owned(members: &[&str]) -> Self {
        let mut members: Vec<Token> = members
            .iter()
            .map(|member| Token::new(member).expect("a fixture member is an IdentityToken"))
            .collect();
        members.sort_unstable();
        assert!(
            members.windows(2).all(|pair| pair[0] < pair[1]),
            "a fixture names each member once"
        );
        Self(members)
    }
}

/// 015's reason for each failure of 017's reader. A digest the shared encoding
/// cannot compute leaves the integrity unverified, which 015 reports as the
/// integrity's failure: it gives no other reason.
pub(crate) const fn body_reason(failure: BodyFailure) -> Reason {
    match failure {
        BodyFailure::Integrity | BodyFailure::Unframable => Reason::BodyIntegrityInvalid,
        BodyFailure::MemberInvalid => Reason::MemberInvalid,
        BodyFailure::MemberDuplicate => Reason::MemberDuplicate,
        BodyFailure::NonCanonicalOrder => Reason::NonCanonicalOrder,
    }
}

const fn not_admitted(reason: Reason) -> Blocking {
    Blocking {
        code: Code::IntentSurfaceClassVocabularyNotAdmitted,
        reason: Some(reason),
        limit: None,
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
) -> Result<Admitted<SurfaceVocabulary>, EvaluationInvariant> {
    let intact = evaluation.check(INTACT, policy, |()| {
        artifact
            .admit()
            .map_err(|failure| not_admitted(body_reason(failure)))
    })?;
    evaluation.check(
        ASSERTED,
        both(intact, assertion.map_err(Causes::clone)),
        |(intact, assertion)| {
            if assertion.identity.as_ref() != intact.identity().as_str() {
                return Err(not_admitted(Reason::IdentityMismatch));
            }
            if assertion.revision.as_ref() != intact.revision().as_str() {
                return Err(not_admitted(Reason::RevisionMismatch));
            }
            if assertion.semantic_digest.as_ref() != intact.semantic_digest().as_str() {
                return Err(not_admitted(Reason::DigestMismatch));
            }
            Ok(SurfaceVocabulary(intact.into_members()))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::vectors::{formal, IDENTITY, MEMBERS, REVISION, SEMANTIC_DIGEST};
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
            (BodyFailure::MemberInvalid, "member-invalid"),
            (BodyFailure::MemberDuplicate, "member-duplicate"),
            (BodyFailure::NonCanonicalOrder, "non-canonical-order"),
        ];
        for (failure, reason) in reasons {
            assert_eq!(body_reason(failure).as_str(), reason, "{failure:?}");
        }
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
