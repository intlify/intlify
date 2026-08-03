// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Purpose-bound read-only handoff from linking to export preparation.
//!
//! This feature-gated module lends one checked outcome's plans and private
//! typed-key baseline relation without adding a general baseline lookup or an
//! owned conversion. Construction remains private to [`crate::LinkOutcome`].

use intlify_contract::{CatalogKey, Locale, MessagePayload};

use crate::model::{BaselineDefinitionSnapshot, TypedKeyModelSnapshotRelation};
use crate::{DefinitionLocation, MessageBundlePlan, ResolvedCatalogScopeId, TypedKeyModel};

/// Borrowed export-preparation input for one successful link outcome.
#[derive(Clone, Copy)]
pub struct ExportPreparationView<'a> {
    plans: &'a [MessageBundlePlan],
    models: &'a [TypedKeyModel],
    relations: &'a [TypedKeyModelSnapshotRelation],
}

impl<'a> ExportPreparationView<'a> {
    pub(crate) const fn new(
        plans: &'a [MessageBundlePlan],
        models: &'a [TypedKeyModel],
        relations: &'a [TypedKeyModelSnapshotRelation],
    ) -> Self {
        Self {
            plans,
            models,
            relations,
        }
    }

    /// Return canonical bundle plans from the checked outcome.
    #[must_use]
    pub const fn plans(&self) -> &'a [MessageBundlePlan] {
        self.plans
    }

    /// Iterate canonical typed-key models with their exact baseline relation.
    pub fn typed_key_baselines(
        &self,
    ) -> impl ExactSizeIterator<Item = TypedKeyBaselineView<'a>> + 'a {
        let models = self.models;
        let relations = self.relations;
        models
            .iter()
            .zip(relations)
            .map(|(model, relation)| TypedKeyBaselineView {
                model,
                baseline_locale: relation.baseline_locale(),
                definitions: relation.snapshots(),
            })
    }
}

/// One public typed-key model paired with its checked private baseline source.
#[derive(Clone, Copy)]
pub struct TypedKeyBaselineView<'a> {
    model: &'a TypedKeyModel,
    baseline_locale: &'a Locale,
    definitions: &'a [BaselineDefinitionSnapshot],
}

impl<'a> TypedKeyBaselineView<'a> {
    /// Return the canonical public typed-key model.
    #[must_use]
    pub const fn model(&self) -> &'a TypedKeyModel {
        self.model
    }

    /// Return the coverage-baseline locale used by every definition.
    #[must_use]
    pub const fn baseline_locale(&self) -> &'a Locale {
        self.baseline_locale
    }

    /// Iterate baseline definitions in exact model-key order.
    pub fn definitions(&self) -> impl ExactSizeIterator<Item = BaselineDefinitionView<'a>> + 'a {
        let definitions = self.definitions;
        definitions
            .iter()
            .map(|snapshot| BaselineDefinitionView { snapshot })
    }
}

/// Borrowed exact baseline definition associated with one typed model key.
#[derive(Clone, Copy)]
pub struct BaselineDefinitionView<'a> {
    snapshot: &'a BaselineDefinitionSnapshot,
}

impl<'a> BaselineDefinitionView<'a> {
    #[must_use]
    pub const fn resolved_scope(&self) -> &'a ResolvedCatalogScopeId {
        self.snapshot.resolved_scope()
    }

    #[must_use]
    pub const fn key(&self) -> &'a CatalogKey {
        self.snapshot.key()
    }

    #[must_use]
    pub const fn locale(&self) -> &'a Locale {
        self.snapshot.locale()
    }

    #[must_use]
    pub const fn message(&self) -> &'a MessagePayload {
        self.snapshot.message()
    }

    #[must_use]
    pub const fn location(&self) -> &'a DefinitionLocation {
        self.snapshot.location()
    }
}
