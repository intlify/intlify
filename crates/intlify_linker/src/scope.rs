// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked structural-scope mapping and execution-derived completeness.
//!
//! This module owns exact one-hop mappings, canonical target-scope coverage,
//! side-specific partial evidence, and their resolved request-local forms. It
//! does not infer scopes from artifacts or read user-authored configuration.

use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt;

use intlify_contract::{CatalogScopeId, LinkLimitCounter, LinkLimitSubject, LinkLimits};

use crate::validation::{check_first_over, usize_count};
use crate::{InvalidRequestError, LinkOperationalError, ScopeEndpoint};

/// Catalog scope after application-owned one-hop mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResolvedCatalogScopeId(CatalogScopeId);

impl ResolvedCatalogScopeId {
    /// Return the canonical mapped structural scope.
    #[must_use]
    pub const fn as_catalog_scope(&self) -> &CatalogScopeId {
        &self.0
    }
}

/// One structural-scope mapping submitted by a host integration.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScopeMapping {
    source: CatalogScopeId,
    target: CatalogScopeId,
}

impl ScopeMapping {
    /// Compose one mapping occurrence from independently checked endpoints.
    #[must_use]
    pub const fn new(source: CatalogScopeId, target: CatalogScopeId) -> Self {
        Self { source, target }
    }

    /// Return the mapping source.
    #[must_use]
    pub const fn source(&self) -> &CatalogScopeId {
        &self.source
    }

    /// Return the mapping target.
    #[must_use]
    pub const fn target(&self) -> &CatalogScopeId {
        &self.target
    }
}

/// Immutable canonical one-hop scope mapping table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeMappingTable {
    declared_scopes: Box<[CatalogScopeId]>,
    mappings: Box<[ScopeMapping]>,
}

impl ScopeMappingTable {
    /// Validate endpoints against one declared-scope registry and retain mappings.
    pub fn try_new(
        declared_scopes: &[CatalogScopeId],
        mappings: Vec<ScopeMapping>,
        limits: &LinkLimits,
    ) -> Result<Self, LinkOperationalError> {
        validate_mapping_limits(&mappings, limits)?;
        let declared_scopes = canonical_scope_inventory(declared_scopes)?;

        let declared = declared_scopes.iter().collect::<BTreeSet<_>>();
        let unknown = mappings
            .iter()
            .flat_map(|mapping| {
                [
                    (mapping.source(), ScopeEndpoint::Source),
                    (mapping.target(), ScopeEndpoint::Target),
                ]
            })
            .filter(|(scope, _)| !declared.contains(scope))
            .map(|(scope, endpoint)| (scope.clone(), endpoint))
            .next();
        if let Some((scope, endpoint)) = unknown {
            return Err(InvalidRequestError::UndeclaredMappingEndpoint { endpoint, scope }.into());
        }

        let mut mappings = mappings;
        mappings.sort_by(|left, right| {
            left.source()
                .cmp(right.source())
                .then_with(|| left.target().cmp(right.target()))
        });

        if let Some(pair) = mappings
            .windows(2)
            .find(|pair| pair[0].source() == pair[1].source())
        {
            return Err(
                InvalidRequestError::DuplicateScopeMappingSource(pair[0].source().clone()).into(),
            );
        }
        if let Some(mapping) = mappings
            .iter()
            .find(|mapping| mapping.source() == mapping.target())
        {
            return Err(InvalidRequestError::ScopeMappingSelfMap(mapping.source().clone()).into());
        }

        let sources = mappings
            .iter()
            .map(ScopeMapping::source)
            .collect::<BTreeSet<_>>();
        if let Some(target) = mappings
            .iter()
            .map(ScopeMapping::target)
            .filter(|target| sources.contains(target))
            .min()
        {
            return Err(InvalidRequestError::ScopeMappingTargetIsSource(target.clone()).into());
        }

        Ok(Self {
            declared_scopes: declared_scopes.into_boxed_slice(),
            mappings: mappings.into_boxed_slice(),
        })
    }

    /// Construct the canonical empty mapping table.
    pub fn empty(
        declared_scopes: &[CatalogScopeId],
        limits: &LinkLimits,
    ) -> Result<Self, LinkOperationalError> {
        Self::try_new(declared_scopes, Vec::new(), limits)
    }

    /// Return mappings in canonical source order.
    #[must_use]
    pub const fn mappings(&self) -> &[ScopeMapping] {
        &self.mappings
    }

    /// Resolve one declared scope through exactly one optional mapping hop.
    #[must_use]
    pub fn resolve(&self, scope: &CatalogScopeId) -> ResolvedCatalogScopeId {
        match self
            .mappings
            .binary_search_by(|mapping| mapping.source().cmp(scope))
        {
            Ok(index) => ResolvedCatalogScopeId(self.mappings[index].target().clone()),
            Err(_) => ResolvedCatalogScopeId(scope.clone()),
        }
    }

    pub(crate) const fn declared_scopes(&self) -> &[CatalogScopeId] {
        &self.declared_scopes
    }

    pub(crate) fn revalidate(&self, limits: &LinkLimits) -> Result<(), LinkOperationalError> {
        validate_mapping_limits(&self.mappings, limits)
    }
}

/// Definitions or references side of one completeness entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CompletenessSide {
    /// Catalog definition inputs.
    Definitions,
    /// Reference producer inputs.
    References,
}

/// Deterministic reason one selected input side is not closed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PartialReason {
    /// Editor intentionally analyzes an open world.
    OpenEditorWorld,
    /// A configured catalog source did not participate.
    SourceOmitted,
    /// A configured catalog source failed.
    SourceFailed,
    /// A selected reference producer did not participate.
    ProducerOmitted,
    /// A selected reference producer failed.
    ProducerFailed,
    /// External evidence is not authoritative for this invocation.
    ExternalArtifactUnverified,
}

/// Closed or deliberately partial execution evidence for one side.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InputCompleteness {
    /// Every selected participant completed successfully.
    Closed,
    /// One deterministic reason prevents a closed-world proof.
    Partial(PartialReason),
}

/// Invalid side/reason pairing for one completeness entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeCompletenessConstructionError {
    side: CompletenessSide,
    reason: PartialReason,
}

impl ScopeCompletenessConstructionError {
    /// Return the rejected side.
    #[must_use]
    pub const fn side(&self) -> CompletenessSide {
        self.side
    }

    /// Return the rejected reason.
    #[must_use]
    pub const fn reason(&self) -> PartialReason {
        self.reason
    }
}

impl fmt::Display for ScopeCompletenessConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{:?} is not valid for {:?} completeness",
            self.reason, self.side
        )
    }
}

impl Error for ScopeCompletenessConstructionError {}

/// One exact original-scope completeness entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopeCompleteness {
    scope: CatalogScopeId,
    definitions: InputCompleteness,
    references: InputCompleteness,
}

impl ScopeCompleteness {
    /// Construct one checked execution-derived entry.
    pub fn try_new(
        scope: CatalogScopeId,
        definitions: InputCompleteness,
        references: InputCompleteness,
    ) -> Result<Self, ScopeCompletenessConstructionError> {
        validate_partial_side(CompletenessSide::Definitions, definitions)?;
        validate_partial_side(CompletenessSide::References, references)?;
        Ok(Self {
            scope,
            definitions,
            references,
        })
    }

    /// Return the original declared scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return definition-side execution evidence.
    #[must_use]
    pub const fn definitions(&self) -> InputCompleteness {
        self.definitions
    }

    /// Return reference-side execution evidence.
    #[must_use]
    pub const fn references(&self) -> InputCompleteness {
        self.references
    }
}

/// Complete canonical table for the exact target-scope inventory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeCompletenessTable {
    entries: Box<[ScopeCompleteness]>,
}

impl ScopeCompletenessTable {
    /// Require exactly one entry for every target scope.
    pub fn try_new(
        target_scopes: &[CatalogScopeId],
        entries: Vec<ScopeCompleteness>,
    ) -> Result<Self, LinkOperationalError> {
        let target_scopes = canonical_scope_inventory(target_scopes)?;
        let target_set = target_scopes.iter().collect::<BTreeSet<_>>();

        let mut seen = BTreeSet::new();
        let duplicate = entries
            .iter()
            .map(ScopeCompleteness::scope)
            .filter(|scope| !seen.insert(*scope))
            .min();
        if let Some(scope) = duplicate {
            return Err(InvalidRequestError::DuplicateCompletenessScope(scope.clone()).into());
        }
        if let Some(pair) = entries
            .windows(2)
            .find(|pair| pair[0].scope() > pair[1].scope())
        {
            return Err(InvalidRequestError::NonCanonicalCompletenessOrder {
                previous: pair[0].scope().clone(),
                current: pair[1].scope().clone(),
            }
            .into());
        }

        if let Some(extra) = entries
            .iter()
            .map(ScopeCompleteness::scope)
            .find(|scope| !target_set.contains(scope))
        {
            return Err(InvalidRequestError::ExtraCompletenessScope(extra.clone()).into());
        }

        let entry_set = entries
            .iter()
            .map(ScopeCompleteness::scope)
            .collect::<BTreeSet<_>>();
        if let Some(missing) = target_scopes
            .iter()
            .find(|scope| !entry_set.contains(scope))
        {
            return Err(InvalidRequestError::MissingCompletenessScope(missing.clone()).into());
        }

        Ok(Self {
            entries: entries.into_boxed_slice(),
        })
    }

    /// Return entries in canonical scope order.
    #[must_use]
    pub const fn entries(&self) -> &[ScopeCompleteness] {
        &self.entries
    }

    /// Find one exact original-scope entry.
    #[must_use]
    pub fn get(&self, scope: &CatalogScopeId) -> Option<&ScopeCompleteness> {
        self.entries
            .binary_search_by(|entry| entry.scope().cmp(scope))
            .ok()
            .map(|index| &self.entries[index])
    }

    pub(crate) fn scopes(&self) -> impl Iterator<Item = &CatalogScopeId> {
        self.entries.iter().map(ScopeCompleteness::scope)
    }
}

/// One original-scope reason contributing to resolved partial completeness.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CompletenessContributor {
    scope: CatalogScopeId,
    reason: PartialReason,
}

impl CompletenessContributor {
    /// Return the contributing original scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the deterministic partial reason.
    #[must_use]
    pub const fn reason(&self) -> PartialReason {
        self.reason
    }
}

/// Effective completeness for one resolved scope side.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResolvedInputCompleteness {
    /// Every contributing original scope is closed.
    Closed,
    /// Complete non-empty canonical contributor evidence.
    Partial(Box<[CompletenessContributor]>),
}

/// One resolved scope and both effective completeness sides.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedScopeCompleteness {
    scope: ResolvedCatalogScopeId,
    definitions: ResolvedInputCompleteness,
    references: ResolvedInputCompleteness,
}

impl ResolvedScopeCompleteness {
    /// Return the resolved semantic scope.
    #[must_use]
    pub const fn scope(&self) -> &ResolvedCatalogScopeId {
        &self.scope
    }

    /// Return effective definition completeness.
    #[must_use]
    pub const fn definitions(&self) -> &ResolvedInputCompleteness {
        &self.definitions
    }

    /// Return effective reference completeness.
    #[must_use]
    pub const fn references(&self) -> &ResolvedInputCompleteness {
        &self.references
    }
}

/// Canonical completeness table after one-hop scope resolution.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedScopeCompletenessTable {
    entries: Box<[ResolvedScopeCompleteness]>,
}

impl ResolvedScopeCompletenessTable {
    pub(crate) fn resolve(table: &ScopeCompletenessTable, mappings: &ScopeMappingTable) -> Self {
        let mut groups: BTreeMap<
            ResolvedCatalogScopeId,
            (Vec<CompletenessContributor>, Vec<CompletenessContributor>),
        > = BTreeMap::new();

        for entry in table.entries() {
            let group = groups.entry(mappings.resolve(entry.scope())).or_default();
            if let InputCompleteness::Partial(reason) = entry.definitions() {
                group.0.push(CompletenessContributor {
                    scope: entry.scope().clone(),
                    reason,
                });
            }
            if let InputCompleteness::Partial(reason) = entry.references() {
                group.1.push(CompletenessContributor {
                    scope: entry.scope().clone(),
                    reason,
                });
            }
        }

        let entries = groups
            .into_iter()
            .map(|(scope, (mut definitions, mut references))| {
                definitions.sort();
                references.sort();
                ResolvedScopeCompleteness {
                    scope,
                    definitions: resolved_side(definitions),
                    references: resolved_side(references),
                }
            })
            .collect::<Vec<_>>()
            .into_boxed_slice();
        Self { entries }
    }

    /// Return entries in canonical resolved-scope order.
    #[must_use]
    pub const fn entries(&self) -> &[ResolvedScopeCompleteness] {
        &self.entries
    }

    /// Find one exact resolved-scope entry.
    #[must_use]
    pub fn get(&self, scope: &ResolvedCatalogScopeId) -> Option<&ResolvedScopeCompleteness> {
        self.entries
            .binary_search_by(|entry| entry.scope().cmp(scope))
            .ok()
            .map(|index| &self.entries[index])
    }
}

fn validate_mapping_limits(
    mappings: &[ScopeMapping],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let subject = LinkLimitSubject::ScopeMappings;
    check_first_over(
        LinkLimitCounter::ScopeMappingEntries,
        subject.clone(),
        usize_count(mappings.len()),
        limits,
    )?;
    for mapping in mappings {
        for scope in [mapping.source(), mapping.target()] {
            check_first_over(
                LinkLimitCounter::CatalogScopeNameBytes,
                subject.clone(),
                usize_count(scope.name().as_str().len()),
                limits,
            )?;
        }
    }
    Ok(())
}

fn canonical_scope_inventory(
    scopes: &[CatalogScopeId],
) -> Result<Vec<CatalogScopeId>, LinkOperationalError> {
    let mut scopes = scopes.to_vec();
    scopes.sort();
    if let Some(pair) = scopes.windows(2).find(|pair| pair[0] == pair[1]) {
        return Err(InvalidRequestError::DuplicateDeclaredScope(pair[0].clone()).into());
    }
    Ok(scopes)
}

fn validate_partial_side(
    side: CompletenessSide,
    completeness: InputCompleteness,
) -> Result<(), ScopeCompletenessConstructionError> {
    let InputCompleteness::Partial(reason) = completeness else {
        return Ok(());
    };
    let admitted = match side {
        CompletenessSide::Definitions => matches!(
            reason,
            PartialReason::OpenEditorWorld
                | PartialReason::SourceOmitted
                | PartialReason::SourceFailed
                | PartialReason::ExternalArtifactUnverified
        ),
        CompletenessSide::References => matches!(
            reason,
            PartialReason::OpenEditorWorld
                | PartialReason::ProducerOmitted
                | PartialReason::ProducerFailed
                | PartialReason::ExternalArtifactUnverified
        ),
    };
    if admitted {
        Ok(())
    } else {
        Err(ScopeCompletenessConstructionError { side, reason })
    }
}

fn resolved_side(contributors: Vec<CompletenessContributor>) -> ResolvedInputCompleteness {
    if contributors.is_empty() {
        ResolvedInputCompleteness::Closed
    } else {
        ResolvedInputCompleteness::Partial(contributors.into_boxed_slice())
    }
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        ArtifactNamespace, CatalogScopeId, CatalogScopeName, LinkLimitCounter,
        LinkLimitObservation, LinkLimits,
    };

    use super::{
        CompletenessSide, InputCompleteness, PartialReason, ResolvedInputCompleteness,
        ResolvedScopeCompletenessTable, ScopeCompleteness, ScopeCompletenessTable, ScopeMapping,
        ScopeMappingTable,
    };
    use crate::{InvalidRequestError, LinkOperationalError};

    fn scope(name: &str) -> CatalogScopeId {
        CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new(name).unwrap(),
        )
    }

    fn closed(scope: CatalogScopeId) -> ScopeCompleteness {
        ScopeCompleteness::try_new(scope, InputCompleteness::Closed, InputCompleteness::Closed)
            .unwrap()
    }

    #[test]
    fn mapping_accepts_empty_and_many_to_one_but_rejects_chains() {
        let scopes = [scope("a"), scope("b"), scope("target")];
        assert!(ScopeMappingTable::empty(&scopes, &LinkLimits::default()).is_ok());

        let table = ScopeMappingTable::try_new(
            &scopes,
            vec![
                ScopeMapping::new(scopes[1].clone(), scopes[2].clone()),
                ScopeMapping::new(scopes[0].clone(), scopes[2].clone()),
            ],
            &LinkLimits::default(),
        )
        .unwrap();
        assert_eq!(table.mappings()[0].source(), &scopes[0]);
        assert_eq!(table.resolve(&scopes[0]).as_catalog_scope(), &scopes[2]);

        let chain = ScopeMappingTable::try_new(
            &scopes,
            vec![
                ScopeMapping::new(scopes[0].clone(), scopes[1].clone()),
                ScopeMapping::new(scopes[1].clone(), scopes[2].clone()),
            ],
            &LinkLimits::default(),
        )
        .unwrap_err();
        assert_eq!(
            chain,
            LinkOperationalError::InvalidRequest(InvalidRequestError::ScopeMappingTargetIsSource(
                scopes[1].clone()
            ))
        );
    }

    #[test]
    fn mapping_count_limit_wins_before_duplicate_semantics() {
        let scopes = [scope("a"), scope("b")];
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ScopeMappingEntries, 1)
            .unwrap();
        let error = ScopeMappingTable::try_new(
            &scopes,
            vec![
                ScopeMapping::new(scopes[0].clone(), scopes[1].clone()),
                ScopeMapping::new(scopes[0].clone(), scopes[1].clone()),
            ],
            &limits,
        )
        .unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::ScopeMappingEntries);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(2));
    }

    #[test]
    fn completeness_requires_exact_inventory_and_side_specific_reasons() {
        let app = scope("app");
        assert!(ScopeCompleteness::try_new(
            app.clone(),
            InputCompleteness::Partial(PartialReason::ProducerFailed),
            InputCompleteness::Closed,
        )
        .is_err_and(|error| error.side() == CompletenessSide::Definitions));

        let missing =
            ScopeCompletenessTable::try_new(std::slice::from_ref(&app), Vec::new()).unwrap_err();
        assert_eq!(
            missing,
            LinkOperationalError::InvalidRequest(InvalidRequestError::MissingCompletenessScope(
                app.clone()
            ))
        );

        let duplicate = ScopeCompletenessTable::try_new(
            std::slice::from_ref(&app),
            vec![closed(app.clone()), closed(app.clone())],
        )
        .unwrap_err();
        assert_eq!(
            duplicate,
            LinkOperationalError::InvalidRequest(InvalidRequestError::DuplicateCompletenessScope(
                app
            ))
        );

        let a = scope("a");
        let b = scope("b");
        let unsorted = ScopeCompletenessTable::try_new(
            &[a.clone(), b.clone()],
            vec![closed(b.clone()), closed(a.clone())],
        )
        .unwrap_err();
        assert_eq!(
            unsorted,
            LinkOperationalError::InvalidRequest(
                InvalidRequestError::NonCanonicalCompletenessOrder {
                    previous: b,
                    current: a,
                }
            )
        );
    }

    #[test]
    fn resolved_completeness_preserves_every_partial_contributor() {
        let source = scope("source");
        let target = scope("target");
        let entries = vec![
            ScopeCompleteness::try_new(
                source.clone(),
                InputCompleteness::Partial(PartialReason::SourceFailed),
                InputCompleteness::Closed,
            )
            .unwrap(),
            ScopeCompleteness::try_new(
                target.clone(),
                InputCompleteness::Closed,
                InputCompleteness::Partial(PartialReason::ProducerOmitted),
            )
            .unwrap(),
        ];
        let table =
            ScopeCompletenessTable::try_new(&[source.clone(), target.clone()], entries).unwrap();
        let mappings = ScopeMappingTable::try_new(
            &[source.clone(), target.clone()],
            vec![ScopeMapping::new(source.clone(), target.clone())],
            &LinkLimits::default(),
        )
        .unwrap();

        let resolved = ResolvedScopeCompletenessTable::resolve(&table, &mappings);
        assert_eq!(resolved.entries().len(), 1);
        let ResolvedInputCompleteness::Partial(definitions) = resolved.entries()[0].definitions()
        else {
            panic!("expected partial definition evidence");
        };
        assert_eq!(definitions[0].scope(), &source);
        let ResolvedInputCompleteness::Partial(references) = resolved.entries()[0].references()
        else {
            panic!("expected partial reference evidence");
        };
        assert_eq!(references[0].scope(), &target);
    }
}
