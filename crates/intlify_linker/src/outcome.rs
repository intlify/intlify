// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable owned result of one complete semantic link.
//!
//! Outcome construction is private to the linker so blocking findings cannot
//! coexist with plans and callers cannot reorder or replace admitted results.

use std::fmt;

use crate::model::TypedKeyModelSnapshotRelation;
use crate::{
    LinkFinding, LinkOperationalError, MessageBundlePlan, ResolvedLinkPolicy, TypedKeyModel,
};

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
        typed_key_models: Vec<TypedKeyModel>,
        typed_key_model_snapshots: Vec<TypedKeyModelSnapshotRelation>,
        resolved_policy: &ResolvedLinkPolicy,
    ) -> Result<Self, LinkOperationalError> {
        let has_blocking_finding = findings.iter().any(LinkFinding::blocking);
        if bundle_plans.is_none() != has_blocking_finding {
            return Err(LinkOperationalError::InternalInvariant);
        }
        validate_typed_key_model_relations(
            &typed_key_models,
            &typed_key_model_snapshots,
            resolved_policy,
        )?;
        Ok(Self {
            findings: findings.into_boxed_slice(),
            bundle_plans: bundle_plans.map(Vec::into_boxed_slice),
            typed_key_models: typed_key_models.into_boxed_slice(),
            typed_key_model_snapshots: typed_key_model_snapshots.into_boxed_slice(),
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
}

fn validate_typed_key_model_relations(
    models: &[TypedKeyModel],
    relations: &[TypedKeyModelSnapshotRelation],
    resolved_policy: &ResolvedLinkPolicy,
) -> Result<(), LinkOperationalError> {
    if models.len() != relations.len()
        || models
            .windows(2)
            .any(|pair| pair[0].resolved_scope() >= pair[1].resolved_scope())
    {
        return Err(LinkOperationalError::InternalInvariant);
    }

    for (model, relation) in models.iter().zip(relations) {
        let baseline = resolved_policy
            .coverage_baselines()
            .binary_search_by(|baseline| baseline.scope().cmp(model.resolved_scope()))
            .ok()
            .map(|index| &resolved_policy.coverage_baselines()[index])
            .ok_or(LinkOperationalError::InternalInvariant)?;
        if model.resolved_scope() != relation.resolved_scope()
            || relation.baseline_locale() != baseline.locale()
            || model.keys().len() != relation.snapshots().len()
            || model.keys().windows(2).any(|pair| pair[0] >= pair[1])
        {
            return Err(LinkOperationalError::InternalInvariant);
        }
        for (key, snapshot) in model.keys().iter().zip(relation.snapshots()) {
            if snapshot.resolved_scope() != model.resolved_scope()
                || snapshot.key() != key
                || snapshot.locale() != relation.baseline_locale()
            {
                return Err(LinkOperationalError::InternalInvariant);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        ArtifactNamespace, CatalogScopeId, CatalogScopeName, LinkLimits, Locale,
    };

    use super::validate_typed_key_model_relations;
    use crate::model::TypedKeyModelSnapshotRelation;
    use crate::{
        CoverageBaseline, DynamicReferenceMode, LinkOperationalError, LinkPolicy, PlacementPolicy,
        ResolvedLinkPolicy, ScopeMappingTable, TypedKeyModel,
    };

    fn scope(name: &str) -> CatalogScopeId {
        CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new(name).unwrap(),
        )
    }

    #[test]
    fn private_model_relation_must_use_the_resolved_policy_baseline() {
        let app = scope("app");
        let mappings =
            ScopeMappingTable::empty(std::slice::from_ref(&app), &LinkLimits::default()).unwrap();
        let resolved_scope = mappings.resolve(&app);
        let policy = LinkPolicy::try_new(
            vec![
                Locale::try_new("en").unwrap(),
                Locale::try_new("ja").unwrap(),
            ],
            Vec::new(),
            vec![CoverageBaseline::new(app, Locale::try_new("en").unwrap())],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap();
        let resolved_policy = ResolvedLinkPolicy::resolve(&policy, &mappings).unwrap();
        let models = vec![TypedKeyModel::new(resolved_scope.clone(), Vec::new())];

        let valid = vec![TypedKeyModelSnapshotRelation::new(
            resolved_scope.clone(),
            Locale::try_new("en").unwrap(),
            Vec::new(),
        )];
        assert!(validate_typed_key_model_relations(&models, &valid, &resolved_policy).is_ok());

        let wrong_baseline = vec![TypedKeyModelSnapshotRelation::new(
            resolved_scope,
            Locale::try_new("ja").unwrap(),
            Vec::new(),
        )];
        assert_eq!(
            validate_typed_key_model_relations(&models, &wrong_baseline, &resolved_policy),
            Err(LinkOperationalError::InternalInvariant)
        );
        assert_eq!(
            validate_typed_key_model_relations(&models, &[], &resolved_policy),
            Err(LinkOperationalError::InternalInvariant)
        );
    }
}
