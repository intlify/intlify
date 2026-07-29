// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable owned result of one complete semantic link.
//!
//! Outcome construction is private to the linker so blocking findings cannot
//! coexist with plans and callers cannot reorder or replace admitted results.

use crate::{LinkFinding, LinkOperationalError, MessageBundlePlan};

/// Fully owned deterministic semantic result.
#[derive(Debug, PartialEq, Eq)]
pub struct LinkOutcome {
    findings: Box<[LinkFinding]>,
    bundle_plans: Option<Box<[MessageBundlePlan]>>,
}

impl LinkOutcome {
    pub(crate) fn try_new(
        findings: Vec<LinkFinding>,
        bundle_plans: Option<Vec<MessageBundlePlan>>,
    ) -> Result<Self, LinkOperationalError> {
        let has_blocking_finding = findings.iter().any(LinkFinding::blocking);
        if bundle_plans.is_none() != has_blocking_finding {
            return Err(LinkOperationalError::InternalInvariant);
        }
        Ok(Self {
            findings: findings.into_boxed_slice(),
            bundle_plans: bundle_plans.map(Vec::into_boxed_slice),
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

    /// Return whether semantic findings withheld every bundle plan.
    #[must_use]
    pub const fn generation_blocked(&self) -> bool {
        self.bundle_plans.is_none()
    }
}
