// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The diagnostics reconciliation and read-only compilation report.
//!
//! Missing evidence is `authoring-identity-update-required`: more evidence
//! or an explicit decision resolves it, and for compilation an identity
//! update does. Inputs that cannot both hold are
//! `authoring-identity-conflict`: only a different decision resolves them.
//! Both belong to the `identity-resolution` stage, and each names its cause
//! with a detail, so a reader can tell "no edit says where this went" from
//! "two edits say it went to the same place".

use intlify_authoring::{
    Detail, Diagnostic, DiagnosticOrigin, Occurrence, ReasonFamily, Severity, Stage,
};

use super::outcome::Conflict;
use crate::continuity::BasisGap;

/// The details reconciliation and compilation report.
///
/// Like the authoring crate's details, these are discriminators for this
/// workspace's tests and tools, not a public code registry.
pub mod detail {
    use intlify_authoring::Detail;

    /// No active entry holds the declaration and no verified edit carries one
    /// onto it, so compilation has no identity to give it.
    #[must_use]
    pub fn association_missing() -> Detail {
        Detail::literal("identity-association-missing")
    }

    /// Nothing shows the declaration is new.
    #[must_use]
    pub fn new_unproven() -> Detail {
        Detail::literal("identity-new-unproven")
    }

    /// Nothing shows where an old declaration went, or the account of it
    /// does not hold.
    #[must_use]
    pub fn continuity_missing() -> Detail {
        Detail::literal("identity-continuity-missing")
    }

    /// More than one account says where an old declaration went.
    #[must_use]
    pub fn continuity_ambiguous() -> Detail {
        Detail::literal("identity-continuity-ambiguous")
    }

    /// An edit touches or crosses an end of an old declaration.
    #[must_use]
    pub fn edit_at_boundary() -> Detail {
        Detail::literal("identity-edit-at-boundary")
    }

    /// The host's membership is not the inventory's units.
    #[must_use]
    pub fn membership_mismatch() -> Detail {
        Detail::literal("identity-membership-mismatch")
    }

    /// The inventory was resolved against other pins than the base's.
    #[must_use]
    pub fn basis_changed() -> Detail {
        Detail::literal("identity-basis-changed")
    }

    /// The update that produced the base was not supplied.
    #[must_use]
    pub fn basis_unknown() -> Detail {
        Detail::literal("identity-basis-unknown")
    }

    /// More than one identity claims one declaration.
    #[must_use]
    pub fn competing_claim() -> Detail {
        Detail::literal("identity-competing-claim")
    }

    /// An explicit allocation names an ID the base already holds.
    #[must_use]
    pub fn collision() -> Detail {
        Detail::literal("identity-collision")
    }

    /// An explicit continuation names a retired ID.
    #[must_use]
    pub fn retired_reuse() -> Detail {
        Detail::literal("identity-retired-reuse")
    }

    /// An explicit decision names another owner's ID.
    #[must_use]
    pub fn foreign_owner() -> Detail {
        Detail::literal("identity-foreign-owner")
    }

    /// An explicit decision does not fit this base or inventory.
    #[must_use]
    pub fn base_mismatch() -> Detail {
        Detail::literal("identity-base-mismatch")
    }
}

/// The detail that names a missing piece of evidence.
pub(super) fn gap_detail(gap: BasisGap) -> Detail {
    match gap {
        BasisGap::NewUnproven => detail::new_unproven(),
        BasisGap::Ambiguous => detail::continuity_ambiguous(),
        BasisGap::EditAtBoundary => detail::edit_at_boundary(),
        BasisGap::MembershipMismatch => detail::membership_mismatch(),
        BasisGap::BasisChanged => detail::basis_changed(),
        BasisGap::BasisUnknown => detail::basis_unknown(),
        BasisGap::UnsupportedProfile
        | BasisGap::EditMissing
        | BasisGap::EditDisagrees
        | BasisGap::ExtraEdits
        | BasisGap::SourceUnavailable
        | BasisGap::ReplayMismatch
        | BasisGap::DeclarationReplaced
        | BasisGap::MappedElsewhere
        | BasisGap::AbsenceUnproven
        | BasisGap::UnresolvedPlan => detail::continuity_missing(),
    }
}

/// The detail that names a conflict.
pub(super) fn conflict_detail(conflict: Conflict) -> Detail {
    match conflict {
        Conflict::CompetingClaim => detail::competing_claim(),
        Conflict::Collision => detail::collision(),
        Conflict::RetiredReuse => detail::retired_reuse(),
        Conflict::ForeignOwner => detail::foreign_owner(),
        Conflict::BaseMismatch => detail::base_mismatch(),
    }
}

/// Report a declaration or an old declaration whose identity the evidence
/// does not establish.
pub(super) fn update_required(
    location: Occurrence,
    gap: BasisGap,
    related: Vec<Occurrence>,
) -> Diagnostic {
    Diagnostic::new(
        Stage::IdentityResolution,
        DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityUpdateRequired),
        Severity::Error,
        location,
    )
    .with_detail(gap_detail(gap))
    .with_related(related)
}

/// Report a declaration compilation has no identity for.
///
/// The related occurrences are the old declarations that claim it without
/// carrying an identity onto it, such as the original of a copy.
pub(crate) fn association_missing(location: Occurrence, related: Vec<Occurrence>) -> Diagnostic {
    Diagnostic::new(
        Stage::IdentityResolution,
        DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityUpdateRequired),
        Severity::Error,
        location,
    )
    .with_detail(detail::association_missing())
    .with_related(related)
}

/// Report inputs that cannot both hold.
pub(super) fn conflict(
    location: Occurrence,
    conflict: Conflict,
    related: Vec<Occurrence>,
) -> Diagnostic {
    Diagnostic::new(
        Stage::IdentityResolution,
        DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityConflict),
        Severity::Error,
        location,
    )
    .with_detail(conflict_detail(conflict))
    .with_related(related)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::fixtures::Chain;

    #[test]
    fn missing_evidence_and_conflicts_are_told_apart() {
        let chain = Chain::load();
        let pay = chain.registry(1).snapshot().entries()[0]
            .declaration()
            .clone();
        let missing = update_required(pay.clone(), BasisGap::EditAtBoundary, vec![]);
        assert_eq!(missing.stage(), Stage::IdentityResolution);
        assert_eq!(
            missing.origin(),
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityUpdateRequired)
        );
        assert_eq!(missing.detail(), Some(detail::edit_at_boundary()));
        assert!(missing.is_blocking());
        assert_eq!(missing.occurrence(), Some(&pay));

        let competing = conflict(pay.clone(), Conflict::CompetingClaim, vec![pay.clone()]);
        assert_eq!(
            competing.origin(),
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityConflict)
        );
        assert_eq!(competing.detail(), Some(detail::competing_claim()));
        assert_eq!(competing.related(), std::slice::from_ref(&pay));

        // A declaration compilation has no identity for needs an update, not
        // a different decision.
        let unassigned = association_missing(pay.clone(), vec![pay.clone()]);
        assert_eq!(unassigned.stage(), Stage::IdentityResolution);
        assert_eq!(
            unassigned.origin(),
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringIdentityUpdateRequired)
        );
        assert_eq!(unassigned.detail(), Some(detail::association_missing()));
        assert!(unassigned.is_blocking());
        assert_eq!(unassigned.occurrence(), Some(&pay));
        assert_eq!(unassigned.related(), [pay]);
    }

    #[test]
    fn every_gap_and_conflict_has_its_detail() {
        let named = [
            (BasisGap::NewUnproven, "identity-new-unproven"),
            (BasisGap::Ambiguous, "identity-continuity-ambiguous"),
            (BasisGap::EditAtBoundary, "identity-edit-at-boundary"),
            (BasisGap::MembershipMismatch, "identity-membership-mismatch"),
            (BasisGap::BasisChanged, "identity-basis-changed"),
            (BasisGap::BasisUnknown, "identity-basis-unknown"),
            (BasisGap::EditMissing, "identity-continuity-missing"),
            (BasisGap::AbsenceUnproven, "identity-continuity-missing"),
            (BasisGap::ReplayMismatch, "identity-continuity-missing"),
        ];
        for (gap, spelling) in named {
            assert_eq!(gap_detail(gap).as_str(), spelling, "{gap:?}");
        }
        let conflicts = [
            (Conflict::CompetingClaim, "identity-competing-claim"),
            (Conflict::Collision, "identity-collision"),
            (Conflict::RetiredReuse, "identity-retired-reuse"),
            (Conflict::ForeignOwner, "identity-foreign-owner"),
            (Conflict::BaseMismatch, "identity-base-mismatch"),
        ];
        for (conflict, spelling) in conflicts {
            assert_eq!(conflict_detail(conflict).as_str(), spelling, "{conflict:?}");
        }
        assert_eq!(
            detail::association_missing().as_str(),
            "identity-association-missing"
        );
    }
}
