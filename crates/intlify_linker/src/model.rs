// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Linker-owned typed-key models and their private baseline-source relation.
//!
//! The public model exposes only a resolved scope and canonical domain-qualified
//! keys. Baseline locale, message payload, and definition location remain in a
//! private relation retained by `LinkOutcome` for a later checked export stage.

use intlify_contract::{CatalogKey, Locale, MessagePayload};

use crate::{DefinitionLocation, ResolvedCatalogScopeId};

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
