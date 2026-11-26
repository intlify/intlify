// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Bootstrap Resource Policy Candidate Admission: for the provisionally
//! selected declaration's exact `resourceLimits` reference, the closed decision
//! over the submitted collection that admits exactly one Resource Limit Policy,
//! or the one Finding code and reason that blocks it. It creates no Profile
//! Resolution Artifact Set and depends on no submission order.
//!
//! The decision accounts for nothing and records nothing. When it admits no
//! candidate, it says what it compared the reference with, which Phase 4
//! observes the Finding through (D15:2250). Phase 4 runs it as
//! two checks, candidate selection and the chosen candidate's body admission,
//! after the capability's `artifactAdmission` bounds, then compares the
//! admitted policy with the capability and rechecks the earlier phases under
//! it. [`work`] counts the `bootstrapOnly.artifactBootstrap` work bounds that
//! Phase 4 checks around the decision (Decision 015-164): the metadata scan
//! before candidate selection, and the chosen candidate's envelope depth,
//! decoded allocation, and validation work before its body admission.
//!
//! 018 authenticates the candidate by membership in its exact-pin
//! Verification Input, which 015's pre-invocation boundary admits
//! (`crate::boundary::verification`), and closes its common body to the
//! optional `projectProfileResolution` section.

pub(crate) mod work;

use crate::artifacts::content;
use crate::artifacts::envelope::{
    ArtifactKind, Envelope, ExactTuple, FormalMember, SPECIFICATION_REVISION,
};
use crate::boundary::verification::VerificationInput;

use super::evaluation::{Blocking, Code, Reason};
use super::limits::{ResolutionLimits, SECTION};

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

/// Why candidate selection admitted no candidate: the blocking result, and
/// what the decision compared the selected reference with.
#[derive(Debug, Clone)]
pub(crate) struct Rejected<'m> {
    pub(crate) blocking: Blocking,
    pub(crate) compared: Compared<'m>,
}

/// What candidate selection compared the selected reference with when it
/// admitted no candidate (D15:2250).
#[derive(Debug, Clone)]
pub(crate) enum Compared<'m> {
    /// Nothing submitted: the reference failed its own checks first.
    Nothing,
    /// The submitted collection as one location, the set the reference must
    /// name a member of: no member shares even its kind or identity.
    Collection,
    /// The candidates of the closest tier, each a mismatched candidate.
    Tier(Vec<&'m FormalMember>),
    /// Every member of the selected group, each one of its conflicting or
    /// duplicate occurrences.
    Group(Vec<&'m FormalMember>),
}

/// 015's candidate selection for the selected `reference`, steps 1 to 3 and 9
/// to 14 of its closed decision. The reference is checked before any candidate
/// is classified; then only members that establish a tuple are grouped: the
/// conflict group shares the reference's kind, identity, revision, and
/// specification revision, and the exact-reference group also shares its
/// semantic digest. A member without a tuple is never guessed into either
/// group. The sole exact candidate it chooses is measured by the
/// `artifactBootstrap` bounds before its body admission, a check of its own
/// (Decision 015-164). Without one, it returns what it compared the reference
/// with.
pub(crate) fn select_candidate<'m>(
    reference: &ExactTuple,
    members: &'m [FormalMember],
) -> Result<Envelope<'m>, Rejected<'m>> {
    let not_admitted = |reason| issue(Code::ArtifactReferenceNotAdmitted, reason);
    let group = |reason, group: &[&Envelope<'m>]| Rejected {
        blocking: issue(Code::ArtifactCollectionNotAdmitted, reason),
        compared: Compared::Group(group.iter().map(|envelope| envelope.member()).collect()),
    };
    reference_checks(reference, ArtifactKind::ResourceLimitPolicy).map_err(|reason| Rejected {
        blocking: not_admitted(reason),
        compared: Compared::Nothing,
    })?;
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
            return Err(group(Reason::ConflictingContent, &conflict_group));
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
            let (reason, tier) = mismatch(reference, &tuples);
            let compared = if tier.is_empty() {
                Compared::Collection
            } else {
                Compared::Tier(
                    tier.iter()
                        .map(|&index| envelopes[index].member())
                        .collect(),
                )
            };
            Err(Rejected {
                blocking: not_admitted(reason),
                compared,
            })
        }
        [candidate] => Ok((*candidate).clone()),
        duplicates @ [_, _, ..] => Err(group(Reason::DuplicateEnvelope, duplicates)),
    }
}

/// The body admission of the sole exact candidate, steps 4 to 8 of the
/// decision. Its specification revision
/// is the reference's, which was found supported, so 017's body admission
/// comes down to the integrity and 018's admission: the pin, then the common
/// body.
pub(crate) fn admit_body(
    candidate: &Envelope<'_>,
    verification: &VerificationInput,
) -> Result<AdmittedResourcePolicy, Blocking> {
    let not_admitted = |reason| issue(Code::ResourcePolicyNotAdmitted, reason);
    if !candidate.integrity_holds() || !verification.authenticates(candidate) {
        return Err(not_admitted(Reason::BodyIntegrityInvalid));
    }
    let content = candidate.content();
    if !content::resource_limit_policy(content) {
        return Err(not_admitted(Reason::BodySchemaInvalid));
    }
    let section = content
        .get(SECTION)
        .ok_or_else(|| not_admitted(Reason::ProjectProfileResolutionMissing))?;
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

/// A mismatch tier's reason, and whether a candidate is in the tier.
type Tier<'t> = (Reason, &'t dyn Fn(&ExactTuple) -> bool);

/// The closest tier some candidate occupies when none of `tuples` is the
/// reference's, with the index of every candidate in it. Each tier is tried
/// only after every closer one is empty, so a candidate there also differs in
/// what the closer tiers share: one sharing everything but the digest differs
/// in the digest. When no candidate is in any tier, the reference names
/// nothing submitted.
pub(crate) fn mismatch(reference: &ExactTuple, tuples: &[&ExactTuple]) -> (Reason, Vec<usize>) {
    let kind = |tuple: &ExactTuple| tuple.kind == reference.kind;
    let identity = |tuple: &ExactTuple| tuple.identity == reference.identity;
    let revision = |tuple: &ExactTuple| tuple.revision == reference.revision;
    let specification =
        |tuple: &ExactTuple| tuple.specification_revision == reference.specification_revision;
    // Each tier's candidates share with the reference what its name leaves
    // out of the mismatch.
    let but_digest = |tuple: &ExactTuple| {
        kind(tuple) && identity(tuple) && revision(tuple) && specification(tuple)
    };
    let but_specification = |tuple: &ExactTuple| kind(tuple) && identity(tuple) && revision(tuple);
    let but_revision = |tuple: &ExactTuple| kind(tuple) && identity(tuple);
    let tiers: [Tier<'_>; 5] = [
        (Reason::DigestMismatch, &but_digest),
        (Reason::SpecificationRevisionMismatch, &but_specification),
        (Reason::RevisionMismatch, &but_revision),
        (Reason::KindMismatch, &identity),
        (Reason::IdentityMismatch, &kind),
    ];
    tiers
        .iter()
        .find_map(|&(reason, tier)| {
            let candidates: Vec<usize> = (0..tuples.len())
                .filter(|&index| tier(tuples[index]))
                .collect();
            (!candidates.is_empty()).then_some((reason, candidates))
        })
        .unwrap_or((Reason::NotFound, Vec::new()))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::envelope::{envelope_of, sealed};
    use crate::resolver::limits::fixture_limits;

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

    /// 018's exact-pin input as trusted setup supplies it.
    fn pinning(pins: &[&Value]) -> VerificationInput {
        VerificationInput::fixture_owned(pins.iter().map(|value| tuple_of(value)))
    }

    /// The input that pins `reference` itself, or another policy when the
    /// reference names no Resource Limit Policy.
    fn trusting(reference: &ExactTuple) -> VerificationInput {
        let pin = if reference.kind == ArtifactKind::ResourceLimitPolicy {
            reference.clone()
        } else {
            tuple_of(&policy("pinned-limits", "1", 1))
        };
        VerificationInput::fixture_owned([pin])
    }

    /// The decision under an input pinning the reference, as the admitted
    /// tuple or the code and reason.
    fn decide(reference: &ExactTuple, values: &[Value]) -> Result<ExactTuple, (Code, Reason)> {
        decide_with(reference, values, &trusting(reference))
    }

    fn decide_with(
        reference: &ExactTuple,
        values: &[Value],
        verification: &VerificationInput,
    ) -> Result<ExactTuple, (Code, Reason)> {
        let members: Vec<FormalMember> = values
            .iter()
            .map(|value| FormalMember::new(value.clone()).unwrap())
            .collect();
        admit_candidate(reference, &members, verification)
            .map(|policy| policy.tuple().clone())
            .map_err(|blocking| (blocking.code, blocking.reason.unwrap()))
    }

    /// The whole decision: candidate selection, then the candidate's body
    /// admission, which Phase 4 runs as two checks.
    fn admit_candidate(
        reference: &ExactTuple,
        members: &[FormalMember],
        verification: &VerificationInput,
    ) -> Result<AdmittedResourcePolicy, Blocking> {
        select_candidate(reference, members)
            .map_err(|rejected| rejected.blocking)
            .and_then(|candidate| admit_body(&candidate, verification))
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
                json!({"authority": "trusted-local-host"}),
            ),
            json!({"kind": "resource-limit-policy"}),
            chosen.clone(),
        ];
        let verified = members.map(|value| FormalMember::new(value).unwrap());
        let admitted = admit_candidate(&reference, &verified, &trusting(&reference)).unwrap();
        assert_eq!(admitted.tuple(), &reference);
        assert_eq!(admitted.limits().bounds(), fixture_limits(10).bounds());
    }

    #[test]
    fn the_reference_is_checked_before_any_candidate() {
        // A trust policy that the resourceLimits role names is the wrong kind,
        // even with its exact artifact submitted.
        let trust = envelope_of(ArtifactKind::TrustPolicy, "trust", "1", json!({}));
        assert_eq!(
            decide(&tuple_of(&trust), &[trust]),
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
            decide(&tuple_of(&revised), &[revised]),
            Err(reference_issue(Reason::SpecificationRevisionUnsupported))
        );
        // The role comes first.
        let both = sealed(edited(
            &envelope_of(ArtifactKind::TrustPolicy, "trust", "1", json!({})),
            "/specification/revision",
            Some(json!("1")),
        ));
        assert_eq!(
            decide(&tuple_of(&both), &[]),
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
            decide(&reference, &[chosen.clone(), other_content.clone()]),
            conflicting
        );
        // The same content, and so the same tuple, with another body.
        let other_body = tampered(&chosen);
        assert_eq!(tuple_of(&other_body), reference);
        assert_eq!(
            decide(&reference, &[chosen.clone(), other_body.clone()]),
            conflicting
        );
        // Before duplicates, and before any body is admitted.
        assert_eq!(
            decide(
                &reference,
                &[chosen.clone(), chosen.clone(), other_content.clone()]
            ),
            conflicting
        );
        assert_eq!(
            decide_with(
                &reference,
                &[tampered(&other_content), other_body],
                &pinning(&[&policy("pinned-limits", "1", 1)])
            ),
            conflicting
        );
        // Without an exact candidate too.
        assert_eq!(
            decide(
                &reference,
                &[other_content.clone(), tampered(&other_content)]
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
            decide(&reference, &[chosen.clone(), chosen.clone()]),
            duplicate
        );
        // Bodies that would fail their integrity or section are still
        // duplicates.
        let stale = tampered(&chosen);
        assert_eq!(
            decide(&reference, &[stale.clone(), stale.clone(), stale]),
            duplicate
        );
        let unsectioned = envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "storefront-limits",
            "1",
            json!({}),
        );
        assert_eq!(
            decide(&tuple_of(&unsectioned), &[unsectioned.clone(), unsectioned]),
            duplicate
        );
    }

    #[test]
    fn the_sole_candidates_body_is_admitted_in_015s_order() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        let with_content = |content: Value| sealed(edited(&chosen, "/body/content", Some(content)));
        let decide_alone = |value: &Value| decide(&tuple_of(value), std::slice::from_ref(value));
        let integrity = Err(policy_issue(Reason::BodyIntegrityInvalid));
        let schema = Err(policy_issue(Reason::BodySchemaInvalid));
        let missing = Err(policy_issue(Reason::ProjectProfileResolutionMissing));
        let invalid = Err(policy_issue(Reason::ProjectProfileResolutionInvalid));
        // 017's integrity, then 018's pin: another policy's, or one that
        // differs only in its digest, authenticates nothing.
        assert_eq!(decide(&reference, &[tampered(&chosen)]), integrity);
        for pin in [
            policy("other-limits", "1", 10),
            policy("storefront-limits", "1", 11),
        ] {
            assert_eq!(
                decide_with(&reference, std::slice::from_ref(&chosen), &pinning(&[&pin])),
                integrity
            );
        }
        let extra = with_content(json!({
            SECTION: fixture_limits(10).section(),
            "translationProvider": {"maxRequests": "8"},
        }));
        assert_eq!(decide_alone(&tampered(&extra)), integrity);
        // 018's common body: nothing but the section, an object.
        assert_eq!(decide_alone(&extra), schema);
        assert_eq!(
            decide_alone(&with_content(json!({"translationProvider": {}}))),
            schema
        );
        assert_eq!(
            decide_alone(&with_content(json!({ SECTION: "default" }))),
            schema
        );
        // Then 015's section: present, and then valid.
        assert_eq!(decide_alone(&with_content(json!({}))), missing);
        assert_eq!(decide_alone(&with_content(json!({ SECTION: {} }))), invalid);
        let mut section = fixture_limits(10).section();
        section["configurationInput"]["maxNodes"] = json!("0");
        assert_eq!(
            decide_alone(&with_content(json!({ SECTION: section }))),
            invalid
        );
    }

    #[test]
    fn the_verification_input_pins_resource_limit_policies_exactly() {
        // The boundary refuses no pin, a pin of another kind, and a repeated
        // pin (`crate::boundary::verification`). Several pins authenticate
        // each, in whatever order they come.
        let chosen = policy("storefront-limits", "1", 10);
        let other = policy("other-limits", "1", 20);
        assert_eq!(pinning(&[&chosen, &other]), pinning(&[&other, &chosen]));
        let both = pinning(&[&other, &chosen]);
        for value in [&chosen, &other] {
            let member = FormalMember::new(value.clone()).unwrap();
            assert!(both.authenticates(&member.envelope().unwrap()));
        }
        let unpinned = FormalMember::new(policy("storefront-limits", "2", 10)).unwrap();
        assert!(!both.authenticates(&unpinned.envelope().unwrap()));
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
                    json!({"authority": "trusted-local-host"}),
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
                decide(&reference, &candidates),
                Err(reference_issue(tiers[index].0)),
                "{:?}",
                tiers[index].0
            );
        }
        // Nothing related at all.
        let unrelated = envelope_of(ArtifactKind::GlossarySet, "glossary", "1", json!({}));
        assert_eq!(
            decide(&reference, &[unrelated]),
            Err(reference_issue(Reason::NotFound))
        );
        assert_eq!(
            decide(&reference, &[]),
            Err(reference_issue(Reason::NotFound))
        );
        // Several candidates in one tier still select that one reason.
        assert_eq!(
            decide(
                &reference,
                &[
                    policy("storefront-limits", "2", 10),
                    policy("storefront-limits", "3", 12),
                ]
            ),
            Err(reference_issue(Reason::RevisionMismatch))
        );
        // A digest mismatch is decided whatever the other candidate's body.
        assert_eq!(
            decide_with(
                &reference,
                &[tampered(&policy("storefront-limits", "1", 11))],
                &pinning(&[&policy("pinned-limits", "1", 1)])
            ),
            Err(reference_issue(Reason::DigestMismatch))
        );
    }

    /// What selection compared the reference with when it admits no
    /// candidate, as the submitted values: nothing for the reference's own
    /// checks, the collection when no member shares even its kind or identity,
    /// the closest tier's candidates, or every member of the selected group
    /// (D15:2250).
    fn compared(reference: &ExactTuple, values: &[Value]) -> (&'static str, Vec<Value>) {
        let members: Vec<FormalMember> = values
            .iter()
            .map(|value| FormalMember::new(value.clone()).unwrap())
            .collect();
        let values = |members: &[&FormalMember]| {
            members
                .iter()
                .map(|member| member.value().clone())
                .collect()
        };
        match select_candidate(reference, &members).unwrap_err().compared {
            Compared::Nothing => ("nothing", Vec::new()),
            Compared::Collection => ("collection", Vec::new()),
            Compared::Tier(candidates) => ("tier", values(&candidates)),
            Compared::Group(group) => ("group", values(&group)),
        }
    }

    #[test]
    fn a_rejected_selection_says_what_it_compared_the_reference_with() {
        let chosen = policy("storefront-limits", "1", 10);
        let reference = tuple_of(&chosen);
        // The reference's own checks compare it with nothing submitted.
        let trust = envelope_of(ArtifactKind::TrustPolicy, "trust", "1", json!({}));
        assert_eq!(
            compared(&tuple_of(&trust), std::slice::from_ref(&trust)),
            ("nothing", Vec::new())
        );
        // Without a member of any tier, the collection as a whole.
        let unrelated = envelope_of(ArtifactKind::GlossarySet, "glossary", "1", json!({}));
        let untupled = edited(&chosen, "/integrityDigest", None);
        for values in [vec![], vec![unrelated.clone(), untupled]] {
            assert_eq!(compared(&reference, &values), ("collection", Vec::new()));
        }
        // Every candidate of the closest tier, and none of a farther one.
        let revisions = [
            policy("storefront-limits", "2", 10),
            policy("storefront-limits", "3", 12),
        ];
        let farther = policy("other-limits", "1", 10);
        assert_eq!(
            compared(
                &reference,
                &[revisions[0].clone(), farther.clone(), revisions[1].clone()]
            ),
            ("tier", revisions.to_vec())
        );
        let digest = policy("storefront-limits", "1", 11);
        assert_eq!(
            compared(&reference, &[farther, digest.clone(), unrelated]),
            ("tier", vec![digest.clone()])
        );
        // Every member of the conflict group, the exact candidate included.
        let stale = tampered(&chosen);
        let conflicting = [chosen.clone(), digest, stale];
        assert_eq!(
            compared(&reference, &conflicting),
            ("group", conflicting.to_vec())
        );
        // Every member of the exact group, and no other member of the name.
        assert_eq!(
            compared(
                &reference,
                &[chosen.clone(), revisions[0].clone(), chosen.clone()]
            ),
            ("group", vec![chosen.clone(), chosen])
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
            assert_eq!(decide(&reference, std::slice::from_ref(&member)), not_found);
            // Nor does it stop the exact candidate from being admitted alone.
            assert_eq!(
                decide(&reference, &[member, chosen.clone()]),
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
            let expected = decide(&reference, &scenario);
            for ordering in permutations(&scenario) {
                assert_eq!(decide(&reference, &ordering), expected);
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
                decide(&tuple_of(&value), &[value]),
                Err(policy_issue(reason)),
                "{id}"
            );
        }
    }
}
