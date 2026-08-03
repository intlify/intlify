// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable owned result of one complete semantic link.
//!
//! Outcome construction is private to the linker so blocking findings cannot
//! coexist with plans and callers cannot reorder or replace admitted results.

use std::fmt;

use crate::model::{TypedKeyModelBatch, TypedKeyModelSnapshotRelation};
use crate::{LinkFinding, LinkOperationalError, MessageBundlePlan, TypedKeyModel};

/// Fully owned deterministic semantic result.
#[derive(PartialEq, Eq)]
pub struct LinkOutcome {
    findings: Box<[LinkFinding]>,
    bundle_plans: Option<Box<[MessageBundlePlan]>>,
    typed_key_models: Box<[TypedKeyModel]>,
    #[allow(dead_code)] // Retained privately for the future checked export handoff.
    typed_key_model_snapshots: Box<[TypedKeyModelSnapshotRelation]>,
}

impl fmt::Debug for LinkOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LinkOutcome")
            .field("findings", &self.findings)
            .field("bundle_plans", &self.bundle_plans)
            .field("typed_key_models", &self.typed_key_models)
            .finish_non_exhaustive()
    }
}

impl LinkOutcome {
    pub(crate) fn try_new(
        findings: Vec<LinkFinding>,
        bundle_plans: Option<Vec<MessageBundlePlan>>,
        typed_key_models: TypedKeyModelBatch,
    ) -> Result<Self, LinkOperationalError> {
        if !findings.is_sorted() {
            return Err(LinkOperationalError::InternalInvariant);
        }
        let has_blocking_finding = findings.iter().any(LinkFinding::blocking);
        if bundle_plans.is_none() != has_blocking_finding {
            return Err(LinkOperationalError::InternalInvariant);
        }
        let (typed_key_models, typed_key_model_snapshots) = typed_key_models.into_parts();
        Ok(Self {
            findings: findings.into_boxed_slice(),
            bundle_plans: bundle_plans.map(Vec::into_boxed_slice),
            typed_key_models,
            typed_key_model_snapshots,
        })
    }

    /// Return the complete canonical finding set.
    #[must_use]
    pub const fn findings(&self) -> &[LinkFinding] {
        &self.findings
    }

    /// Return all plans, preserving blocked, empty, and non-empty states.
    #[must_use]
    pub fn bundle_plans(&self) -> Option<&[MessageBundlePlan]> {
        self.bundle_plans.as_deref()
    }

    /// Return canonical language-neutral typed-key models.
    #[must_use]
    pub const fn typed_key_models(&self) -> &[TypedKeyModel] {
        &self.typed_key_models
    }

    /// Return whether semantic findings withheld every bundle plan.
    #[must_use]
    pub const fn generation_blocked(&self) -> bool {
        self.bundle_plans.is_none()
    }

    /// Lend the checked plan and typed-key baseline relation to export preparation.
    #[cfg(feature = "export-preparation")]
    #[doc(hidden)]
    #[must_use]
    pub fn export_preparation_view(
        &self,
    ) -> Option<crate::export_preparation_handoff::ExportPreparationView<'_>> {
        let plans = self.bundle_plans.as_deref()?;
        Some(
            crate::export_preparation_handoff::ExportPreparationView::new(
                plans,
                &self.typed_key_models,
                &self.typed_key_model_snapshots,
            ),
        )
    }
}

#[cfg(all(test, feature = "export-preparation"))]
mod export_preparation_tests {
    use super::LinkOutcome;

    fn outcome_with_plans(bundle_plans: Option<Box<[crate::MessageBundlePlan]>>) -> LinkOutcome {
        LinkOutcome {
            findings: Box::new([]),
            bundle_plans,
            typed_key_models: Box::new([]),
            typed_key_model_snapshots: Box::new([]),
        }
    }

    #[test]
    fn export_view_preserves_blocked_and_empty_plan_states() {
        assert!(outcome_with_plans(None).export_preparation_view().is_none());

        let outcome = outcome_with_plans(Some(Box::new([])));
        let view = outcome.export_preparation_view().unwrap();
        assert!(view.plans().is_empty());
        assert_eq!(view.typed_key_baselines().len(), 0);
    }
}
