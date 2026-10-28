// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's ordinary exact resolution, which ends Phase 9
//! (`artifact-reference-resolution`). Only once the complete collection forms
//! the Profile Resolution Artifact Set is each required and present optional
//! reference of the selected declaration resolved against it. A reference is
//! first checked for its role's kind and then for a specification revision the
//! resolver supports; it then names the Set's artifact with its exact tuple, or
//! takes the closest mismatch in the bootstrap's vocabulary and order. The Set
//! holds one artifact for each tuple, so neither several matches nor
//! conflicting content can occur here.
//!
//! When every present reference names its artifact, the resolution also makes
//! the Resolved Profile Artifact Bindings, kept as Phase 3's internal sidecar
//! (plan 022 #13). 015 derives them in profile projection (Phase 14) from the
//! checked profile's references; until that phase exists they come from this
//! resolution, and nothing projects them to a consumer.

use std::collections::BTreeMap;

use crate::artifacts::envelope::{ArtifactKind, ExactTuple};
use crate::references::{PolicyReference, TargetProfileReference};

use super::artifact_resolution::{AdmittedArtifact, ArtifactSet};
use super::bootstrap::{mismatch, reference_checks};
use super::evaluation::{
    and, Admitted, Blocking, Causes, Check, Code, Evaluation, EvaluationInvariant, Phase, Subject,
    Unit,
};
use super::resource_policy::{DeclarationReferences, ReferenceField};

/// The unit Phase 9 records for the reference at `field`.
pub(crate) const fn unit(field: ReferenceField) -> Unit {
    Unit {
        phase: Phase::ArtifactReferenceResolution,
        subject: Subject::ArtifactReference(field),
        check: Check::ResolvedReference,
    }
}

/// The Set's artifact each present reference of the selected declaration
/// names, and the Set's tuple for it, by the reference's field. An explicit
/// `null` has no field here, and references repeating one exact tuple name the
/// same artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedReferences<'s>(
    BTreeMap<ReferenceField, (&'s ExactTuple, &'s AdmittedArtifact<'s>)>,
);

impl<'s> ResolvedReferences<'s> {
    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = (&ReferenceField, &'s ExactTuple, &'s AdmittedArtifact<'s>)> {
        self.0
            .iter()
            .map(|(field, (tuple, artifact))| (field, *tuple, *artifact))
    }
}

/// The Resolved Profile Artifact Bindings: each distinct exact reference
/// bound to the one artifact of the Set with that tuple, in the tuples' order.
/// Repeated use of one reference makes one binding, and an explicit `null`
/// makes none. A submitted artifact no reference names is never bound,
/// although the collection admission checked it, and the Locale
/// Canonicalization Data Artifact is no member of the Set. A body is no part of
/// the profile's semantics: the exact reference already identifies it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ArtifactBindings<'s>(BTreeMap<&'s ExactTuple, &'s AdmittedArtifact<'s>>);

impl<'s> ArtifactBindings<'s> {
    /// The bindings of a resolution in which every present reference named its
    /// artifact. Each key is the tuple the Set holds the artifact under, which
    /// is the reference's own.
    pub(crate) fn bind(references: &ResolvedReferences<'s>) -> Self {
        Self(
            references
                .iter()
                .map(|(_, tuple, artifact)| (tuple, artifact))
                .collect(),
        )
    }

    pub(crate) fn iter(
        &self,
    ) -> impl Iterator<Item = (&'s ExactTuple, &'s AdmittedArtifact<'s>)> + '_ {
        self.0.iter().map(|(tuple, artifact)| (*tuple, *artifact))
    }
}

/// Phase 9's ordinary exact resolution. Each present reference is one unit
/// under the Set. A field structural admission did not admit has no
/// references to resolve, and its causes leave the resolution unavailable.
pub(crate) fn resolve_references<'s>(
    evaluation: &mut Evaluation,
    set: Result<&'s ArtifactSet<'s>, &Causes>,
    references: &DeclarationReferences<'_>,
) -> Result<Admitted<ResolvedReferences<'s>>, EvaluationInvariant> {
    let (present, unavailable) =
        references.visible(PolicyReference::tuple, TargetProfileReference::tuple);
    let prerequisite = set.map_err(Causes::clone);
    let mut holds: Admitted<()> = unavailable.map_or(Ok(()), Err);
    let mut resolved = BTreeMap::new();
    for (field, reference) in present {
        let role = field.role_kind();
        let found = evaluation.check(unit(field.clone()), prerequisite.clone(), |set| {
            resolve(set, &reference, role)
        })?;
        match found {
            Ok(found) => {
                resolved.insert(field, found);
            }
            Err(causes) => holds = and(holds, Err(causes)),
        }
    }
    Ok(holds.map(|()| ResolvedReferences(resolved)))
}

/// One reference's decision: its own two checks, then the Set's artifact with
/// its exact tuple, or else the closest tier some artifact of the Set
/// occupies.
fn resolve<'s>(
    set: &'s ArtifactSet<'s>,
    reference: &ExactTuple,
    role: ArtifactKind,
) -> Result<(&'s ExactTuple, &'s AdmittedArtifact<'s>), Blocking> {
    let not_admitted = |reason| Blocking {
        code: Code::ArtifactReferenceNotAdmitted,
        reason: Some(reason),
        details: None,
    };
    reference_checks(reference, role).map_err(not_admitted)?;
    set.get(reference).ok_or_else(|| {
        let tuples: Vec<&ExactTuple> = set.iter().map(|(tuple, _)| tuple).collect();
        not_admitted(mismatch(reference, &tuples))
    })
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Map, Value};

    use super::*;
    use crate::artifacts::collection::{read_collection, CollectionLimits};
    use crate::artifacts::content::sample;
    use crate::artifacts::envelope::{envelope_of, FormalMember};
    use crate::model::{NonEmptyMap, Policies, TargetDeclaration, TargetId};
    use crate::resolver::artifact_resolution::admit_collection;
    use crate::resolver::evaluation::{IssueKey, Reason, Status};
    use crate::resolver::limits::{fixture_limits, ConfirmedPolicy};
    use crate::resolver::locales::fixtures::stand_in;
    use crate::resolver::resource_policy::PolicySlot::{
        self, Approval, GlossarySet, ProviderRouting, ResourceLimits, Selection, SourceAdmission,
        Trust,
    };
    use crate::resolver::specification::Slot;

    const ROOM: CollectionLimits = CollectionLimits {
        members: u64::MAX,
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    /// A sealed envelope of `kind` with an admitted content.
    fn valid(kind: ArtifactKind, identity: &str) -> Value {
        envelope_of(kind, identity, "1", sample(kind))
    }

    fn tuple_of(value: &Value) -> ExactTuple {
        FormalMember::new(value.clone())
            .unwrap()
            .envelope()
            .unwrap()
            .tuple()
            .clone()
    }

    /// The reference a declaration writes for `tuple`.
    fn reference_value(tuple: &ExactTuple) -> Value {
        json!({
            "kind": tuple.kind.spelling(),
            "identity": tuple.identity.as_str(),
            "revision": tuple.revision.as_str(),
            "specificationRevision": tuple.specification_revision.as_str(),
            "semanticDigest": tuple.semantic_digest.as_str(),
        })
    }

    const fn policy(slot: PolicySlot) -> ReferenceField {
        ReferenceField::Policy(slot)
    }

    fn target(id: &str) -> ReferenceField {
        ReferenceField::TargetProfile(serde_json::from_value(json!(id)).unwrap())
    }

    /// The artifact each field of the default declaration names: every slot,
    /// and the targets `app` and `web`.
    fn artifacts() -> Vec<(ReferenceField, Value)> {
        vec![
            (
                policy(Approval),
                valid(ArtifactKind::ApprovalPolicy, "storefront-approval"),
            ),
            (
                policy(GlossarySet),
                valid(ArtifactKind::GlossarySet, "storefront-glossary"),
            ),
            (
                policy(ProviderRouting),
                valid(ArtifactKind::ProviderRoutingPolicy, "storefront-routing"),
            ),
            (
                policy(ResourceLimits),
                valid(ArtifactKind::ResourceLimitPolicy, "storefront-limits"),
            ),
            (
                policy(Selection),
                valid(ArtifactKind::SelectionPolicy, "storefront-selection"),
            ),
            (
                policy(SourceAdmission),
                valid(ArtifactKind::SourceAdmissionPolicy, "storefront-source"),
            ),
            (
                policy(Trust),
                valid(ArtifactKind::TrustPolicy, "storefront-trust"),
            ),
            (
                target("app"),
                valid(ArtifactKind::TargetProfile, "storefront-app"),
            ),
            (
                target("web"),
                valid(ArtifactKind::TargetProfile, "storefront-web"),
            ),
        ]
    }

    /// What Phase 9 reads, which a test edits: the submitted members, the
    /// reference each field writes (`null` for an explicit absence), and
    /// whether structural admission admitted `policies` and `targets`.
    struct Inputs {
        members: Vec<Value>,
        references: Vec<(ReferenceField, Value)>,
        policies: Admitted<()>,
        targets: Admitted<()>,
    }

    impl Inputs {
        /// Every field naming its artifact.
        fn new() -> Self {
            let artifacts = artifacts();
            Self {
                references: artifacts
                    .iter()
                    .map(|(field, value)| (field.clone(), reference_value(&tuple_of(value))))
                    .collect(),
                members: artifacts.into_iter().map(|(_, value)| value).collect(),
                policies: Ok(()),
                targets: Ok(()),
            }
        }

        fn reference(&mut self, field: &ReferenceField) -> &mut Value {
            &mut self
                .references
                .iter_mut()
                .find(|(written, _)| written == field)
                .unwrap()
                .1
        }

        /// Phase 9 over the inputs: the collection's admission, then the exact
        /// resolution, whose result `keep` copies out. Returns the evaluation
        /// and the copy.
        fn with<T>(
            &self,
            keep: impl FnOnce(Admitted<ResolvedReferences<'_>>) -> Admitted<T>,
        ) -> (Evaluation, Admitted<T>) {
            let mut policies = Map::new();
            let mut targets = Map::new();
            for (field, reference) in &self.references {
                match field {
                    ReferenceField::Policy(slot) => {
                        policies.insert(slot.name().into(), reference.clone());
                    }
                    ReferenceField::TargetProfile(id) => {
                        targets.insert(
                            id.as_str().into(),
                            json!({"profile": reference, "requestedLocales": ["en"]}),
                        );
                    }
                }
            }
            let policies: Policies<PolicyReference> =
                serde_json::from_value(Value::Object(policies)).unwrap();
            let targets: NonEmptyMap<TargetId, TargetDeclaration<TargetProfileReference>> =
                serde_json::from_value(Value::Object(targets)).unwrap();
            let texts: Vec<String> = self.members.iter().map(Value::to_string).collect();
            let bytes: Vec<&[u8]> = texts.iter().map(String::as_bytes).collect();
            let collection = read_collection(Some(&bytes), &ROOM).unwrap();
            let mut evaluation = Evaluation::default();
            let confirmed = ConfirmedPolicy::fixture_owned(fixture_limits(1));
            let set = admit_collection(&mut evaluation, Ok(&confirmed), &collection).unwrap();
            let references = DeclarationReferences {
                policies: self.policies.clone().map(|()| &policies),
                target_profiles: self.targets.clone().map(|()| &targets),
            };
            let resolved = resolve_references(&mut evaluation, set.as_ref(), &references).unwrap();
            (evaluation, keep(resolved))
        }

        /// The tuple of the artifact each resolved field names, read from the
        /// artifact's own member, which the Set holds under that tuple.
        fn run(&self) -> (Evaluation, Admitted<Vec<(ReferenceField, ExactTuple)>>) {
            self.with(|resolved| {
                resolved.map(|resolved| {
                    resolved
                        .iter()
                        .map(|(field, held, artifact)| {
                            let tuple = artifact.member.envelope().unwrap().tuple().clone();
                            assert_eq!(held, &tuple);
                            (field.clone(), tuple)
                        })
                        .collect()
                })
            })
        }

        /// Each binding's tuple and the member of its artifact.
        fn bind(&self) -> Admitted<Vec<(ExactTuple, Value)>> {
            self.with(|resolved| {
                resolved.map(|resolved| {
                    ArtifactBindings::bind(&resolved)
                        .iter()
                        .map(|(tuple, artifact)| (tuple.clone(), artifact.member.value().clone()))
                        .collect()
                })
            })
            .1
        }
    }

    /// The field of a reference unit, which only Phase 9's one check has.
    fn field_of(recorded: &Unit) -> Option<ReferenceField> {
        let Subject::ArtifactReference(field) = recorded.subject.clone() else {
            return None;
        };
        assert_eq!(recorded.phase, Phase::ArtifactReferenceResolution);
        assert_eq!(recorded.check, Check::ResolvedReference);
        Some(field)
    }

    /// The reference units' issues, each with its reason.
    fn issues(evaluation: &Evaluation) -> Vec<(ReferenceField, Reason)> {
        evaluation
            .issues()
            .filter_map(|key: &IssueKey| {
                let field = field_of(key.unit())?;
                assert_eq!(key.code(), Code::ArtifactReferenceNotAdmitted);
                Some((field, key.reason().unwrap()))
            })
            .collect()
    }

    /// The reference units' statuses, in 015's order.
    fn statuses(evaluation: &Evaluation) -> Vec<(ReferenceField, Status)> {
        evaluation
            .statuses()
            .filter_map(|(recorded, status)| Some((field_of(recorded)?, status.clone())))
            .collect()
    }

    #[test]
    fn every_present_reference_names_its_artifact_in_field_order() {
        let (evaluation, resolved) = Inputs::new().run();
        assert!(evaluation.issues().next().is_none());
        // `artifacts` lists the fields in their logical order: the slots by
        // name, then the targets by ID.
        let expected: Vec<(ReferenceField, ExactTuple)> = artifacts()
            .iter()
            .map(|(field, value)| (field.clone(), tuple_of(value)))
            .collect();
        assert_eq!(resolved, Ok(expected.clone()));
        assert_eq!(
            statuses(&evaluation),
            expected
                .into_iter()
                .map(|(field, _)| (field, Status::Evaluated))
                .collect::<Vec<_>>()
        );
        // Within the phase, every collection unit comes before the references.
        let references: Vec<bool> = evaluation
            .statuses()
            .map(|(unit, _)| {
                assert_eq!(unit.phase, Phase::ArtifactReferenceResolution);
                matches!(unit.subject, Subject::ArtifactReference(_))
            })
            .collect();
        assert!(references.is_sorted());
        // Two units for each of the nine members, and one for each group.
        assert_eq!(
            references.iter().filter(|reference| !**reference).count(),
            27
        );
    }

    #[test]
    fn an_explicit_null_has_no_unit_and_names_nothing() {
        let mut inputs = Inputs::new();
        *inputs.reference(&policy(ProviderRouting)) = Value::Null;
        *inputs.reference(&policy(GlossarySet)) = Value::Null;
        let (evaluation, resolved) = inputs.run();
        assert!(evaluation.issues().next().is_none());
        let fields: Vec<ReferenceField> = statuses(&evaluation)
            .into_iter()
            .map(|(field, _)| field)
            .collect();
        assert!(!fields.contains(&policy(ProviderRouting)));
        assert!(!fields.contains(&policy(GlossarySet)));
        assert_eq!(fields.len(), 7);
        let resolved = resolved.unwrap();
        assert_eq!(resolved.len(), 7);
        assert!(resolved
            .iter()
            .all(|(field, _)| *field != policy(ProviderRouting) && *field != policy(GlossarySet)));
    }

    /// One change to the inputs.
    type Edit = fn(&mut Inputs);

    #[test]
    fn a_failed_reference_takes_the_first_decision_that_applies() {
        let cases: [(&str, Edit, Reason); 8] = [
            // The two checks of the reference itself come first, the role's
            // kind before the specification revision.
            (
                "another role's kind",
                |inputs| {
                    let approval = inputs.reference(&policy(Approval)).clone();
                    *inputs.reference(&policy(Trust)) = approval;
                },
                Reason::RoleKindMismatch,
            ),
            (
                "an unsupported specification revision",
                |inputs| {
                    inputs.reference(&policy(Trust))["specificationRevision"] = json!("1");
                },
                Reason::SpecificationRevisionUnsupported,
            ),
            (
                "both",
                |inputs| {
                    let mut approval = inputs.reference(&policy(Approval)).clone();
                    approval["specificationRevision"] = json!("1");
                    *inputs.reference(&policy(Trust)) = approval;
                },
                Reason::RoleKindMismatch,
            ),
            // Then the closest tier: everything but the digest, the revision,
            // the identity under another kind, and the kind.
            (
                "another digest",
                |inputs| {
                    inputs.reference(&policy(Trust))["semanticDigest"] =
                        json!(format!("sha256:{}", "1".repeat(64)));
                },
                Reason::DigestMismatch,
            ),
            (
                "another revision",
                |inputs| {
                    inputs.reference(&policy(Trust))["revision"] = json!("2");
                    // A candidate in a farther tier does not change it.
                    inputs
                        .members
                        .push(valid(ArtifactKind::ApprovalPolicy, "storefront-trust"));
                },
                Reason::RevisionMismatch,
            ),
            (
                "the identity of another kind",
                |inputs| {
                    inputs.reference(&policy(Trust))["identity"] = json!("storefront-source");
                },
                Reason::KindMismatch,
            ),
            (
                "another identity",
                |inputs| {
                    inputs.reference(&policy(Trust))["identity"] = json!("storefront-vault");
                },
                Reason::IdentityMismatch,
            ),
            (
                "nothing related",
                |inputs| {
                    let trust = tuple_of(&valid(ArtifactKind::TrustPolicy, "storefront-trust"));
                    inputs.members.retain(|member| tuple_of(member) != trust);
                    assert_eq!(inputs.members.len(), 8);
                },
                Reason::NotFound,
            ),
        ];
        for (case, edit, reason) in cases {
            let mut inputs = Inputs::new();
            edit(&mut inputs);
            let (evaluation, resolved) = inputs.run();
            assert_eq!(issues(&evaluation), [(policy(Trust), reason)], "{case}");
            // The collection itself was admitted.
            assert_eq!(evaluation.issues().count(), 1, "{case}");
            let Err(causes) = resolved else {
                panic!("{case}: resolved");
            };
            assert_eq!(
                causes
                    .keys()
                    .map(|key| key.unit().clone())
                    .collect::<Vec<_>>(),
                [unit(policy(Trust))],
                "{case}"
            );
            // Every reference was evaluated, and only the trust reference
            // failed.
            assert_eq!(statuses(&evaluation).len(), 9, "{case}");
            assert!(
                statuses(&evaluation)
                    .iter()
                    .all(|(_, status)| *status == Status::Evaluated),
                "{case}"
            );
        }
    }

    #[test]
    fn a_target_profile_reference_is_resolved_like_a_policy() {
        let mut inputs = Inputs::new();
        inputs.reference(&target("web"))["revision"] = json!("2");
        let (evaluation, _) = inputs.run();
        assert_eq!(
            issues(&evaluation),
            [(target("web"), Reason::RevisionMismatch)]
        );
        // Two targets may name one Target Profile, which is one artifact.
        let mut inputs = Inputs::new();
        let web = inputs.reference(&target("web")).clone();
        *inputs.reference(&target("app")) = web;
        let (evaluation, resolved) = inputs.run();
        assert!(evaluation.issues().next().is_none());
        let resolved = resolved.unwrap();
        let named = |field: &ReferenceField| {
            resolved
                .iter()
                .find(|(resolved, _)| resolved == field)
                .unwrap()
                .1
                .clone()
        };
        assert_eq!(named(&target("app")), named(&target("web")));
    }

    #[test]
    fn without_the_set_no_reference_is_evaluated() {
        let mut inputs = Inputs::new();
        // A duplicate member leaves the collection without a Set.
        let trust = valid(ArtifactKind::TrustPolicy, "storefront-trust");
        inputs.members.push(trust);
        let (evaluation, resolved) = inputs.run();
        let collection_causes: Vec<IssueKey> = evaluation
            .issues()
            .filter(|key| matches!(key.unit().subject, Subject::ArtifactCollection(_)))
            .cloned()
            .collect();
        assert_eq!(collection_causes.len(), 1);
        assert!(issues(&evaluation).is_empty());
        let statuses = statuses(&evaluation);
        assert_eq!(statuses.len(), 9);
        for (field, status) in statuses {
            let Status::NotEvaluated(causes) = status else {
                panic!("{field:?} was evaluated");
            };
            assert_eq!(
                causes.keys().cloned().collect::<Vec<_>>(),
                collection_causes
            );
        }
        let Err(causes) = resolved else {
            panic!("resolved without a Set");
        };
        assert_eq!(
            causes.keys().cloned().collect::<Vec<_>>(),
            collection_causes
        );
    }

    #[test]
    fn a_field_structural_admission_did_not_admit_has_no_references() {
        let behind = stand_in(Slot::ProjectProfile);
        let mut inputs = Inputs::new();
        inputs.policies = Err(behind.clone());
        let (evaluation, resolved) = inputs.run();
        // Only the targets' references are resolved, and they hold.
        assert_eq!(
            statuses(&evaluation),
            [
                (target("app"), Status::Evaluated),
                (target("web"), Status::Evaluated)
            ]
        );
        assert_eq!(resolved, Err(behind.clone()));
        let mut inputs = Inputs::new();
        inputs.targets = Err(behind.clone());
        let (evaluation, resolved) = inputs.run();
        assert_eq!(statuses(&evaluation).len(), 7);
        assert!(evaluation
            .issues()
            .all(|key| !matches!(key.unit().subject, Subject::ArtifactReference(_))));
        assert_eq!(resolved, Err(behind));
    }

    #[test]
    fn each_distinct_reference_is_bound_once_to_its_artifact() {
        // Every field names its own artifact: nine bindings, in the tuples'
        // order.
        let mut expected: Vec<(ExactTuple, Value)> = artifacts()
            .into_iter()
            .map(|(_, value)| (tuple_of(&value), value))
            .collect();
        expected.sort_by(|left, right| left.0.cmp(&right.0));
        assert_eq!(Inputs::new().bind(), Ok(expected));
        // Two targets naming one Target Profile make one binding. The other
        // Target Profile stays submitted, and no reference names it.
        let mut inputs = Inputs::new();
        let web = inputs.reference(&target("web")).clone();
        *inputs.reference(&target("app")) = web;
        let bound = inputs.bind().unwrap();
        assert_eq!(bound.len(), 8);
        let app = tuple_of(&valid(ArtifactKind::TargetProfile, "storefront-app"));
        let web = tuple_of(&valid(ArtifactKind::TargetProfile, "storefront-web"));
        assert!(bound.iter().all(|(tuple, _)| *tuple != app));
        assert_eq!(bound.iter().filter(|(tuple, _)| *tuple == web).count(), 1);
    }

    #[test]
    fn an_explicit_null_and_an_unreferenced_artifact_make_no_binding() {
        let mut inputs = Inputs::new();
        *inputs.reference(&policy(ProviderRouting)) = Value::Null;
        // The routing policy stays submitted, and an approval policy no
        // reference names is added.
        inputs
            .members
            .push(valid(ArtifactKind::ApprovalPolicy, "storefront-unused"));
        let bound = inputs.bind().unwrap();
        assert_eq!(bound.len(), 8);
        let kinds: Vec<ArtifactKind> = bound.iter().map(|(tuple, _)| tuple.kind).collect();
        assert!(!kinds.contains(&ArtifactKind::ProviderRoutingPolicy));
        assert_eq!(
            kinds
                .iter()
                .filter(|kind| **kind == ArtifactKind::ApprovalPolicy)
                .count(),
            1
        );
    }

    #[test]
    fn only_a_complete_resolution_makes_bindings() {
        let mut inputs = Inputs::new();
        inputs.reference(&policy(Trust))["revision"] = json!("2");
        let Err(causes) = inputs.bind() else {
            panic!("a failed reference was bound");
        };
        assert_eq!(
            causes
                .keys()
                .map(|key| key.unit().clone())
                .collect::<Vec<_>>(),
            [unit(policy(Trust))]
        );
        // Without the Set, no reference is evaluated.
        let mut inputs = Inputs::new();
        inputs
            .members
            .push(valid(ArtifactKind::TrustPolicy, "storefront-trust"));
        let Err(causes) = inputs.bind() else {
            panic!("bound without a Set");
        };
        assert!(causes
            .keys()
            .all(|key| matches!(key.unit().subject, Subject::ArtifactCollection(_))));
    }
}
