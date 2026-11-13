// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The owners of design 015's semantic dependency records (D15:2516,
//! 015-161). Each semantic group of the profile view has one singleton unit in
//! the phase of the last check that establishes the group. The unit ranks
//! after that check's kind and waits on the group's checks, so it is evaluated
//! exactly when the group is established. `profileSpecification`, which
//! `resolver-bootstrap-admission` establishes, is owned in
//! `structural-admission`, because a semantic dependency is a Shared record.
//! An owner finds nothing of its own, and nothing waits on it. Not the records
//! themselves: Resolution Evidence builds them, each under its owner. The
//! materialized and entry dependencies name the closed input slots 015 lists
//! (D15:2507), [`MaterializedSlot`] and [`EntrySlot`].

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::evaluation::{
    Admitted, Blocking, Check, Evaluation, EvaluationInvariant, Subject, Unit,
};
use super::resource_policy::PolicySlot;

/// One semantic group of the profile view, in the order of 015's semantic
/// inventory. It reads and writes as its semantic group ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub(crate) enum SemanticGroup {
    ProfileSpecification,
    ProjectIdentity,
    LocaleCanonicalization,
    ProjectLocales,
    LocaleNegotiation,
    MessageFallback,
    Coverage,
    RequiredPolicies,
    OptionalPolicies,
    Targets,
    DeploymentGroups,
    Delivery,
}

impl SemanticGroup {
    /// Every group, in the inventory's order.
    pub(crate) const ALL: [Self; 12] = [
        Self::ProfileSpecification,
        Self::ProjectIdentity,
        Self::LocaleCanonicalization,
        Self::ProjectLocales,
        Self::LocaleNegotiation,
        Self::MessageFallback,
        Self::Coverage,
        Self::RequiredPolicies,
        Self::OptionalPolicies,
        Self::Targets,
        Self::DeploymentGroups,
        Self::Delivery,
    ];

    /// The group's semantic group ID: its member name in the profile view.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::ProfileSpecification => "profileSpecification",
            Self::ProjectIdentity => "projectIdentity",
            Self::LocaleCanonicalization => "localeCanonicalization",
            Self::ProjectLocales => "projectLocales",
            Self::LocaleNegotiation => "localeNegotiation",
            Self::MessageFallback => "messageFallback",
            Self::Coverage => "coverage",
            Self::RequiredPolicies => "requiredPolicies",
            Self::OptionalPolicies => "optionalPolicies",
            Self::Targets => "targets",
            Self::DeploymentGroups => "deploymentGroups",
            Self::Delivery => "delivery",
        }
    }
}

/// A common or materialized input slot, which a materialized dependency
/// names. Each reads and writes as its slot ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MaterializedSlot {
    SpecificationInput,
    IntentSurfaceClassVocabulary,
    LocaleCanonicalizationData,
    ArtifactCollection,
    VerificationInput,
    ImplementationCapability,
    ConfigurationValue,
    ProfileSelector,
}

impl MaterializedSlot {
    /// Every slot, the six common ones first, in 015's order.
    pub(crate) const ALL: [Self; 8] = [
        Self::SpecificationInput,
        Self::IntentSurfaceClassVocabulary,
        Self::LocaleCanonicalizationData,
        Self::ArtifactCollection,
        Self::VerificationInput,
        Self::ImplementationCapability,
        Self::ConfigurationValue,
        Self::ProfileSelector,
    ];
}

/// An entry input slot, which an entry dependency names. Each reads and
/// writes as its slot ID.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum EntrySlot {
    EntrySource,
    CallSiteSource,
    ProfileSelector,
    Origins,
}

impl EntrySlot {
    /// Every slot, in 015's order.
    pub(crate) const ALL: [Self; 4] = [
        Self::EntrySource,
        Self::CallSiteSource,
        Self::ProfileSelector,
        Self::Origins,
    ];
}

/// The `policies` slots whose references the `requiredPolicies` group holds.
pub(crate) const REQUIRED_POLICIES: [PolicySlot; 5] = [
    PolicySlot::ResourceLimits,
    PolicySlot::Trust,
    PolicySlot::SourceAdmission,
    PolicySlot::Approval,
    PolicySlot::Selection,
];

/// The `policies` slots the `optionalPolicies` group holds, each an explicit
/// `null` or a reference.
pub(crate) const OPTIONAL_POLICIES: [PolicySlot; 2] =
    [PolicySlot::ProviderRouting, PolicySlot::GlossarySet];

/// The unit that owns `group`'s semantic dependency, in its kind's phase.
pub(crate) const fn unit(group: SemanticGroup) -> Unit {
    let subject = Subject::SemanticDependency(group);
    Unit {
        phase: subject.kind().phase(),
        subject,
        check: Check::EstablishedGroup,
    }
}

/// Records the owner of `group`'s semantic dependency, which waits on
/// `established`: the result of every check that establishes the group.
pub(crate) fn own(
    evaluation: &mut Evaluation,
    group: SemanticGroup,
    established: Admitted<()>,
) -> Result<(), EvaluationInvariant> {
    evaluation
        .check(unit(group), established, Ok::<(), Blocking>)
        .map(|_owned| ())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::resolver::evaluation::{Code, Phase, Status};
    use crate::resolver::subject::SubjectKind;
    use crate::resolver::targets::{self, TargetSubject};

    #[test]
    fn groups_and_slots_read_and_write_as_015_spells_them() {
        for group in SemanticGroup::ALL {
            let value = serde_json::to_value(group).unwrap();
            assert_eq!(value, group.id());
            assert_eq!(
                serde_json::from_value::<SemanticGroup>(value).unwrap(),
                group
            );
        }
        assert_eq!(
            serde_json::to_value(MaterializedSlot::ALL).unwrap(),
            json!([
                "specification-input",
                "intent-surface-class-vocabulary",
                "locale-canonicalization-data",
                "artifact-collection",
                "verification-input",
                "implementation-capability",
                "configuration-value",
                "profile-selector"
            ])
        );
        assert_eq!(
            serde_json::to_value(EntrySlot::ALL).unwrap(),
            json!([
                "entry-source",
                "call-site-source",
                "profile-selector",
                "origins"
            ])
        );
    }

    #[test]
    fn each_group_is_owned_in_the_phase_of_its_last_establishing_check() {
        let phases = SemanticGroup::ALL.map(|group| (group, unit(group).phase));
        assert_eq!(
            phases,
            [
                // Phase 0 establishes it, but a semantic dependency is Shared.
                (
                    SemanticGroup::ProfileSpecification,
                    Phase::StructuralAdmission
                ),
                (
                    SemanticGroup::ProjectIdentity,
                    Phase::ProjectIdentityResolution
                ),
                (
                    SemanticGroup::LocaleCanonicalization,
                    Phase::SpecificationAdmission
                ),
                (SemanticGroup::ProjectLocales, Phase::LocaleResolution),
                (
                    SemanticGroup::LocaleNegotiation,
                    Phase::LocalePolicyResolution
                ),
                (
                    SemanticGroup::MessageFallback,
                    Phase::LocalePolicyResolution
                ),
                (SemanticGroup::Coverage, Phase::LocalePolicyResolution),
                (
                    SemanticGroup::RequiredPolicies,
                    Phase::ArtifactReferenceResolution
                ),
                (
                    SemanticGroup::OptionalPolicies,
                    Phase::ArtifactReferenceResolution
                ),
                (SemanticGroup::Targets, Phase::TargetResolution),
                (SemanticGroup::DeploymentGroups, Phase::HydrationValidation),
                (SemanticGroup::Delivery, Phase::DeliveryResolution),
            ]
        );
    }

    /// An owner ranks after every kind of its phase, so it follows the kinds
    /// of the checks that establish its group; one phase's owners keep the
    /// inventory's order.
    #[test]
    fn the_owners_come_last_in_their_phase_in_inventory_order() {
        for phase in Phase::ALL {
            let kinds: Vec<SubjectKind> = SubjectKind::ALL
                .into_iter()
                .filter(|kind| kind.phase() == phase)
                .collect();
            let owners: Vec<SubjectKind> = SemanticGroup::ALL
                .into_iter()
                .map(|group| unit(group).subject.kind())
                .filter(|kind| kind.phase() == phase)
                .collect();
            assert!(kinds.ends_with(&owners), "{phase:?}");
        }
    }

    #[test]
    fn the_ids_are_the_profile_views_members_in_inventory_order() {
        assert!(SemanticGroup::ALL.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            SemanticGroup::ALL.map(SemanticGroup::id),
            [
                "profileSpecification",
                "projectIdentity",
                "localeCanonicalization",
                "projectLocales",
                "localeNegotiation",
                "messageFallback",
                "coverage",
                "requiredPolicies",
                "optionalPolicies",
                "targets",
                "deploymentGroups",
                "delivery",
            ]
        );
    }

    #[test]
    fn the_two_policy_groups_split_the_policy_slots() {
        let mut slots: Vec<PolicySlot> = REQUIRED_POLICIES
            .into_iter()
            .chain(OPTIONAL_POLICIES)
            .collect();
        slots.sort();
        assert_eq!(
            slots,
            [
                PolicySlot::Approval,
                PolicySlot::GlossarySet,
                PolicySlot::ProviderRouting,
                PolicySlot::ResourceLimits,
                PolicySlot::Selection,
                PolicySlot::SourceAdmission,
                PolicySlot::Trust,
            ]
        );
    }

    #[test]
    fn an_owner_is_evaluated_exactly_when_its_group_is_established() {
        let mut evaluation = Evaluation::default();
        // A target whose locales are not the project's leaves the targets
        // group unestablished.
        let id = serde_json::from_value(json!("browser")).unwrap();
        let subset = targets::unit(TargetSubject::Target(id), Check::ProjectSubset);
        let failed = evaluation
            .check(subset, Ok(()), |()| {
                Err::<(), _>(Blocking {
                    code: Code::TargetLocalesNotSubset,
                    reason: None,
                    details: None,
                })
            })
            .unwrap();
        let causes = failed.clone().unwrap_err();
        own(&mut evaluation, SemanticGroup::Targets, failed).unwrap();
        own(&mut evaluation, SemanticGroup::Delivery, Ok(())).unwrap();
        assert_eq!(
            evaluation.status(&unit(SemanticGroup::Targets)),
            Some(&Status::NotEvaluated(causes))
        );
        assert_eq!(
            evaluation.status(&unit(SemanticGroup::Delivery)),
            Some(&Status::Evaluated)
        );
        // An owner reports nothing of its own.
        assert_eq!(evaluation.issues().count(), 1);
        assert_eq!(evaluation.notices().count(), 0);
        // And owns its group once.
        assert_eq!(
            own(&mut evaluation, SemanticGroup::Delivery, Ok(())),
            Err(EvaluationInvariant::UnitRecordedTwice(unit(
                SemanticGroup::Delivery
            )))
        );
    }
}
