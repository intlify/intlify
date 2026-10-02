// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Assembling one invocation's units into an `authoring-inventory`.
//!
//! An invocation runs in two layers. [`crate::analyze_unit`] reads one unit
//! and nothing else, so a caller may read units in any order and on any
//! workers it owns. [`assemble_inventory`] then merges what they established
//! in one deterministic step: the result depends on which analyses it is
//! given, never on the order they arrive in or how they were scheduled. This
//! crate starts no thread in either layer.
//!
//! Assembly checks the analyses against the declared scope before it reads a
//! single fact. Each one has to be a member at the revision it read, read
//! under this context and this profile, and handed over once, and a complete
//! scope has to have every member read. A bound on the whole invocation is
//! applied here, where the whole invocation is first seen.
//!
//! The inventory is sealed whatever the outcome, because a blocked or partial
//! inventory is still a faithful record of what was read. What it may be used
//! for is a separate question, answered by type: only a checked result has a
//! checked inventory, and only a complete one is complete checked authoring
//! input.

use intlify_authoring::{
    AuthoringContext, AuthoringFailure, Completeness, Diagnostic, InventoryArtifact,
    InventoryBuilder, LimitKind, Outcome, UnitOutcome, UnitResult,
};

use crate::analysis::UnitAnalysis;
use crate::failure::ProducerFailure;
use crate::limits::{JsAuthoringLimits, JsLimitKind};
use crate::profile::JsAuthoringProfile;
use crate::unit::{check_invocation, is_member, sorted_members, UnitMember};

/// A checked inventory, and whether it covers its whole declared scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckedInventory<'r> {
    /// Every unit of a complete scope was checked. This is complete checked
    /// authoring input.
    Complete(&'r InventoryArtifact),
    /// Every unit of a partial scope was checked. These are checked facts
    /// about that part only. They never show that a declaration is absent
    /// from the project, so they support no retirement and no complete build.
    Partial(&'r InventoryArtifact),
}

impl<'r> CheckedInventory<'r> {
    /// Borrow the artifact, whichever scope it covers.
    #[must_use]
    pub const fn artifact(self) -> &'r InventoryArtifact {
        match self {
            Self::Complete(artifact) | Self::Partial(artifact) => artifact,
        }
    }

    /// Borrow the artifact only when it is complete checked input.
    #[must_use]
    pub const fn complete(self) -> Option<&'r InventoryArtifact> {
        match self {
            Self::Complete(artifact) => Some(artifact),
            Self::Partial(_) => None,
        }
    }
}

/// What assembling one invocation established.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssembledInventory {
    outcome: Outcome,
    artifact: InventoryArtifact,
    diagnostics: Box<[Diagnostic]>,
}

impl AssembledInventory {
    /// Return whether every unit was checked.
    #[must_use]
    pub const fn outcome(&self) -> Outcome {
        self.outcome
    }

    /// Return the inventory, when every unit was checked.
    ///
    /// A blocked result returns `None` rather than the inventory it did seal,
    /// so that inventory cannot be consumed as though the scope had been
    /// covered.
    #[must_use]
    pub fn checked_inventory(&self) -> Option<CheckedInventory<'_>> {
        if self.outcome != Outcome::Checked {
            return None;
        }
        Some(match self.artifact.body().completeness() {
            Completeness::Complete => CheckedInventory::Complete(&self.artifact),
            Completeness::Partial => CheckedInventory::Partial(&self.artifact),
        })
    }

    /// Borrow the inventory whatever the outcome.
    ///
    /// It records every unit and the facts each established, and supports
    /// inspection only. A blocked unit's facts are not complete authoring
    /// input.
    #[must_use]
    pub const fn inspection_inventory(&self) -> &InventoryArtifact {
        &self.artifact
    }

    /// Borrow every unit's diagnostics, in reporting order.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

/// Merge the analyses of one invocation's units into a sealed inventory.
///
/// `membership` and `completeness` are the scope the units were admitted for,
/// and `scope` names it in the inventory. The analyses may be given in any
/// order. Checks run before any fact is copied, in unit order, so the same
/// inputs report the same failure however they were supplied.
pub fn assemble_inventory(
    context: &dyn AuthoringContext,
    profile: &JsAuthoringProfile,
    scope: &str,
    completeness: Completeness,
    membership: &[UnitMember],
    analyses: &[UnitAnalysis],
    limits: &JsAuthoringLimits,
) -> Result<AssembledInventory, ProducerFailure> {
    check_invocation(context, profile)?;
    if membership.len() as u64 > limits.units || analyses.len() as u64 > limits.units {
        return Err(ProducerFailure::Limit(JsLimitKind::Units));
    }
    let members = sorted_members(membership)?;

    let mut read: Vec<&UnitAnalysis> = analyses.iter().collect();
    read.sort_unstable_by(|left, right| left.source().unit().cmp(right.source().unit()));
    if let Some(pair) = read
        .windows(2)
        .find(|pair| pair[0].source().unit() == pair[1].source().unit())
    {
        return Err(ProducerFailure::DuplicateUnit {
            unit: pair[0].source().unit().clone(),
        });
    }
    for analysis in &read {
        let source = analysis.source();
        let unit = || source.unit().clone();
        if source.owner() != context.owner() {
            return Err(ProducerFailure::ForeignOwner { unit: unit() });
        }
        if !analysis.read_under(context.basis(), profile) {
            return Err(ProducerFailure::ForeignAnalysis { unit: unit() });
        }
        if !is_member(&members, source) {
            return Err(ProducerFailure::NotAMember { unit: unit() });
        }
    }
    if completeness == Completeness::Complete {
        // Every analysis is a distinct member by now, so a complete scope is
        // missing a member exactly when some member has no analysis.
        if let Some(missing) = members.iter().find(|member| {
            read.binary_search_by(|analysis| analysis.source().unit().cmp(member.unit()))
                .is_err()
        }) {
            return Err(ProducerFailure::MissingMember {
                unit: missing.unit().clone(),
            });
        }
    }

    // Each unit stayed within these bounds on its own; the invocation is
    // bounded by them as a whole.
    let total = |count: fn(&UnitAnalysis) -> usize| -> u64 {
        read.iter().map(|analysis| count(analysis) as u64).sum()
    };
    if total(|analysis| analysis.inspection_facts().declarations().len())
        > limits.authoring.declarations
    {
        return Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
            LimitKind::Declarations,
        )));
    }
    if total(|analysis| analysis.diagnostics().len()) > limits.authoring.diagnostics {
        return Err(ProducerFailure::Authoring(AuthoringFailure::Limit(
            LimitKind::Diagnostics,
        )));
    }

    let mut builder = InventoryBuilder::new(
        context.owner().clone(),
        scope,
        context.basis().clone(),
        completeness,
    )
    .map_err(|_| ProducerFailure::InvalidScope)?;
    let mut diagnostics = Vec::new();
    for analysis in &read {
        builder.unit(UnitResult::new(
            analysis.source().clone(),
            analysis.outcome(),
        ));
        let facts = analysis.inspection_facts();
        builder.declarations(facts.declarations().iter().cloned());
        for reference in facts.references() {
            builder.reference(reference.clone());
        }
        for exclusion in facts.exclusions() {
            builder.exclusion(exclusion.clone());
        }
        diagnostics.extend(analysis.diagnostics().iter().cloned());
    }
    let inventory = builder.finish().map_err(ProducerFailure::Inventory)?;
    let artifact = InventoryArtifact::seal(inventory).map_err(ProducerFailure::Sealing)?;
    diagnostics.sort_by(Diagnostic::reporting_cmp);
    let outcome = if read
        .iter()
        .all(|analysis| analysis.outcome() == UnitOutcome::Checked)
    {
        Outcome::Checked
    } else {
        Outcome::Blocked
    };
    Ok(AssembledInventory {
        outcome,
        artifact,
        diagnostics: diagnostics.into_boxed_slice(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::tests::generous;
    use crate::test_support::{context, profile};

    fn empty(completeness: Completeness) -> AssembledInventory {
        assemble_inventory(
            &context(),
            &profile(),
            "empty",
            completeness,
            &[],
            &[],
            &generous(),
        )
        .expect("assembled")
    }

    #[test]
    fn an_empty_complete_scope_is_complete_checked_input() {
        // A scope that declares no unit is covered by reading none.
        let assembled = empty(Completeness::Complete);
        assert_eq!(assembled.outcome(), Outcome::Checked);
        assert!(assembled.diagnostics().is_empty());
        let artifact = assembled.inspection_inventory();
        assert!(artifact.body().units().is_empty());
        assert!(artifact.body().is_complete_checked());
        let checked = assembled.checked_inventory().expect("checked");
        assert_eq!(checked, CheckedInventory::Complete(artifact));
        assert_eq!(checked.artifact(), artifact);
        assert_eq!(checked.complete(), Some(artifact));
    }

    #[test]
    fn a_partial_checked_inventory_is_never_complete_input() {
        let assembled = empty(Completeness::Partial);
        let artifact = assembled.inspection_inventory();
        let checked = assembled.checked_inventory().expect("checked");
        assert_eq!(checked, CheckedInventory::Partial(artifact));
        assert_eq!(checked.artifact(), artifact);
        assert_eq!(checked.complete(), None);
        assert!(!artifact.body().is_complete_checked());
    }

    #[test]
    fn an_invocation_this_producer_cannot_run_is_refused_first() {
        // The same checks as admission, so analyses cannot be assembled under
        // a profile the context does not pin.
        let profile = profile().with_dom_globals([crate::profile::DomGlobal::Document]);
        let bare = intlify_authoring::test_context::TestContext::builder(
            crate::test_support::owner(),
            intlify_authoring::SurfaceVocabulary::new(["checkout"]).unwrap(),
        )
        .authoring_profile(profile.identity().clone())
        .build()
        .unwrap();
        assert_eq!(
            assemble_inventory(
                &bare,
                &profile,
                "empty",
                Completeness::Complete,
                &[],
                &[],
                &generous(),
            ),
            Err(ProducerFailure::UsageProfileMismatch)
        );
    }
}
