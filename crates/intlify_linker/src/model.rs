// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Linker-owned typed-key models and their private baseline-source relation.
//!
//! The public model exposes only a resolved scope and canonical domain-qualified
//! keys. Baseline locale, message payload, and definition location remain in a
//! private relation retained by `LinkOutcome` for a later checked export stage.

use intlify_contract::{CatalogKey, Locale, MessagePayload};

use crate::{DefinitionLocation, LinkOperationalError, ResolvedCatalogScopeId, ResolvedLinkPolicy};

/// Immutable language-neutral key surface for one resolved catalog scope.
#[derive(Debug, PartialEq, Eq)]
pub struct TypedKeyModel {
    resolved_scope: ResolvedCatalogScopeId,
    keys: Box<[CatalogKey]>,
}

impl TypedKeyModel {
    pub(crate) fn new(resolved_scope: ResolvedCatalogScopeId, keys: Vec<CatalogKey>) -> Self {
        Self {
            resolved_scope,
            keys: keys.into_boxed_slice(),
        }
    }

    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return strictly ordered domain-qualified catalog keys.
    #[must_use]
    pub const fn keys(&self) -> &[CatalogKey] {
        &self.keys
    }
}

/// Exact owned baseline definition associated with one public model key.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct BaselineDefinitionSnapshot {
    resolved_scope: ResolvedCatalogScopeId,
    key: CatalogKey,
    locale: Locale,
    #[allow(dead_code)] // Retained for the future checked export handoff.
    message: MessagePayload,
    #[allow(dead_code)] // Retained for the future checked export handoff.
    location: DefinitionLocation,
}

impl BaselineDefinitionSnapshot {
    pub(crate) fn new(
        resolved_scope: ResolvedCatalogScopeId,
        key: CatalogKey,
        locale: Locale,
        message: MessagePayload,
        location: DefinitionLocation,
    ) -> Self {
        Self {
            resolved_scope,
            key,
            locale,
            message,
            location,
        }
    }

    pub(crate) const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    pub(crate) const fn key(&self) -> &CatalogKey {
        &self.key
    }

    pub(crate) const fn locale(&self) -> &Locale {
        &self.locale
    }

    #[cfg(feature = "benchmark")]
    pub(crate) const fn message(&self) -> &MessagePayload {
        &self.message
    }

    #[cfg(feature = "benchmark")]
    pub(crate) const fn location(&self) -> &DefinitionLocation {
        &self.location
    }
}

/// Private one-to-one source relation for one public typed-key model.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TypedKeyModelSnapshotRelation {
    resolved_scope: ResolvedCatalogScopeId,
    baseline_locale: Locale,
    snapshots: Box<[BaselineDefinitionSnapshot]>,
}

impl TypedKeyModelSnapshotRelation {
    pub(crate) fn new(
        resolved_scope: ResolvedCatalogScopeId,
        baseline_locale: Locale,
        snapshots: Vec<BaselineDefinitionSnapshot>,
    ) -> Self {
        Self {
            resolved_scope,
            baseline_locale,
            snapshots: snapshots.into_boxed_slice(),
        }
    }

    pub(crate) const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    pub(crate) const fn baseline_locale(&self) -> &Locale {
        &self.baseline_locale
    }

    pub(crate) const fn snapshots(&self) -> &[BaselineDefinitionSnapshot] {
        &self.snapshots
    }
}

/// Checked model set and its exact private source relation.
pub(crate) struct TypedKeyModelBatch {
    models: Box<[TypedKeyModel]>,
    relations: Box<[TypedKeyModelSnapshotRelation]>,
}

impl TypedKeyModelBatch {
    pub(crate) fn try_new(
        models: Vec<TypedKeyModel>,
        relations: Vec<TypedKeyModelSnapshotRelation>,
        resolved_policy: &ResolvedLinkPolicy,
    ) -> Result<Self, LinkOperationalError> {
        validate_relations(&models, &relations, resolved_policy)?;
        Ok(Self {
            models: models.into_boxed_slice(),
            relations: relations.into_boxed_slice(),
        })
    }

    #[cfg(feature = "benchmark")]
    pub(crate) const fn models(&self) -> &[TypedKeyModel] {
        &self.models
    }

    #[cfg(feature = "benchmark")]
    pub(crate) const fn relations(&self) -> &[TypedKeyModelSnapshotRelation] {
        &self.relations
    }

    pub(crate) fn into_parts(self) -> (Box<[TypedKeyModel]>, Box<[TypedKeyModelSnapshotRelation]>) {
        (self.models, self.relations)
    }
}

fn validate_relations(
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

    use super::{TypedKeyModelBatch, TypedKeyModelSnapshotRelation};
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
        let models = || vec![TypedKeyModel::new(resolved_scope.clone(), Vec::new())];

        let valid = vec![TypedKeyModelSnapshotRelation::new(
            resolved_scope.clone(),
            Locale::try_new("en").unwrap(),
            Vec::new(),
        )];
        assert!(TypedKeyModelBatch::try_new(models(), valid, &resolved_policy).is_ok());

        let wrong_baseline = vec![TypedKeyModelSnapshotRelation::new(
            resolved_scope.clone(),
            Locale::try_new("ja").unwrap(),
            Vec::new(),
        )];
        assert!(matches!(
            TypedKeyModelBatch::try_new(models(), wrong_baseline, &resolved_policy),
            Err(LinkOperationalError::InternalInvariant)
        ));
        assert!(matches!(
            TypedKeyModelBatch::try_new(models(), Vec::new(), &resolved_policy),
            Err(LinkOperationalError::InternalInvariant)
        ));
    }
}
