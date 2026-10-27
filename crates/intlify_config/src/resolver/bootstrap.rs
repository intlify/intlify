// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Bootstrap Resource Policy Candidate Admission: for the provisionally
//! selected declaration's exact `resourceLimits` reference, the closed decision
//! over the submitted collection that admits exactly one Resource Limit Policy,
//! or the one Finding code and reason that blocks it. It creates no Profile
//! Resolution Artifact Set and depends on no submission order.
//!
//! The decision accounts for nothing and records nothing. Phase 4 runs it after
//! the capability's `artifactAdmission` bounds and records its result, then
//! compares the admitted policy with the capability and rechecks the earlier
//! phases under it. The `bootstrapOnly.artifactBootstrap` work bounds wait for
//! the Capability Specification that closes their units; the scan and the
//! verification below are the work they will bound.
//!
//! 018 defines neither the Resource Limit Policy's common body nor its
//! Verification Input, so the resolver takes 018's admission of the candidate
//! as a seam that only tests implement.

use serde_json::{Map, Value};

use crate::artifacts::envelope::{
    ArtifactKind, Envelope, ExactTuple, FormalMember, SPECIFICATION_REVISION,
};

use super::evaluation::{Blocking, Code, Reason};
use super::limits::{ResolutionLimits, SECTION};

/// 018's admission of the Resource Limit Policy candidate under the explicit
/// Resource Limit Policy Verification Input.
pub(crate) trait ResourcePolicyVerification {
    /// Whether 018's trust and signature admission accepts the candidate.
    fn authenticates(&self, candidate: &Envelope<'_>) -> bool;

    /// Whether 018's common body schema allows the content's members.
    fn admits_common_body(&self, content: &Map<String, Value>) -> bool;
}

/// The admitted Resource Limit Policy: its exact reference and every bound of
/// its `projectProfileResolution` section, which 015 records together.
#[derive(Debug, Clone)]
pub(crate) struct AdmittedResourcePolicy {
    tuple: ExactTuple,
    limits: ResolutionLimits,
}

impl AdmittedResourcePolicy {
    pub(crate) const fn tuple(&self) -> &ExactTuple {
        &self.tuple
    }

    pub(crate) const fn limits(&self) -> &ResolutionLimits {
        &self.limits
    }
}

const fn issue(code: Code, reason: Reason) -> Blocking {
    Blocking {
        code,
        reason: Some(reason),
        details: None,
    }
}

/// 015's closed decision for the selected `reference`. The reference is
/// checked before any candidate; then only members that establish a tuple
/// are grouped: the conflict group shares the reference's kind, identity,
/// revision, and specification revision, and the exact-reference group also
/// shares its semantic digest. A member without a tuple is never guessed into
/// either group.
pub(crate) fn admit_candidate(
    reference: &ExactTuple,
    members: &[FormalMember],
    verification: &impl ResourcePolicyVerification,
) -> Result<AdmittedResourcePolicy, Blocking> {
    let not_admitted = |reason| issue(Code::ArtifactReferenceNotAdmitted, reason);
    reference_checks(reference, ArtifactKind::ResourceLimitPolicy).map_err(not_admitted)?;
    let envelopes: Vec<Envelope<'_>> = members.iter().filter_map(FormalMember::envelope).collect();
    let conflict_group: Vec<&Envelope<'_>> = envelopes
        .iter()
        .filter(|envelope| {
            let tuple = envelope.tuple();
            tuple.kind == reference.kind
                && tuple.identity == reference.identity
                && tuple.revision == reference.revision
                && tuple.specification_revision == reference.specification_revision
        })
        .collect();
    // Equal canonical bodies have equal semantic digests, so two different
    // bodies are conflicting content whether or not their digests differ.
    if let Some(first) = conflict_group.first() {
        let body = first.member().canonical_body();
        if conflict_group
            .iter()
            .any(|envelope| envelope.member().canonical_body() != body)
        {
            return Err(issue(
                Code::ArtifactCollectionNotAdmitted,
                Reason::ConflictingContent,
            ));
        }
    }
    let exact_group: Vec<&Envelope<'_>> = conflict_group
        .iter()
        .copied()
        .filter(|envelope| envelope.tuple().semantic_digest == reference.semantic_digest)
        .collect();
    match exact_group.as_slice() {
        [] => {
            let tuples: Vec<&ExactTuple> = envelopes.iter().map(Envelope::tuple).collect();
            Err(not_admitted(mismatch(reference, &tuples)))
        }
        [candidate] => admit_body(candidate, verification),
        [_, _, ..] => Err(issue(
            Code::ArtifactCollectionNotAdmitted,
            Reason::DuplicateEnvelope,
        )),
    }
}

/// The body admission of the sole exact candidate. Its specification revision
/// is the reference's, which was found supported, and 018 admits its content,
/// so 017's body admission comes down to the integrity.
fn admit_body(
    candidate: &Envelope<'_>,
    verification: &impl ResourcePolicyVerification,
) -> Result<AdmittedResourcePolicy, Blocking> {
    let not_admitted = |reason| issue(Code::ResourcePolicyNotAdmitted, reason);
    if !candidate.integrity_holds() || !verification.authenticates(candidate) {
        return Err(not_admitted(Reason::BodyIntegrityInvalid));
    }
    let content = candidate.content();
    let section = content.get(SECTION);
    // 017 records that 018's common body schema requires a present section to
    // be an object; which other members a content may have is 018's.
    if section.is_some_and(|section| !section.is_object())
        || !verification.admits_common_body(content)
    {
        return Err(not_admitted(Reason::BodySchemaInvalid));
    }
    let section = section.ok_or_else(|| not_admitted(Reason::ProjectProfileResolutionMissing))?;
    let limits = ResolutionLimits::read(section)
        .ok_or_else(|| not_admitted(Reason::ProjectProfileResolutionInvalid))?;
    Ok(AdmittedResourcePolicy {
        tuple: candidate.tuple().clone(),
        limits,
    })
}

/// The two checks of a reference itself, made before any candidate is
/// examined: its kind is the one its role takes, and the resolver supports
/// the specification revision it requests.
pub(crate) fn reference_checks(reference: &ExactTuple, role: ArtifactKind) -> Result<(), Reason> {
    if reference.kind != role {
        return Err(Reason::RoleKindMismatch);
    }
    if reference.specification_revision.as_str() != SPECIFICATION_REVISION {
        return Err(Reason::SpecificationRevisionUnsupported);
    }
    Ok(())
}

/// The closest tier some candidate occupies when none of `tuples` is the
/// reference's. Each tier is tried only after every closer one is empty, so a
/// candidate there also differs in what the closer tiers share: one sharing
/// everything but the digest differs in the digest.
pub(crate) fn mismatch(reference: &ExactTuple, tuples: &[&ExactTuple]) -> Reason {
    let any = |predicate: &dyn Fn(&ExactTuple) -> bool| tuples.iter().any(|tuple| predicate(tuple));
    let kind = |tuple: &ExactTuple| tuple.kind == reference.kind;
    let identity = |tuple: &ExactTuple| tuple.identity == reference.identity;
    let revision = |tuple: &ExactTuple| tuple.revision == reference.revision;
    let specification =
        |tuple: &ExactTuple| tuple.specification_revision == reference.specification_revision;
    if any(&|tuple| kind(tuple) && identity(tuple) && revision(tuple) && specification(tuple)) {
        Reason::DigestMismatch
    } else if any(&|tuple| kind(tuple) && identity(tuple) && revision(tuple)) {
        Reason::SpecificationRevisionMismatch
    } else if any(&|tuple| kind(tuple) && identity(tuple)) {
        Reason::RevisionMismatch
    } else if any(&identity) {
        Reason::KindMismatch
    } else if any(&kind) {
        Reason::IdentityMismatch
    } else {
        Reason::NotFound
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::artifacts::envelope::{envelope_of, sealed};
    use crate::resolver::limits::fixture_limits;

    /// 018's admission as a test states it.
    struct Verifier {
        authentic: bool,
        common_body: bool,
    }

    impl ResourcePolicyVerification for Verifier {
        fn authenticates(&self, _: &Envelope<'_>) -> bool {
            self.authentic
        }

        fn admits_common_body(&self, _: &Map<String, Value>) -> bool {
            self.common_body
        }
    }

    const TRUSTED: Verifier = Verifier {
        authentic: true,
        common_body: true,
    };

    /// A sealed Resource Limit Policy whose section sets the fixture bounds
    /// counting up from `first`.
    fn policy(identity: &str, revision: &str, first: u64) -> Value {
        envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            identity,
            revision,
            json!({ SECTION: fixture_limits(first).section() }),
        )
    }

    /// `value` with the member at `pointer` set, or removed for `None`.
    fn edited(value: &Value, pointer: &str, replacement: Option<Value>) -> Value {
        let mut edited = value.clone();
        let (parent, name) = pointer.rsplit_once('/').unwrap();
        let object = edited.pointer_mut(parent).unwrap().as_object_mut().unwrap();
        match replacement {
            Some(replacement) => object.insert(name.into(), replacement),
            None => object.remove(name),
        };
        edited
    }

    /// `value` with an integrity digest no body has.
    fn tampered(value: &Value) -> Value {
        edited(
            value,
            "/integrityDigest",
            Some(json!(format!("sha256:{}", "e".repeat(64)))),
        )
    }

    fn tuple_of(value: &Value) -> ExactTuple {
        FormalMember::new(value.clone())
            .unwrap()
            .envelope()
            .unwrap()
            .tuple()
            .clone()
    }

    /// The decision, as the admitted tuple or the code and reason.
    fn decide(
        reference: &ExactTuple,
        values: &[Value],
        verification: &Verifier,
    ) -> Result<ExactTuple, (Code, Reason)> {
        let members: Vec<FormalMember> = values
            .iter()
            .map(|value| FormalMember::new(value.clone()).unwrap())
            .collect();
        admit_candidate(reference, &members, verification)
            .map(|policy| policy.tuple().clone())
            .map_err(|blocking| (blocking.code, blocking.reason.unwrap()))
    }

    const fn reference_issue(reason: Reason) -> (Code, Reason) {
        (Code::ArtifactReferenceNotAdmitted, reason)
    }

    const fn collection_issue(reason: Reason) -> (Code, Reason) {
        (Code::ArtifactCollectionNotAdmitted, reason)
    }

    const fn policy_issue(reason: Reason) -> (Code, Reason) {
        (Code::ResourcePolicyNotAdmitted, reason)
    }

    #[test]
    fn the_sole_exact_candidate_is_admitted_with_its_tuple_and_bounds() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        let members = [
            policy("other-limits", "1", 20),
            envelope_of(
                ArtifactKind::TrustPolicy,
                "storefront-limits",
                "1",
                json!({}),
            ),
            json!({"kind": "resource-limit-policy"}),
            chosen.clone(),
        ];
        let verified = members.map(|value| FormalMember::new(value).unwrap());
        let admitted = admit_candidate(&reference, &verified, &TRUSTED).unwrap();
        assert_eq!(admitted.tuple(), &reference);
        assert_eq!(admitted.limits().bounds(), fixture_limits(10).bounds());
    }

    #[test]
    fn the_reference_is_checked_before_any_candidate() {
        // A trust policy that the resourceLimits role names is the wrong kind,
        // even with its exact artifact submitted.
        let trust = envelope_of(ArtifactKind::TrustPolicy, "trust", "1", json!({}));
        assert_eq!(
            decide(&tuple_of(&trust), &[trust], &TRUSTED),
            Err(reference_issue(Reason::RoleKindMismatch))
        );
        // A requested specification revision other than "0" is unsupported,
        // even with an artifact under that revision submitted.
        let revised = sealed(edited(
            &policy("storefront-limits", "1", 10),
            "/specification/revision",
            Some(json!("1")),
        ));
        assert_eq!(
            decide(&tuple_of(&revised), &[revised], &TRUSTED),
            Err(reference_issue(Reason::SpecificationRevisionUnsupported))
        );
        // The role comes first.
        let both = sealed(edited(
            &envelope_of(ArtifactKind::TrustPolicy, "trust", "1", json!({})),
            "/specification/revision",
            Some(json!("1")),
        ));
        assert_eq!(
            decide(&tuple_of(&both), &[], &TRUSTED),
            Err(reference_issue(Reason::RoleKindMismatch))
        );
    }

    #[test]
    fn different_bodies_under_one_name_are_conflicting_content_first() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        let conflicting = Err(collection_issue(Reason::ConflictingContent));
        // Another content under the same name, with another semantic digest.
        let other_content = policy("storefront-limits", "1", 11);
        assert_ne!(tuple_of(&other_content), reference);
        assert_eq!(
            decide(
                &reference,
                &[chosen.clone(), other_content.clone()],
                &TRUSTED
            ),
            conflicting
        );
        // The same content, and so the same tuple, with another body.
        let other_body = tampered(&chosen);
        assert_eq!(tuple_of(&other_body), reference);
        assert_eq!(
            decide(&reference, &[chosen.clone(), other_body.clone()], &TRUSTED),
            conflicting
        );
        // Before duplicates, and before any body is admitted.
        assert_eq!(
            decide(
                &reference,
                &[chosen.clone(), chosen.clone(), other_content.clone()],
                &TRUSTED
            ),
            conflicting
        );
        assert_eq!(
            decide(
                &reference,
                &[tampered(&other_content), other_body],
                &Verifier {
                    authentic: false,
                    common_body: false,
                }
            ),
            conflicting
        );
        // Without an exact candidate too.
        assert_eq!(
            decide(
                &reference,
                &[other_content.clone(), tampered(&other_content)],
                &TRUSTED
            ),
            conflicting
        );
    }

    #[test]
    fn repeated_exact_occurrences_are_duplicates_before_any_body_is_admitted() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        let duplicate = Err(collection_issue(Reason::DuplicateEnvelope));
        assert_eq!(
            decide(&reference, &[chosen.clone(), chosen.clone()], &TRUSTED),
            duplicate
        );
        // Bodies that would fail their integrity or section are still
        // duplicates.
        let stale = tampered(&chosen);
        assert_eq!(
            decide(&reference, &[stale.clone(), stale.clone(), stale], &TRUSTED),
            duplicate
        );
        let unsectioned = envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "storefront-limits",
            "1",
            json!({}),
        );
        assert_eq!(
            decide(
                &tuple_of(&unsectioned),
                &[unsectioned.clone(), unsectioned],
                &TRUSTED
            ),
            duplicate
        );
    }

    #[test]
    fn the_sole_candidates_body_is_admitted_in_015s_order() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        let verifier = |authentic, common_body| Verifier {
            authentic,
            common_body,
        };
        let with_content = |content: Value| sealed(edited(&chosen, "/body/content", Some(content)));
        let decide_alone = |value: &Value, verification: &Verifier| {
            decide(&tuple_of(value), std::slice::from_ref(value), verification)
        };
        let integrity = Err(policy_issue(Reason::BodyIntegrityInvalid));
        let schema = Err(policy_issue(Reason::BodySchemaInvalid));
        let missing = Err(policy_issue(Reason::ProjectProfileResolutionMissing));
        let invalid = Err(policy_issue(Reason::ProjectProfileResolutionInvalid));
        // 017's integrity, then 018's trust and signature.
        assert_eq!(
            decide(&reference, &[tampered(&chosen)], &TRUSTED),
            integrity
        );
        assert_eq!(
            decide(
                &reference,
                std::slice::from_ref(&chosen),
                &verifier(false, true)
            ),
            integrity
        );
        let no_section = with_content(json!({}));
        assert_eq!(
            decide_alone(&tampered(&no_section), &verifier(true, false)),
            integrity
        );
        // 018's common body schema, which keeps a present section an object.
        assert_eq!(
            decide(
                &reference,
                std::slice::from_ref(&chosen),
                &verifier(true, false)
            ),
            schema
        );
        assert_eq!(
            decide_alone(&with_content(json!({ SECTION: "default" })), &TRUSTED),
            schema
        );
        assert_eq!(decide_alone(&no_section, &verifier(true, false)), schema);
        // Then 015's section: present, and then valid.
        assert_eq!(decide_alone(&no_section, &TRUSTED), missing);
        assert_eq!(
            decide_alone(&with_content(json!({ SECTION: {} })), &TRUSTED),
            invalid
        );
        let mut section = fixture_limits(10).section();
        section["configurationInput"]["maxNodes"] = json!("0");
        assert_eq!(
            decide_alone(&with_content(json!({ SECTION: section })), &TRUSTED),
            invalid
        );
        // Other members of the content are 018's to allow.
        let extra = with_content(json!({
            SECTION: fixture_limits(10).section(),
            "translationProvider": {"maxRequests": "8"},
        }));
        assert_eq!(decide_alone(&extra, &TRUSTED), Ok(tuple_of(&extra)));
    }

    /// One candidate for each mismatch tier against the reference
    /// `(resource-limit-policy, storefront-limits, "1", "0", digest of 10)`,
    /// closest first.
    fn tiers() -> Vec<(Reason, Value)> {
        let revised = |revision: &str| {
            sealed(edited(
                &policy("storefront-limits", "1", 10),
                "/specification/revision",
                Some(json!(revision)),
            ))
        };
        vec![
            (Reason::DigestMismatch, policy("storefront-limits", "1", 11)),
            (Reason::SpecificationRevisionMismatch, revised("1")),
            (
                Reason::RevisionMismatch,
                policy("storefront-limits", "2", 10),
            ),
            (
                Reason::KindMismatch,
                envelope_of(
                    ArtifactKind::TrustPolicy,
                    "storefront-limits",
                    "1",
                    json!({}),
                ),
            ),
            (Reason::IdentityMismatch, policy("other-limits", "1", 10)),
        ]
    }

    #[test]
    fn without_an_exact_candidate_the_closest_tier_decides() {
        let reference = tuple_of(&policy("storefront-limits", "1", 10));
        let tiers = tiers();
        for index in 0..tiers.len() {
            // This tier's candidate together with one of every farther tier.
            let candidates: Vec<Value> = tiers[index..]
                .iter()
                .map(|(_, candidate)| candidate.clone())
                .collect();
            assert_eq!(
                decide(&reference, &candidates, &TRUSTED),
                Err(reference_issue(tiers[index].0)),
                "{:?}",
                tiers[index].0
            );
        }
        // Nothing related at all.
        let unrelated = envelope_of(ArtifactKind::GlossarySet, "glossary", "1", json!({}));
        assert_eq!(
            decide(&reference, &[unrelated], &TRUSTED),
            Err(reference_issue(Reason::NotFound))
        );
        assert_eq!(
            decide(&reference, &[], &TRUSTED),
            Err(reference_issue(Reason::NotFound))
        );
        // Several candidates in one tier still select that one reason.
        assert_eq!(
            decide(
                &reference,
                &[
                    policy("storefront-limits", "2", 10),
                    policy("storefront-limits", "3", 12),
                ],
                &TRUSTED
            ),
            Err(reference_issue(Reason::RevisionMismatch))
        );
        // A digest mismatch is decided whatever the other candidate's body.
        assert_eq!(
            decide(
                &reference,
                &[tampered(&policy("storefront-limits", "1", 11))],
                &Verifier {
                    authentic: false,
                    common_body: false,
                }
            ),
            Err(reference_issue(Reason::DigestMismatch))
        );
    }

    #[test]
    fn a_member_without_a_tuple_joins_no_group() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        let not_found = Err(reference_issue(Reason::NotFound));
        for member in [
            edited(&chosen, "/integrityDigest", None),
            edited(&chosen, "/integrityDigest", Some(json!("sha256:00"))),
            edited(&chosen, "/schemaRevision", Some(json!("1"))),
            edited(
                &chosen,
                "/specification/identity",
                Some(json!("intlify-design-021")),
            ),
            edited(&chosen, "/body/content", Some(json!([]))),
            json!(["resource-limit-policy", "storefront-limits"]),
        ] {
            assert_eq!(
                decide(&reference, std::slice::from_ref(&member), &TRUSTED),
                not_found
            );
            // Nor does it stop the exact candidate from being admitted alone.
            assert_eq!(
                decide(&reference, &[member, chosen.clone()], &TRUSTED),
                Ok(reference.clone())
            );
        }
    }

    /// Every ordering of `values`.
    fn permutations(values: &[Value]) -> Vec<Vec<Value>> {
        if values.len() <= 1 {
            return vec![values.to_vec()];
        }
        let mut orderings = Vec::new();
        for index in 0..values.len() {
            let mut rest = values.to_vec();
            let first = rest.remove(index);
            for mut tail in permutations(&rest) {
                tail.insert(0, first.clone());
                orderings.push(tail);
            }
        }
        orderings
    }

    #[test]
    fn the_decision_does_not_depend_on_submission_order() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        let tiers: Vec<Value> = tiers()
            .into_iter()
            .map(|(_, candidate)| candidate)
            .collect();
        let scenarios = [
            // Admitted among related candidates and a member without a tuple.
            vec![
                chosen.clone(),
                tiers[2].clone(),
                tiers[4].clone(),
                json!({"kind": "resource-limit-policy"}),
            ],
            // Conflicting, duplicate, and each mismatch tier.
            vec![
                chosen.clone(),
                chosen.clone(),
                tiers[0].clone(),
                tiers[3].clone(),
            ],
            vec![
                chosen.clone(),
                chosen.clone(),
                tiers[1].clone(),
                tiers[4].clone(),
            ],
            tiers[1..].to_vec(),
            vec![tiers[3].clone(), tiers[4].clone(), tiers[2].clone()],
        ];
        for scenario in scenarios {
            let expected = decide(&reference, &scenario, &TRUSTED);
            for ordering in permutations(&scenario) {
                assert_eq!(decide(&reference, &ordering, &TRUSTED), expected);
            }
        }
    }

    #[test]
    fn the_envelope_vectors_resource_limit_policies_reach_their_checks() {
        let vectors: Value =
            serde_json::from_str(include_str!("../../fixtures/envelope/vectors.json")).unwrap();
        let member = |id: &str| {
            vectors["members"]
                .as_array()
                .unwrap()
                .iter()
                .find(|case| case["id"] == id)
                .unwrap()["member"]
                .clone()
        };
        for (id, reason) in [
            (
                "resource-limit-policy-without-section",
                Reason::ProjectProfileResolutionMissing,
            ),
            (
                "resource-limit-policy-non-object-section",
                Reason::BodySchemaInvalid,
            ),
            (
                "resource-limit-policy-invalid-section",
                Reason::ProjectProfileResolutionInvalid,
            ),
            // The sample section names a bound 015 does not define.
            (
                "resource-limit-policy",
                Reason::ProjectProfileResolutionInvalid,
            ),
        ] {
            let value = member(id);
            assert_eq!(
                decide(&tuple_of(&value), &[value], &TRUSTED),
                Err(policy_issue(reason)),
                "{id}"
            );
        }
    }
}
