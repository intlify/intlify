// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Execution-evidence projection into linker scope completeness.
//!
//! This module converts retained definition and reference participant failures
//! into exactly one checked completeness entry per configuration-resolved target
//! scope. It does not infer missing participants from artifact contents, resolve
//! scope mappings, or perform semantic linking.

use std::collections::BTreeSet;

use intlify_contract::{CatalogScopeId, LinkLimits};
use intlify_linker::{
    InputCompleteness, LinkOperationalError, PartialReason, ScopeCompleteness,
    ScopeCompletenessTable,
};

use super::inventory::DefinitionSourceFailure;
use super::reference::ReferenceSourceFailure;

/// Construct exact per-scope completeness from authoritative execution evidence.
pub(crate) fn build_scope_completeness(
    target_scopes: &[CatalogScopeId],
    definition_failures: &[DefinitionSourceFailure],
    reference_failures: &[ReferenceSourceFailure],
    limits: &LinkLimits,
) -> Result<ScopeCompletenessTable, LinkOperationalError> {
    let definition_failed = collect_failed_scopes(
        definition_failures
            .iter()
            .map(DefinitionSourceFailure::affected_scopes),
    );
    let reference_failed = collect_failed_scopes(
        reference_failures
            .iter()
            .map(ReferenceSourceFailure::affected_scopes),
    );

    build_from_failed_scopes(target_scopes, &definition_failed, &reference_failed, limits)
}

fn build_from_failed_scopes(
    target_scopes: &[CatalogScopeId],
    definition_failed: &BTreeSet<CatalogScopeId>,
    reference_failed: &BTreeSet<CatalogScopeId>,
    limits: &LinkLimits,
) -> Result<ScopeCompletenessTable, LinkOperationalError> {
    let target_set = target_scopes.iter().collect::<BTreeSet<_>>();
    if definition_failed
        .iter()
        .chain(reference_failed)
        .any(|scope| !target_set.contains(scope))
    {
        return Err(LinkOperationalError::InternalInvariant);
    }

    let entries = target_scopes
        .iter()
        .cloned()
        .map(|scope| {
            let definitions = if definition_failed.contains(&scope) {
                InputCompleteness::Partial(PartialReason::SourceFailed)
            } else {
                InputCompleteness::Closed
            };
            let references = if reference_failed.contains(&scope) {
                InputCompleteness::Partial(PartialReason::ProducerFailed)
            } else {
                InputCompleteness::Closed
            };
            ScopeCompleteness::try_new(scope, definitions, references)
                .expect("fixed source and producer failure reasons are valid for their sides")
        })
        .collect();

    ScopeCompletenessTable::try_new(target_scopes, entries, limits)
}

fn collect_failed_scopes<'a>(
    affected: impl Iterator<Item = &'a [CatalogScopeId]>,
) -> BTreeSet<CatalogScopeId> {
    affected.flat_map(|scopes| scopes.iter().cloned()).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use intlify_contract::{ArtifactNamespace, CatalogScopeId, CatalogScopeName, LinkLimits};
    use intlify_linker::{InputCompleteness, LinkOperationalError, PartialReason};

    use super::build_from_failed_scopes;

    fn scope(name: &str) -> CatalogScopeId {
        CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new(name).unwrap(),
        )
    }

    #[test]
    fn no_failure_is_closed_even_when_no_reference_producer_was_configured() {
        let app = scope("app");
        let table = build_from_failed_scopes(
            std::slice::from_ref(&app),
            &BTreeSet::new(),
            &BTreeSet::new(),
            &LinkLimits::default(),
        )
        .unwrap();

        assert_eq!(
            table.get(&app).unwrap().definitions(),
            InputCompleteness::Closed
        );
        assert_eq!(
            table.get(&app).unwrap().references(),
            InputCompleteness::Closed
        );
    }

    #[test]
    fn failures_weaken_only_their_affected_side_and_scope() {
        let app = scope("app");
        let vendor = scope("vendor");
        let definitions = BTreeSet::from([app.clone()]);
        let references = BTreeSet::from([vendor.clone()]);
        let table = build_from_failed_scopes(
            &[app.clone(), vendor.clone()],
            &definitions,
            &references,
            &LinkLimits::default(),
        )
        .unwrap();

        assert_eq!(
            table.get(&app).unwrap().definitions(),
            InputCompleteness::Partial(PartialReason::SourceFailed)
        );
        assert_eq!(
            table.get(&app).unwrap().references(),
            InputCompleteness::Closed
        );
        assert_eq!(
            table.get(&vendor).unwrap().definitions(),
            InputCompleteness::Closed
        );
        assert_eq!(
            table.get(&vendor).unwrap().references(),
            InputCompleteness::Partial(PartialReason::ProducerFailed)
        );
    }

    #[test]
    fn failure_for_an_unknown_scope_is_an_internal_invariant() {
        let app = scope("app");
        let unknown = scope("unknown");
        let references = BTreeSet::from([unknown]);

        assert_eq!(
            build_from_failed_scopes(
                std::slice::from_ref(&app),
                &BTreeSet::new(),
                &references,
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InternalInvariant)
        );
    }
}
