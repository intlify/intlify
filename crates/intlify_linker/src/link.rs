// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Stateless semantic linking over one completely admitted request.
//!
//! This module builds canonical semantic indexes, expands selectors, records
//! reachability, applies result budgets, and constructs owned findings and
//! plans. It treats message payloads as opaque checked strings.

use std::collections::{BTreeMap, BTreeSet};
#[cfg(feature = "benchmark")]
use std::time::{Duration, Instant};

use intlify_contract::{
    CatalogKey, CatalogKeyDomain, DeliveryUnitId, EntryReference, LinkLimitCounter,
    LinkLimitEvidence, LinkLimitObservation, LinkLimitSubject, Locale, MessagePayload,
    MessageSelector, PatternToken, ReasonText, ReferenceRecordIdentity, SourceDocumentIdentity,
    SourceOrigin,
};

#[cfg(feature = "benchmark")]
use crate::benchmark::{BenchmarkLinkExecution, BenchmarkLinkStage, BenchmarkLinkStageMeasurement};
use crate::{
    AmbiguousMessageDefinitionFinding, CompletenessContributor, CompletenessSide,
    DefinitionLocation, DegradedAnalysisFinding, DynamicReferenceMode, LinkFinding,
    LinkFindingRecord, LinkOperationalError, LinkOutcome, LinkRequest, MessageBundlePlan,
    PartialCompletenessDegradation, PlacementPolicy, ResolutionFailure, ResolvedCatalogScopeId,
    ResolvedInputCompleteness, ResolvedMessage, ResolvedScopeCompleteness,
    UnboundedDynamicReferenceFinding, UnresolvedMessageFinding, UnusedMessageFinding,
    WideSelectorDegradation,
};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ScopeDomain {
    scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LogicalMessageIdentity {
    scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct DefinitionIdentity {
    logical: LogicalMessageIdentity,
    locale: Locale,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DefinitionSnapshot {
    logical: LogicalMessageIdentity,
    locale: Locale,
    message: MessagePayload,
    location: DefinitionLocation,
}

type UniqueDefinitions = BTreeMap<LogicalMessageIdentity, BTreeMap<Locale, DefinitionSnapshot>>;
type DeliveryUnitReachability = BTreeMap<DeliveryUnitId, BTreeSet<LogicalMessageIdentity>>;

#[derive(Debug, Clone, PartialEq, Eq)]
struct ReferenceFact {
    identity: ReferenceRecordIdentity,
    delivery_unit: DeliveryUnitId,
    scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    selector: MessageSelector,
    reason: Option<ReasonText>,
    origin: Option<SourceOrigin>,
}

struct SemanticIndex {
    definitions: BTreeMap<DefinitionIdentity, Vec<DefinitionSnapshot>>,
    candidates: BTreeMap<ScopeDomain, BTreeSet<CatalogKey>>,
    references: Vec<ReferenceFact>,
}

struct DefinitionAnalysis {
    ambiguities: Vec<AmbiguityFact>,
    ambiguous_logical: BTreeSet<LogicalMessageIdentity>,
    unique_definitions: UniqueDefinitions,
}

struct AmbiguityFact {
    identity: DefinitionIdentity,
    definitions: Vec<DefinitionLocation>,
}

struct ReferenceResolution {
    reference: ReferenceFact,
    selected: BTreeSet<LogicalMessageIdentity>,
    unresolved: bool,
}

struct Reachability {
    by_delivery_unit: DeliveryUnitReachability,
    reachable: BTreeSet<LogicalMessageIdentity>,
    unbounded_scope_domains: BTreeSet<ScopeDomain>,
}

impl ReferenceFact {
    fn scope_domain(&self) -> ScopeDomain {
        ScopeDomain {
            scope: self.scope.clone(),
            domain: self.domain,
        }
    }

    fn is_unbounded_dynamic(&self) -> bool {
        matches!(self.selector, MessageSelector::UnboundedDynamic)
    }

    fn is_wide_selector(&self) -> bool {
        matches!(self.selector, MessageSelector::AllInScope)
    }
}

impl ReferenceResolution {
    const fn is_unresolved(&self) -> bool {
        self.unresolved
    }

    fn is_unbounded_dynamic(&self) -> bool {
        self.reference.is_unbounded_dynamic()
    }

    fn is_wide_selector(&self) -> bool {
        self.reference.is_wide_selector()
    }
}

/// Link one completely admitted immutable request.
pub fn link(request: &LinkRequest<'_>) -> Result<LinkOutcome, LinkOperationalError> {
    link_with_observer(request, &mut NoopLinkObserver)
}

trait LinkObserver {
    fn begin(&mut self, _stage: LinkStage) {}

    fn finish_semantic_index(&mut self, _index: &SemanticIndex, _definitions: &DefinitionAnalysis) {
    }

    fn finish_selector_resolution(&mut self, _resolutions: &[ReferenceResolution]) {}

    fn finish_reachability(&mut self, _reachability: &Reachability) {}

    fn finish_outcome(&mut self, _outcome: &LinkOutcome) {}
}

struct NoopLinkObserver;

impl LinkObserver for NoopLinkObserver {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LinkStage {
    SemanticIndexConstruction,
    SelectorExpansionAndReferenceResolution,
    ReachabilityAndPlacement,
    FindingAndPlanMaterialization,
}

fn link_with_observer<O>(
    request: &LinkRequest<'_>,
    observer: &mut O,
) -> Result<LinkOutcome, LinkOperationalError>
where
    O: LinkObserver,
{
    observer.begin(LinkStage::SemanticIndexConstruction);
    let index = build_semantic_index(request)?;
    let definitions = analyze_definitions(&index);
    observer.finish_semantic_index(&index, &definitions);

    observer.begin(LinkStage::SelectorExpansionAndReferenceResolution);
    let mut pattern_budget = PatternWorkBudget::new(request);
    let resolutions = resolve_references(request, &index, &mut pattern_budget)?;
    observer.finish_selector_resolution(&resolutions);

    observer.begin(LinkStage::ReachabilityAndPlacement);
    let reachability = match request.resolved_policy().placement() {
        PlacementPolicy::Duplicate => build_duplicate_reachability(
            request,
            &index,
            &definitions.ambiguous_logical,
            &resolutions,
            &mut pattern_budget,
        )?,
    };
    observer.finish_reachability(&reachability);

    observer.begin(LinkStage::FindingAndPlanMaterialization);
    let findings = materialize_findings(request, &definitions, &resolutions, &reachability)?;

    let bundle_plans = if findings.iter().any(LinkFinding::blocking) {
        None
    } else {
        Some(materialize_plans(
            request,
            &definitions.unique_definitions,
            &reachability.by_delivery_unit,
        )?)
    };

    let outcome = LinkOutcome::try_new(findings, bundle_plans)?;
    observer.finish_outcome(&outcome);
    Ok(outcome)
}

#[cfg(feature = "benchmark")]
pub(crate) fn benchmark_link(
    request: &LinkRequest<'_>,
) -> Result<BenchmarkLinkExecution, LinkOperationalError> {
    let mut observer = BenchmarkLinkObserver::new();
    let outcome = link_with_observer(request, &mut observer)?;
    let (stages, observation_overhead) = observer.finish()?;
    Ok(BenchmarkLinkExecution::new(
        outcome,
        stages,
        observation_overhead,
    ))
}

#[cfg(feature = "benchmark")]
struct BenchmarkLinkObserver {
    active: Option<(LinkStage, Instant)>,
    stages: Vec<BenchmarkLinkStageMeasurement>,
    observed: Vec<bool>,
    observation_overhead: Duration,
    invariant_failed: bool,
}

#[cfg(feature = "benchmark")]
impl BenchmarkLinkObserver {
    fn new() -> Self {
        Self {
            active: None,
            stages: Vec::with_capacity(4),
            observed: Vec::with_capacity(4),
            observation_overhead: Duration::ZERO,
            invariant_failed: false,
        }
    }

    fn complete(&mut self, expected: LinkStage) -> Option<usize> {
        let Some((active, started)) = self.active.take() else {
            self.invariant_failed = true;
            return None;
        };
        if active != expected {
            self.invariant_failed = true;
            return None;
        }
        self.stages.push(BenchmarkLinkStageMeasurement::new(
            benchmark_stage(expected),
            started.elapsed(),
            0,
        ));
        self.observed.push(false);
        Some(self.stages.len() - 1)
    }

    fn observe(&mut self, index: Option<usize>, checksum: u32) {
        let Some(index) = index else {
            return;
        };
        if self.observed.get(index).copied() != Some(false) {
            self.invariant_failed = true;
            return;
        }
        let measurement = &self.stages[index];
        self.stages[index] = BenchmarkLinkStageMeasurement::new(
            measurement.stage(),
            measurement.elapsed(),
            checksum,
        );
        self.observed[index] = true;
    }

    fn observe_with(&mut self, index: Option<usize>, checksum: impl FnOnce() -> u32) {
        let observation_started = Instant::now();
        let checksum = checksum();
        self.observe(index, checksum);
        self.observation_overhead = self
            .observation_overhead
            .saturating_add(observation_started.elapsed());
    }

    fn finish(
        self,
    ) -> Result<(Vec<BenchmarkLinkStageMeasurement>, Duration), LinkOperationalError> {
        if self.invariant_failed
            || self.active.is_some()
            || self.stages.len() != 4
            || self.observed.len() != self.stages.len()
            || self.observed.iter().any(|observed| !observed)
            || self
                .stages
                .iter()
                .map(BenchmarkLinkStageMeasurement::stage)
                .ne([
                    BenchmarkLinkStage::SemanticIndexConstruction,
                    BenchmarkLinkStage::SelectorExpansionAndReferenceResolution,
                    BenchmarkLinkStage::ReachabilityAndPlacement,
                    BenchmarkLinkStage::FindingAndPlanMaterialization,
                ])
        {
            return Err(LinkOperationalError::InternalInvariant);
        }
        Ok((self.stages, self.observation_overhead))
    }
}

#[cfg(feature = "benchmark")]
impl LinkObserver for BenchmarkLinkObserver {
    fn begin(&mut self, stage: LinkStage) {
        if self.active.replace((stage, Instant::now())).is_some() {
            self.invariant_failed = true;
        }
    }

    fn finish_semantic_index(&mut self, index: &SemanticIndex, definitions: &DefinitionAnalysis) {
        let measurement = self.complete(LinkStage::SemanticIndexConstruction);
        self.observe_with(measurement, || checksum_semantic_index(index, definitions));
    }

    fn finish_selector_resolution(&mut self, resolutions: &[ReferenceResolution]) {
        let measurement = self.complete(LinkStage::SelectorExpansionAndReferenceResolution);
        self.observe_with(measurement, || checksum_resolutions(resolutions));
    }

    fn finish_reachability(&mut self, reachability: &Reachability) {
        let measurement = self.complete(LinkStage::ReachabilityAndPlacement);
        self.observe_with(measurement, || checksum_reachability(reachability));
    }

    fn finish_outcome(&mut self, outcome: &LinkOutcome) {
        let measurement = self.complete(LinkStage::FindingAndPlanMaterialization);
        self.observe_with(measurement, || checksum_outcome(outcome));
    }
}

#[cfg(feature = "benchmark")]
const fn benchmark_stage(stage: LinkStage) -> BenchmarkLinkStage {
    match stage {
        LinkStage::SemanticIndexConstruction => BenchmarkLinkStage::SemanticIndexConstruction,
        LinkStage::SelectorExpansionAndReferenceResolution => {
            BenchmarkLinkStage::SelectorExpansionAndReferenceResolution
        }
        LinkStage::ReachabilityAndPlacement => BenchmarkLinkStage::ReachabilityAndPlacement,
        LinkStage::FindingAndPlanMaterialization => {
            BenchmarkLinkStage::FindingAndPlanMaterialization
        }
    }
}

#[cfg(feature = "benchmark")]
fn checksum_semantic_index(index: &SemanticIndex, definitions: &DefinitionAnalysis) -> u32 {
    let mut checksum = BenchmarkChecksum::new(b"intlify-link-semantic-index-v1");
    checksum.write_u64(0x01, index.definitions.len() as u64);
    for (identity, snapshots) in &index.definitions {
        checksum.write_definition_identity(0x02, identity);
        checksum.write_u64(0x03, snapshots.len() as u64);
        for snapshot in snapshots {
            checksum.write_definition_snapshot(0x04, snapshot);
        }
    }
    checksum.write_u64(0x05, index.candidates.len() as u64);
    for (scope_domain, keys) in &index.candidates {
        checksum.write_scope_domain(0x06, scope_domain);
        checksum.write_u64(0x07, keys.len() as u64);
        for key in keys {
            checksum.write_str(0x08, key.as_str());
        }
    }
    checksum.write_u64(0x09, index.references.len() as u64);
    for reference in &index.references {
        checksum.write_reference_fact(0x0a, reference);
    }
    checksum.write_u64(0x0b, definitions.ambiguities.len() as u64);
    checksum.write_u64(0x0c, definitions.unique_definitions.len() as u64);
    checksum.finish()
}

#[cfg(feature = "benchmark")]
fn checksum_resolutions(resolutions: &[ReferenceResolution]) -> u32 {
    let mut checksum = BenchmarkChecksum::new(b"intlify-link-selector-resolution-v1");
    checksum.write_u64(0x01, resolutions.len() as u64);
    for resolution in resolutions {
        checksum.write_reference_fact(0x02, &resolution.reference);
        checksum.write_bool(0x03, resolution.unresolved);
        checksum.write_u64(0x04, resolution.selected.len() as u64);
        for selected in &resolution.selected {
            checksum.write_logical_identity(0x05, selected);
        }
    }
    checksum.finish()
}

#[cfg(feature = "benchmark")]
fn checksum_reachability(reachability: &Reachability) -> u32 {
    let mut checksum = BenchmarkChecksum::new(b"intlify-link-reachability-placement-v1");
    checksum.write_u64(0x01, reachability.by_delivery_unit.len() as u64);
    for (delivery_unit, identities) in &reachability.by_delivery_unit {
        checksum.write_delivery_unit(0x02, delivery_unit);
        checksum.write_u64(0x03, identities.len() as u64);
        for identity in identities {
            checksum.write_logical_identity(0x04, identity);
        }
    }
    checksum.write_u64(0x05, reachability.reachable.len() as u64);
    for identity in &reachability.reachable {
        checksum.write_logical_identity(0x06, identity);
    }
    checksum.write_u64(0x07, reachability.unbounded_scope_domains.len() as u64);
    for scope_domain in &reachability.unbounded_scope_domains {
        checksum.write_scope_domain(0x08, scope_domain);
    }
    checksum.finish()
}

#[cfg(feature = "benchmark")]
fn checksum_outcome(outcome: &LinkOutcome) -> u32 {
    let mut checksum = BenchmarkChecksum::new(b"intlify-link-finding-plan-v1");
    checksum.write_u64(0x01, outcome.findings().len() as u64);
    for finding in outcome.findings() {
        checksum.write_u64(0x02, u64::from(finding.kind().precedence()));
        checksum.write_bool(0x03, finding.blocking());
        checksum.write_resolved_scope(0x04, finding.resolved_scope());
    }
    match outcome.bundle_plans() {
        None => checksum.write_bool(0x05, false),
        Some(plans) => {
            checksum.write_bool(0x05, true);
            checksum.write_u64(0x06, plans.len() as u64);
            for plan in plans {
                checksum.write_delivery_unit(0x07, plan.delivery_unit());
                checksum.write_str(0x08, plan.locale().as_str());
                checksum.write_u64(0x09, plan.messages().len() as u64);
                for message in plan.messages() {
                    checksum.write_resolved_scope(0x0a, message.resolved_scope());
                    checksum.write_str(0x0b, message.domain().as_str());
                    checksum.write_str(0x0c, message.key().as_str());
                    checksum.write_str(0x0d, message.definition_locale().as_str());
                    checksum.write_str(0x0e, message.message().as_str());
                    checksum.write_definition_location(0x0f, message.definition());
                }
            }
        }
    }
    checksum.finish()
}

#[cfg(feature = "benchmark")]
struct BenchmarkChecksum {
    state: u32,
}

#[cfg(feature = "benchmark")]
impl BenchmarkChecksum {
    const OFFSET_BASIS: u32 = 2_166_136_261;
    const MULTIPLIER: u32 = 16_777_619;

    fn new(domain: &[u8]) -> Self {
        let mut checksum = Self {
            state: Self::OFFSET_BASIS,
        };
        checksum.update(domain);
        checksum.update(&[0]);
        checksum
    }

    fn finish(self) -> u32 {
        self.state
    }

    fn update(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.state ^= u32::from(*byte);
            self.state = self.state.wrapping_mul(Self::MULTIPLIER);
        }
    }

    fn write_field(&mut self, tag: u8, payload: &[u8]) {
        self.update(&[tag]);
        self.update(&(payload.len() as u64).to_be_bytes());
        self.update(payload);
    }

    fn write_str(&mut self, tag: u8, value: &str) {
        self.write_field(tag, value.as_bytes());
    }

    fn write_u64(&mut self, tag: u8, value: u64) {
        self.write_field(tag, &value.to_be_bytes());
    }

    fn write_bool(&mut self, tag: u8, value: bool) {
        self.write_field(tag, &[u8::from(value)]);
    }

    fn write_scope_domain(&mut self, tag: u8, value: &ScopeDomain) {
        let mut nested = Self::new(b"scope-domain");
        nested.write_resolved_scope(0x01, &value.scope);
        nested.write_str(0x02, value.domain.as_str());
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_logical_identity(&mut self, tag: u8, value: &LogicalMessageIdentity) {
        let mut nested = Self::new(b"logical-message-identity");
        nested.write_resolved_scope(0x01, &value.scope);
        nested.write_str(0x02, value.domain.as_str());
        nested.write_str(0x03, value.key.as_str());
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_definition_identity(&mut self, tag: u8, value: &DefinitionIdentity) {
        let mut nested = Self::new(b"definition-identity");
        nested.write_logical_identity(0x01, &value.logical);
        nested.write_str(0x02, value.locale.as_str());
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_definition_snapshot(&mut self, tag: u8, value: &DefinitionSnapshot) {
        let mut nested = Self::new(b"definition-snapshot");
        nested.write_logical_identity(0x01, &value.logical);
        nested.write_str(0x02, value.locale.as_str());
        nested.write_str(0x03, value.message.as_str());
        nested.write_definition_location(0x04, &value.location);
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_reference_fact(&mut self, tag: u8, value: &ReferenceFact) {
        let mut nested = Self::new(b"reference-fact");
        nested.write_reference_identity(0x01, &value.identity);
        nested.write_delivery_unit(0x02, &value.delivery_unit);
        nested.write_resolved_scope(0x03, &value.scope);
        nested.write_str(0x04, value.domain.as_str());
        nested.write_selector(0x05, &value.selector);
        match &value.reason {
            Some(reason) => {
                nested.write_bool(0x06, true);
                nested.write_str(0x07, reason.as_str());
            }
            None => nested.write_bool(0x06, false),
        }
        match &value.origin {
            Some(origin) => {
                nested.write_bool(0x08, true);
                nested.write_source_identity(0x09, origin.source());
                nested.write_u64(0x0a, u64::from(origin.span().start()));
                nested.write_u64(0x0b, u64::from(origin.span().end()));
            }
            None => nested.write_bool(0x08, false),
        }
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_resolved_scope(&mut self, tag: u8, value: &ResolvedCatalogScopeId) {
        self.write_scope(tag, value.as_catalog_scope());
    }

    fn write_scope(&mut self, tag: u8, value: &intlify_contract::CatalogScopeId) {
        let mut nested = Self::new(b"catalog-scope");
        nested.write_str(0x01, value.namespace().as_str());
        nested.write_str(0x02, value.name().as_str());
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_reference_identity(&mut self, tag: u8, value: &ReferenceRecordIdentity) {
        let mut nested = Self::new(b"reference-record-identity");
        nested.write_str(0x01, value.artifact().namespace().as_str());
        nested.write_u64(0x02, value.artifact().segments().len() as u64);
        for segment in value.artifact().segments() {
            nested.write_str(0x03, segment.as_str());
        }
        nested.write_u64(0x04, u64::from(value.ordinal()));
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_delivery_unit(&mut self, tag: u8, value: &DeliveryUnitId) {
        let mut nested = Self::new(b"delivery-unit");
        nested.write_u64(0x01, value.segments().len() as u64);
        for segment in value.segments() {
            nested.write_str(0x02, segment.as_str());
        }
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_selector(&mut self, tag: u8, value: &MessageSelector) {
        let mut nested = Self::new(b"message-selector");
        nested.write_str(0x01, value.kind_str());
        match value {
            MessageSelector::Exact(key) => nested.write_str(0x02, key.as_str()),
            MessageSelector::Prefix(prefix) => nested.write_str(0x02, prefix.as_str()),
            MessageSelector::Pattern(pattern) => nested.write_str(0x02, pattern.as_str()),
            MessageSelector::AllInScope | MessageSelector::UnboundedDynamic => {}
        }
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_source_identity(&mut self, tag: u8, value: &SourceDocumentIdentity) {
        let mut nested = Self::new(b"source-document-identity");
        nested.write_str(0x01, value.namespace().as_str());
        nested.write_u64(0x02, value.path().segments().len() as u64);
        for segment in value.path().segments() {
            nested.write_str(0x03, segment.as_str());
        }
        self.write_field(tag, &nested.finish().to_be_bytes());
    }

    fn write_definition_location(&mut self, tag: u8, value: &DefinitionLocation) {
        let mut nested = Self::new(b"definition-location");
        nested.write_source_identity(0x01, value.source());
        nested.write_str(0x02, value.entry().structural_path().as_str());
        nested.write_u64(0x03, u64::from(value.entry().occurrence()));
        self.write_field(tag, &nested.finish().to_be_bytes());
    }
}

fn build_semantic_index(request: &LinkRequest<'_>) -> Result<SemanticIndex, LinkOperationalError> {
    let mut definitions = BTreeMap::<DefinitionIdentity, Vec<DefinitionSnapshot>>::new();
    let mut candidates = BTreeMap::<ScopeDomain, BTreeSet<CatalogKey>>::new();

    for artifact in request.definition_artifacts() {
        for definition in artifact.definitions() {
            let scope = request.scope_mappings().resolve(definition.scope());
            let logical = LogicalMessageIdentity {
                scope: scope.clone(),
                domain: definition.domain(),
                key: definition.key().clone(),
            };
            let identity = DefinitionIdentity {
                logical: logical.clone(),
                locale: definition.locale().clone(),
            };
            let snapshot = DefinitionSnapshot {
                logical,
                locale: definition.locale().clone(),
                message: definition.message().clone(),
                location: DefinitionLocation::new(
                    artifact.source().clone(),
                    definition.source().clone(),
                ),
            };
            definitions.entry(identity).or_default().push(snapshot);
            candidates
                .entry(ScopeDomain {
                    scope,
                    domain: definition.domain(),
                })
                .or_default()
                .insert(definition.key().clone());
        }
    }

    for snapshots in definitions.values_mut() {
        snapshots.sort_by(|left, right| left.location.cmp(&right.location));
    }

    let mut references = Vec::new();
    for artifact in request.reference_artifacts() {
        for (ordinal, reference) in artifact.references().iter().enumerate() {
            let Some(identity) = artifact.record_identity(ordinal) else {
                return Err(LinkOperationalError::InternalInvariant);
            };
            references.push(ReferenceFact {
                identity,
                delivery_unit: artifact.delivery_unit().clone(),
                scope: request.scope_mappings().resolve(reference.scope()),
                domain: reference.domain(),
                selector: reference.selector().clone(),
                reason: reference.reason().cloned(),
                origin: reference.origin().cloned(),
            });
        }
    }
    references.sort_by(|left, right| left.identity.cmp(&right.identity));

    Ok(SemanticIndex {
        definitions,
        candidates,
        references,
    })
}

fn analyze_definitions(index: &SemanticIndex) -> DefinitionAnalysis {
    let mut ambiguities = Vec::new();
    let mut ambiguous_logical = BTreeSet::new();
    let mut unique_definitions =
        BTreeMap::<LogicalMessageIdentity, BTreeMap<Locale, DefinitionSnapshot>>::new();

    for (identity, snapshots) in &index.definitions {
        if snapshots.len() >= 2 {
            ambiguities.push(AmbiguityFact {
                identity: identity.clone(),
                definitions: snapshots
                    .iter()
                    .map(|snapshot| snapshot.location.clone())
                    .collect(),
            });
            ambiguous_logical.insert(identity.logical.clone());
        } else if let Some(snapshot) = snapshots.first() {
            unique_definitions
                .entry(identity.logical.clone())
                .or_default()
                .insert(identity.locale.clone(), snapshot.clone());
        }
    }
    for logical in &ambiguous_logical {
        unique_definitions.remove(logical);
    }

    DefinitionAnalysis {
        ambiguities,
        ambiguous_logical,
        unique_definitions,
    }
}

fn resolve_references(
    request: &LinkRequest<'_>,
    index: &SemanticIndex,
    pattern_budget: &mut PatternWorkBudget,
) -> Result<Vec<ReferenceResolution>, LinkOperationalError> {
    let mut resolutions = Vec::with_capacity(index.references.len());

    for reference in &index.references {
        let unbounded_dynamic = reference.is_unbounded_dynamic();
        let selected = if unbounded_dynamic {
            match request.resolved_policy().dynamic_references() {
                DynamicReferenceMode::Compat => all_candidates(reference, index),
                DynamicReferenceMode::Strict => BTreeSet::new(),
            }
        } else {
            select_reference_candidates(reference, index, pattern_budget)?
        };

        let unresolved = !unbounded_dynamic
            && selected.is_empty()
            && definition_side_closed(request, &reference.scope)?;
        resolutions.push(ReferenceResolution {
            reference: reference.clone(),
            selected,
            unresolved,
        });
    }

    Ok(resolutions)
}

fn select_reference_candidates(
    reference: &ReferenceFact,
    index: &SemanticIndex,
    pattern_budget: &mut PatternWorkBudget,
) -> Result<BTreeSet<LogicalMessageIdentity>, LinkOperationalError> {
    let scope_domain = reference.scope_domain();
    select_candidates(&scope_domain, &reference.selector, index, pattern_budget)
}

fn select_candidates(
    scope_domain: &ScopeDomain,
    selector: &MessageSelector,
    index: &SemanticIndex,
    pattern_budget: &mut PatternWorkBudget,
) -> Result<BTreeSet<LogicalMessageIdentity>, LinkOperationalError> {
    let Some(candidates) = index.candidates.get(scope_domain) else {
        return Ok(BTreeSet::new());
    };

    let mut selected = BTreeSet::new();
    for candidate in candidates {
        let matches = match selector {
            MessageSelector::Exact(key) => key == candidate,
            MessageSelector::Prefix(prefix) => candidate.tokens().starts_with(prefix.tokens()),
            MessageSelector::Pattern(pattern) => {
                let (matches, states) = match_pattern(pattern.tokens(), candidate.tokens())?;
                pattern_budget.charge(states)?;
                matches
            }
            MessageSelector::AllInScope => true,
            MessageSelector::UnboundedDynamic => {
                return Err(LinkOperationalError::InternalInvariant);
            }
        };
        if matches {
            selected.insert(LogicalMessageIdentity {
                scope: scope_domain.scope.clone(),
                domain: scope_domain.domain,
                key: candidate.clone(),
            });
        }
    }
    Ok(selected)
}

fn all_candidates(
    reference: &ReferenceFact,
    index: &SemanticIndex,
) -> BTreeSet<LogicalMessageIdentity> {
    let scope_domain = reference.scope_domain();
    index
        .candidates
        .get(&scope_domain)
        .into_iter()
        .flatten()
        .map(|key| LogicalMessageIdentity {
            scope: scope_domain.scope.clone(),
            domain: scope_domain.domain,
            key: key.clone(),
        })
        .collect()
}

fn build_duplicate_reachability(
    request: &LinkRequest<'_>,
    index: &SemanticIndex,
    ambiguous: &BTreeSet<LogicalMessageIdentity>,
    resolutions: &[ReferenceResolution],
    pattern_budget: &mut PatternWorkBudget,
) -> Result<Reachability, LinkOperationalError> {
    let mut by_delivery_unit = request
        .delivery_graph()
        .nodes()
        .iter()
        .cloned()
        .map(|unit| (unit, BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();

    for resolution in resolutions {
        let Some(messages) = by_delivery_unit.get_mut(&resolution.reference.delivery_unit) else {
            return Err(LinkOperationalError::InternalInvariant);
        };
        extend_unambiguous(messages, &resolution.selected, ambiguous);
    }

    for root in request.resolved_policy().configured_roots() {
        let scope_domain = ScopeDomain {
            scope: root.scope().clone(),
            domain: root.domain(),
        };
        let selected = select_candidates(&scope_domain, root.selector(), index, pattern_budget)?;
        for unit in request.delivery_graph().roots() {
            let Some(messages) = by_delivery_unit.get_mut(unit) else {
                return Err(LinkOperationalError::InternalInvariant);
            };
            extend_unambiguous(messages, &selected, ambiguous);
        }
    }

    let reachable = by_delivery_unit
        .values()
        .flat_map(BTreeSet::iter)
        .cloned()
        .collect();
    let unbounded_scope_domains = resolutions
        .iter()
        .filter(|resolution| resolution.is_unbounded_dynamic())
        .map(|resolution| resolution.reference.scope_domain())
        .collect();
    Ok(Reachability {
        by_delivery_unit,
        reachable,
        unbounded_scope_domains,
    })
}

fn extend_unambiguous(
    messages: &mut BTreeSet<LogicalMessageIdentity>,
    selected: &BTreeSet<LogicalMessageIdentity>,
    ambiguous: &BTreeSet<LogicalMessageIdentity>,
) {
    messages.extend(
        selected
            .iter()
            .filter(|identity| !ambiguous.contains(*identity))
            .cloned(),
    );
}

fn materialize_findings(
    request: &LinkRequest<'_>,
    definitions: &DefinitionAnalysis,
    resolutions: &[ReferenceResolution],
    reachability: &Reachability,
) -> Result<Vec<LinkFinding>, LinkOperationalError> {
    let finding_count = count_findings(request, definitions, resolutions, reachability)?;
    let capacity =
        usize::try_from(finding_count).map_err(|_| LinkOperationalError::InternalInvariant)?;
    let mut findings = Vec::with_capacity(capacity);

    for ambiguity in &definitions.ambiguities {
        findings.push(LinkFinding::new(
            LinkFindingRecord::AmbiguousMessageDefinition(AmbiguousMessageDefinitionFinding::new(
                ambiguity.identity.logical.scope.clone(),
                ambiguity.identity.logical.domain,
                ambiguity.identity.logical.key.clone(),
                ambiguity.identity.locale.clone(),
                ambiguity.definitions.clone(),
            )),
        ));
    }

    for resolution in resolutions
        .iter()
        .filter(|resolution| resolution.is_unresolved())
    {
        findings.push(LinkFinding::new(LinkFindingRecord::UnresolvedMessage(
            UnresolvedMessageFinding::new(
                resolution.reference.identity.clone(),
                resolution.reference.delivery_unit.clone(),
                resolution.reference.scope.clone(),
                resolution.reference.domain,
                resolution.reference.selector.clone(),
                resolution.reference.reason.clone(),
                resolution.reference.origin.clone(),
                request
                    .resolved_policy()
                    .production_locales()
                    .iter()
                    .cloned()
                    .map(ResolutionFailure::fallback_blind)
                    .collect(),
            ),
        )));
    }

    for (logical, definitions_by_locale) in &definitions.unique_definitions {
        if !should_report_unused(request, definitions, reachability, logical)? {
            continue;
        }
        for snapshot in definitions_by_locale.values() {
            findings.push(LinkFinding::new(LinkFindingRecord::UnusedMessage(
                UnusedMessageFinding::new(
                    logical.scope.clone(),
                    logical.domain,
                    logical.key.clone(),
                    snapshot.locale.clone(),
                    snapshot.location.clone(),
                ),
            )));
        }
    }

    for resolution in resolutions
        .iter()
        .filter(|resolution| resolution.is_unbounded_dynamic())
    {
        findings.push(LinkFinding::new(
            LinkFindingRecord::UnboundedDynamicReference(UnboundedDynamicReferenceFinding::new(
                resolution.reference.identity.clone(),
                resolution.reference.delivery_unit.clone(),
                resolution.reference.scope.clone(),
                resolution.reference.domain,
                resolution.reference.reason.clone(),
                resolution.reference.origin.clone(),
                request.resolved_policy().dynamic_references(),
            )),
        ));
    }

    for resolution in resolutions
        .iter()
        .filter(|resolution| resolution.is_wide_selector())
    {
        findings.push(LinkFinding::new(LinkFindingRecord::DegradedAnalysis(
            DegradedAnalysisFinding::WideSelector(WideSelectorDegradation::new(
                resolution.reference.identity.clone(),
                resolution.reference.delivery_unit.clone(),
                resolution.reference.scope.clone(),
                resolution.reference.domain,
                resolution.reference.reason.clone(),
                resolution.reference.origin.clone(),
            )),
        )));
    }

    for completeness in request.resolved_scope_completeness().entries() {
        for (side, value) in completeness_sides(completeness) {
            if let ResolvedInputCompleteness::Partial(contributors) = value {
                findings.push(LinkFinding::new(LinkFindingRecord::DegradedAnalysis(
                    DegradedAnalysisFinding::PartialCompleteness(
                        PartialCompletenessDegradation::new(
                            completeness.scope().clone(),
                            side,
                            contributors.to_vec(),
                        ),
                    ),
                )));
            }
        }
    }

    if findings.len() != capacity {
        return Err(LinkOperationalError::InternalInvariant);
    }
    findings.sort();
    account_finding_bytes(request, &findings)?;
    Ok(findings)
}

fn count_findings(
    request: &LinkRequest<'_>,
    definitions: &DefinitionAnalysis,
    resolutions: &[ReferenceResolution],
    reachability: &Reachability,
) -> Result<u64, LinkOperationalError> {
    let mut count = u64::try_from(definitions.ambiguities.len())
        .map_err(|_| LinkOperationalError::InternalInvariant)?;
    check_finding_count(count, request)?;

    add_count(
        &mut count,
        resolutions
            .iter()
            .filter(|resolution| resolution.is_unresolved())
            .count(),
        request,
    )?;

    for (logical, definitions_by_locale) in &definitions.unique_definitions {
        if !should_report_unused(request, definitions, reachability, logical)? {
            continue;
        }
        add_count(&mut count, definitions_by_locale.len(), request)?;
    }

    add_count(
        &mut count,
        resolutions
            .iter()
            .filter(|resolution| resolution.is_unbounded_dynamic())
            .count(),
        request,
    )?;
    add_count(
        &mut count,
        resolutions
            .iter()
            .filter(|resolution| resolution.is_wide_selector())
            .count(),
        request,
    )?;

    for completeness in request.resolved_scope_completeness().entries() {
        for (_, value) in completeness_sides(completeness) {
            if matches!(value, ResolvedInputCompleteness::Partial(_)) {
                add_count(&mut count, 1, request)?;
            }
        }
    }

    Ok(count)
}

fn should_report_unused(
    request: &LinkRequest<'_>,
    definitions: &DefinitionAnalysis,
    reachability: &Reachability,
    logical: &LogicalMessageIdentity,
) -> Result<bool, LinkOperationalError> {
    Ok(!definitions.ambiguous_logical.contains(logical)
        && !reachability.reachable.contains(logical)
        && !reachability
            .unbounded_scope_domains
            .contains(&logical.scope_domain())
        && both_sides_closed(request, &logical.scope)?)
}

fn completeness_sides(
    completeness: &ResolvedScopeCompleteness,
) -> [(CompletenessSide, &ResolvedInputCompleteness); 2] {
    [
        (CompletenessSide::Definitions, completeness.definitions()),
        (CompletenessSide::References, completeness.references()),
    ]
}

impl LogicalMessageIdentity {
    fn scope_domain(&self) -> ScopeDomain {
        ScopeDomain {
            scope: self.scope.clone(),
            domain: self.domain,
        }
    }
}

fn add_count(
    total: &mut u64,
    count: usize,
    request: &LinkRequest<'_>,
) -> Result<(), LinkOperationalError> {
    let count = u64::try_from(count).map_err(|_| LinkOperationalError::InternalInvariant)?;
    *total = total
        .checked_add(count)
        .ok_or(LinkOperationalError::InternalInvariant)?;
    check_finding_count(*total, request)
}

fn check_finding_count(count: u64, request: &LinkRequest<'_>) -> Result<(), LinkOperationalError> {
    let limit = request
        .limits()
        .effective_limit(LinkLimitCounter::FindingsTotal);
    if count > limit {
        let first_over = limit
            .checked_add(1)
            .ok_or(LinkOperationalError::InternalInvariant)?;
        return Err(limit_error(
            LinkLimitCounter::FindingsTotal,
            limit,
            first_over,
        ));
    }
    Ok(())
}

fn materialize_plans(
    request: &LinkRequest<'_>,
    definitions: &UniqueDefinitions,
    reachability: &DeliveryUnitReachability,
) -> Result<Vec<MessageBundlePlan>, LinkOperationalError> {
    let node_count = u64::try_from(request.delivery_graph().nodes().len())
        .map_err(|_| LinkOperationalError::InternalInvariant)?;
    let locale_count = u64::try_from(request.resolved_policy().production_locales().len())
        .map_err(|_| LinkOperationalError::InternalInvariant)?;
    let plan_count = node_count
        .checked_mul(locale_count)
        .ok_or(LinkOperationalError::InternalInvariant)?;
    preflight_first_over(LinkLimitCounter::BundlePlansTotal, plan_count, request)?;

    let mut resolved_count = 0_u64;
    for unit in request.delivery_graph().nodes() {
        let selected = reachability
            .get(unit)
            .ok_or(LinkOperationalError::InternalInvariant)?;
        for locale in request.resolved_policy().production_locales() {
            for logical in selected {
                if exact_definition(definitions, logical, locale).is_some() {
                    resolved_count = resolved_count
                        .checked_add(1)
                        .ok_or(LinkOperationalError::InternalInvariant)?;
                    preflight_first_over(
                        LinkLimitCounter::ResolvedMessagesTotal,
                        resolved_count,
                        request,
                    )?;
                }
            }
        }
    }

    account_plan_bytes(request, definitions, reachability)?;

    let capacity =
        usize::try_from(plan_count).map_err(|_| LinkOperationalError::InternalInvariant)?;
    let mut plans = Vec::with_capacity(capacity);
    for unit in request.delivery_graph().nodes() {
        let selected = reachability
            .get(unit)
            .ok_or(LinkOperationalError::InternalInvariant)?;
        for locale in request.resolved_policy().production_locales() {
            let messages = selected
                .iter()
                .filter_map(|logical| exact_definition(definitions, logical, locale))
                .map(|snapshot| {
                    ResolvedMessage::new(
                        snapshot.logical.scope.clone(),
                        snapshot.logical.domain,
                        snapshot.logical.key.clone(),
                        snapshot.locale.clone(),
                        snapshot.message.clone(),
                        snapshot.location.clone(),
                    )
                })
                .collect();
            plans.push(MessageBundlePlan::new(
                unit.clone(),
                locale.clone(),
                messages,
            ));
        }
    }
    Ok(plans)
}

fn account_plan_bytes(
    request: &LinkRequest<'_>,
    definitions: &UniqueDefinitions,
    reachability: &DeliveryUnitReachability,
) -> Result<(), LinkOperationalError> {
    let mut budget = SemanticByteBudget::new(LinkLimitCounter::BundlePlanBytesTotal, request);

    for unit in request.delivery_graph().nodes() {
        let selected = reachability
            .get(unit)
            .ok_or(LinkOperationalError::InternalInvariant)?;
        for locale in request.resolved_policy().production_locales() {
            add_delivery_unit_bytes(&mut budget, unit)?;
            budget.add(locale.as_str().len())?;
            for logical in selected {
                let Some(snapshot) = exact_definition(definitions, logical, locale) else {
                    continue;
                };
                add_scope_bytes(&mut budget, &snapshot.logical.scope)?;
                budget.add(snapshot.logical.key.as_str().len())?;
                budget.add(snapshot.locale.as_str().len())?;
                budget.add(snapshot.message.as_str().len())?;
                add_source_bytes(&mut budget, snapshot.location.source())?;
                budget.add(snapshot.location.entry().structural_path().as_str().len())?;
            }
        }
    }
    Ok(())
}

fn exact_definition<'a>(
    definitions: &'a UniqueDefinitions,
    logical: &LogicalMessageIdentity,
    locale: &Locale,
) -> Option<&'a DefinitionSnapshot> {
    definitions
        .get(logical)
        .and_then(|by_locale| by_locale.get(locale))
}

fn definition_side_closed(
    request: &LinkRequest<'_>,
    scope: &ResolvedCatalogScopeId,
) -> Result<bool, LinkOperationalError> {
    let completeness = scope_completeness(request, scope)?;
    Ok(matches!(
        completeness.definitions(),
        ResolvedInputCompleteness::Closed
    ))
}

fn both_sides_closed(
    request: &LinkRequest<'_>,
    scope: &ResolvedCatalogScopeId,
) -> Result<bool, LinkOperationalError> {
    let completeness = scope_completeness(request, scope)?;
    Ok(matches!(
        (completeness.definitions(), completeness.references()),
        (
            ResolvedInputCompleteness::Closed,
            ResolvedInputCompleteness::Closed
        )
    ))
}

fn scope_completeness<'a>(
    request: &'a LinkRequest<'_>,
    scope: &ResolvedCatalogScopeId,
) -> Result<&'a ResolvedScopeCompleteness, LinkOperationalError> {
    let completeness = request
        .resolved_scope_completeness()
        .get(scope)
        .ok_or(LinkOperationalError::InternalInvariant)?;
    Ok(completeness)
}

struct PatternWorkBudget {
    limit: u64,
    total: u64,
}

impl PatternWorkBudget {
    fn new(request: &LinkRequest<'_>) -> Self {
        Self {
            limit: request
                .limits()
                .effective_limit(LinkLimitCounter::PatternMatchStatesTotal),
            total: 0,
        }
    }

    fn charge(&mut self, states: u64) -> Result<(), LinkOperationalError> {
        let attempted = self
            .total
            .checked_add(states)
            .ok_or(LinkOperationalError::InternalInvariant)?;
        if attempted > self.limit {
            return Err(limit_error(
                LinkLimitCounter::PatternMatchStatesTotal,
                self.limit,
                attempted,
            ));
        }
        self.total = attempted;
        Ok(())
    }
}

fn match_pattern(
    pattern: &[PatternToken],
    candidate: &[intlify_contract::CatalogKeyToken],
) -> Result<(bool, u64), LinkOperationalError> {
    let width = pattern
        .len()
        .checked_add(1)
        .ok_or(LinkOperationalError::InternalInvariant)?;
    let height = candidate
        .len()
        .checked_add(1)
        .ok_or(LinkOperationalError::InternalInvariant)?;
    let rectangle = width
        .checked_mul(height)
        .ok_or(LinkOperationalError::InternalInvariant)?;
    let absolute_max = (LinkLimitCounter::SelectorPatternTokens.protocol_ceiling() + 1)
        .checked_mul(LinkLimitCounter::CatalogKeyTokens.protocol_ceiling() + 1)
        .ok_or(LinkOperationalError::InternalInvariant)?;
    let rectangle =
        u64::try_from(rectangle).map_err(|_| LinkOperationalError::InternalInvariant)?;
    if rectangle > absolute_max {
        return Err(LinkOperationalError::InternalInvariant);
    }

    let mut current = vec![false; width];
    current[0] = true;
    let mut states = 0_u64;

    for candidate_position in 0..=candidate.len() {
        let mut next = vec![false; width];
        for pattern_position in 0..=pattern.len() {
            if !current[pattern_position] {
                continue;
            }
            states = states
                .checked_add(1)
                .ok_or(LinkOperationalError::InternalInvariant)?;

            let Some(token) = pattern.get(pattern_position) else {
                continue;
            };
            match token {
                PatternToken::Literal(literal) => {
                    if candidate
                        .get(candidate_position)
                        .is_some_and(|value| value == literal)
                    {
                        next[pattern_position + 1] = true;
                    }
                }
                PatternToken::AnyOne => {
                    if candidate_position < candidate.len() {
                        next[pattern_position + 1] = true;
                    }
                }
                PatternToken::AnyMany => {
                    current[pattern_position + 1] = true;
                    if candidate_position < candidate.len() {
                        next[pattern_position] = true;
                    }
                }
            }
        }

        if candidate_position == candidate.len() {
            if states > rectangle {
                return Err(LinkOperationalError::InternalInvariant);
            }
            return Ok((current[pattern.len()], states));
        }
        current = next;
    }

    Err(LinkOperationalError::InternalInvariant)
}

fn account_finding_bytes(
    request: &LinkRequest<'_>,
    findings: &[LinkFinding],
) -> Result<(), LinkOperationalError> {
    let mut budget = SemanticByteBudget::new(LinkLimitCounter::FindingBytesTotal, request);
    for finding in findings {
        match finding.record() {
            LinkFindingRecord::AmbiguousMessageDefinition(finding) => {
                let subject = finding.subject();
                add_scope_bytes(&mut budget, subject.resolved_scope())?;
                budget.add(subject.key().as_str().len())?;
                budget.add(subject.locale().as_str().len())?;
                for definition in finding.evidence().definitions() {
                    add_definition_location_bytes(&mut budget, definition)?;
                }
            }
            LinkFindingRecord::UnresolvedMessage(finding) => {
                let evidence = finding.evidence();
                add_reference_analysis_bytes(
                    &mut budget,
                    finding.subject().reference(),
                    evidence.delivery_unit(),
                    evidence.resolved_scope(),
                    Some(evidence.selector()),
                    evidence.reason(),
                    evidence.origin(),
                )?;
                for failure in evidence.failures() {
                    budget.add(failure.requested_locale().as_str().len())?;
                    for locale in failure.probed_locales() {
                        budget.add(locale.as_str().len())?;
                    }
                }
            }
            LinkFindingRecord::MissingTranslation(finding) => {
                let subject = finding.subject();
                add_reference_identity_bytes(&mut budget, subject.reference())?;
                budget.add(subject.requested_locale().as_str().len())?;
                budget.add(subject.key().as_str().len())?;
                let evidence = finding.evidence();
                add_delivery_unit_bytes(&mut budget, evidence.delivery_unit())?;
                add_scope_bytes(&mut budget, evidence.resolved_scope())?;
                for locale in evidence.probed_locales() {
                    budget.add(locale.as_str().len())?;
                }
                budget.add(evidence.selected_locale().as_str().len())?;
                add_definition_location_bytes(&mut budget, evidence.definition())?;
                add_optional_origin_bytes(&mut budget, evidence.origin())?;
            }
            LinkFindingRecord::OrphanedTranslation(finding) => {
                let subject = finding.subject();
                add_scope_bytes(&mut budget, subject.resolved_scope())?;
                budget.add(subject.key().as_str().len())?;
                budget.add(subject.locale().as_str().len())?;
                budget.add(finding.evidence().baseline_locale().as_str().len())?;
                add_definition_location_bytes(&mut budget, finding.evidence().definition())?;
            }
            LinkFindingRecord::UnusedMessage(finding) => {
                let subject = finding.subject();
                add_scope_bytes(&mut budget, subject.resolved_scope())?;
                budget.add(subject.key().as_str().len())?;
                budget.add(subject.locale().as_str().len())?;
                add_definition_location_bytes(&mut budget, finding.evidence().definition())?;
            }
            LinkFindingRecord::UnboundedDynamicReference(finding) => {
                let evidence = finding.evidence();
                add_reference_analysis_bytes(
                    &mut budget,
                    finding.subject().reference(),
                    evidence.delivery_unit(),
                    evidence.resolved_scope(),
                    None,
                    evidence.reason(),
                    evidence.origin(),
                )?;
            }
            LinkFindingRecord::DegradedAnalysis(finding) => match finding {
                DegradedAnalysisFinding::WideSelector(finding) => {
                    let evidence = finding.evidence();
                    add_reference_analysis_bytes(
                        &mut budget,
                        finding.subject().reference(),
                        evidence.delivery_unit(),
                        evidence.resolved_scope(),
                        None,
                        evidence.reason(),
                        evidence.origin(),
                    )?;
                }
                DegradedAnalysisFinding::PartialCompleteness(finding) => {
                    add_scope_bytes(&mut budget, finding.subject().resolved_scope())?;
                    for contributor in finding.evidence().contributors() {
                        add_contributor_bytes(&mut budget, contributor)?;
                    }
                }
            },
        }
    }
    Ok(())
}

struct SemanticByteBudget {
    counter: LinkLimitCounter,
    limit: u64,
    total: u64,
}

impl SemanticByteBudget {
    fn new(counter: LinkLimitCounter, request: &LinkRequest<'_>) -> Self {
        Self {
            counter,
            limit: request.limits().effective_limit(counter),
            total: 0,
        }
    }

    fn add(&mut self, bytes: usize) -> Result<(), LinkOperationalError> {
        let bytes = u64::try_from(bytes).map_err(|_| LinkOperationalError::InternalInvariant)?;
        let attempted = self
            .total
            .checked_add(bytes)
            .ok_or(LinkOperationalError::InternalInvariant)?;
        if attempted > self.limit {
            return Err(limit_error(self.counter, self.limit, attempted));
        }
        self.total = attempted;
        Ok(())
    }
}

fn preflight_first_over(
    counter: LinkLimitCounter,
    observed: u64,
    request: &LinkRequest<'_>,
) -> Result<(), LinkOperationalError> {
    let limit = request.limits().effective_limit(counter);
    if observed > limit {
        return Err(limit_error(counter, limit, limit + 1));
    }
    Ok(())
}

fn limit_error(counter: LinkLimitCounter, limit: u64, attempted: u64) -> LinkOperationalError {
    match LinkLimitEvidence::try_new(
        counter,
        LinkLimitSubject::Request,
        limit,
        LinkLimitObservation::Exact(attempted),
    ) {
        Ok(evidence) => LinkOperationalError::Limit(evidence),
        Err(_) => LinkOperationalError::InternalInvariant,
    }
}

fn add_reference_analysis_bytes(
    budget: &mut SemanticByteBudget,
    reference: &ReferenceRecordIdentity,
    delivery_unit: &DeliveryUnitId,
    scope: &ResolvedCatalogScopeId,
    selector: Option<&MessageSelector>,
    reason: Option<&ReasonText>,
    origin: Option<&SourceOrigin>,
) -> Result<(), LinkOperationalError> {
    add_reference_identity_bytes(budget, reference)?;
    add_delivery_unit_bytes(budget, delivery_unit)?;
    add_scope_bytes(budget, scope)?;
    if let Some(selector) = selector {
        add_selector_bytes(budget, selector)?;
    }
    add_optional_reason_bytes(budget, reason)?;
    add_optional_origin_bytes(budget, origin)
}

fn add_scope_bytes(
    budget: &mut SemanticByteBudget,
    scope: &ResolvedCatalogScopeId,
) -> Result<(), LinkOperationalError> {
    budget.add(scope.as_catalog_scope().name().as_str().len())
}

fn add_reference_identity_bytes(
    budget: &mut SemanticByteBudget,
    identity: &ReferenceRecordIdentity,
) -> Result<(), LinkOperationalError> {
    for segment in identity.artifact().segments() {
        budget.add(segment.as_str().len())?;
    }
    Ok(())
}

fn add_delivery_unit_bytes(
    budget: &mut SemanticByteBudget,
    delivery_unit: &DeliveryUnitId,
) -> Result<(), LinkOperationalError> {
    for segment in delivery_unit.segments() {
        budget.add(segment.as_str().len())?;
    }
    Ok(())
}

fn add_source_bytes(
    budget: &mut SemanticByteBudget,
    source: &SourceDocumentIdentity,
) -> Result<(), LinkOperationalError> {
    for segment in source.path().segments() {
        budget.add(segment.as_str().len())?;
    }
    Ok(())
}

fn add_entry_bytes(
    budget: &mut SemanticByteBudget,
    entry: &EntryReference,
) -> Result<(), LinkOperationalError> {
    budget.add(entry.structural_path().as_str().len())
}

fn add_definition_location_bytes(
    budget: &mut SemanticByteBudget,
    location: &DefinitionLocation,
) -> Result<(), LinkOperationalError> {
    add_source_bytes(budget, location.source())?;
    add_entry_bytes(budget, location.entry())
}

fn add_selector_bytes(
    budget: &mut SemanticByteBudget,
    selector: &MessageSelector,
) -> Result<(), LinkOperationalError> {
    match selector {
        MessageSelector::Exact(key) => budget.add(key.as_str().len()),
        MessageSelector::Prefix(prefix) => budget.add(prefix.as_str().len()),
        MessageSelector::Pattern(pattern) => budget.add(pattern.as_str().len()),
        MessageSelector::AllInScope | MessageSelector::UnboundedDynamic => Ok(()),
    }
}

fn add_optional_reason_bytes(
    budget: &mut SemanticByteBudget,
    reason: Option<&ReasonText>,
) -> Result<(), LinkOperationalError> {
    if let Some(reason) = reason {
        budget.add(reason.as_str().len())?;
    }
    Ok(())
}

fn add_optional_origin_bytes(
    budget: &mut SemanticByteBudget,
    origin: Option<&SourceOrigin>,
) -> Result<(), LinkOperationalError> {
    if let Some(origin) = origin {
        add_source_bytes(budget, origin.source())?;
    }
    Ok(())
}

fn add_contributor_bytes(
    budget: &mut SemanticByteBudget,
    contributor: &CompletenessContributor,
) -> Result<(), LinkOperationalError> {
    budget.add(contributor.scope().name().as_str().len())
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        CatalogKey, CatalogKeyDomain, CatalogKeyPattern, CatalogKeyToken, PatternToken,
    };

    use super::match_pattern;

    fn key(value: &str) -> CatalogKey {
        CatalogKey::try_new(CatalogKeyDomain::JsonPointer, value).unwrap()
    }

    fn pattern(value: &str) -> CatalogKeyPattern {
        CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, value).unwrap()
    }

    fn exhaustive_pattern_match(
        pattern: &[PatternToken],
        candidate: &[CatalogKeyToken],
        pattern_position: usize,
        candidate_position: usize,
    ) -> bool {
        let Some(token) = pattern.get(pattern_position) else {
            return candidate_position == candidate.len();
        };
        match token {
            PatternToken::Literal(literal) => {
                candidate
                    .get(candidate_position)
                    .is_some_and(|value| value == literal)
                    && exhaustive_pattern_match(
                        pattern,
                        candidate,
                        pattern_position + 1,
                        candidate_position + 1,
                    )
            }
            PatternToken::AnyOne => {
                candidate_position < candidate.len()
                    && exhaustive_pattern_match(
                        pattern,
                        candidate,
                        pattern_position + 1,
                        candidate_position + 1,
                    )
            }
            PatternToken::AnyMany => {
                exhaustive_pattern_match(
                    pattern,
                    candidate,
                    pattern_position + 1,
                    candidate_position,
                ) || (candidate_position < candidate.len()
                    && exhaustive_pattern_match(
                        pattern,
                        candidate,
                        pattern_position,
                        candidate_position + 1,
                    ))
            }
        }
    }

    #[test]
    fn iterative_pattern_matcher_is_anchored_and_counts_reachable_states() {
        let exact = pattern("/checkout/*");
        let title = key("/checkout/title");
        let nested = key("/checkout/form/title");
        assert_eq!(
            match_pattern(exact.tokens(), title.tokens()).unwrap(),
            (true, 3)
        );
        assert!(!match_pattern(exact.tokens(), nested.tokens()).unwrap().0);

        let descendants = pattern("/checkout/**");
        let root = key("/checkout");
        assert!(
            match_pattern(descendants.tokens(), root.tokens())
                .unwrap()
                .0
        );
        assert!(
            match_pattern(descendants.tokens(), nested.tokens())
                .unwrap()
                .0
        );
    }

    #[test]
    fn iterative_pattern_matcher_accepts_the_derived_token_boundaries() {
        let pattern_text = format!(
            "/{}",
            (0..513)
                .map(|index| if index % 2 == 0 { "**" } else { "*" })
                .collect::<Vec<_>>()
                .join("/")
        );
        let key_text = format!("/{}", vec!["value"; 256].join("/"));
        let largest_matchable_pattern = pattern(&pattern_text);
        let deepest_key = key(&key_text);

        let (matches, states) =
            match_pattern(largest_matchable_pattern.tokens(), deepest_key.tokens()).unwrap();

        assert!(matches);
        assert!(states <= 132_098);
    }

    #[test]
    fn iterative_pattern_matcher_matches_exhaustive_small_cases() {
        let pattern_symbols = ["a", "b", "*", "**"];
        let candidate_symbols = ["a", "b"];

        for pattern_len in 1_usize..=3 {
            for mut encoded_pattern in 0..pattern_symbols.len().pow(pattern_len as u32) {
                let mut pattern_parts = Vec::with_capacity(pattern_len);
                for _ in 0..pattern_len {
                    pattern_parts.push(pattern_symbols[encoded_pattern % pattern_symbols.len()]);
                    encoded_pattern /= pattern_symbols.len();
                }
                if pattern_parts.windows(2).any(|pair| pair == ["**", "**"]) {
                    continue;
                }
                let checked_pattern = pattern(&format!("/{}", pattern_parts.join("/")));

                for candidate_len in 0_usize..=3 {
                    for mut encoded_candidate in
                        0..candidate_symbols.len().pow(candidate_len as u32)
                    {
                        let mut candidate_parts = Vec::with_capacity(candidate_len);
                        for _ in 0..candidate_len {
                            candidate_parts.push(
                                candidate_symbols[encoded_candidate % candidate_symbols.len()],
                            );
                            encoded_candidate /= candidate_symbols.len();
                        }
                        let candidate_text = if candidate_parts.is_empty() {
                            String::new()
                        } else {
                            format!("/{}", candidate_parts.join("/"))
                        };
                        let checked_candidate = key(&candidate_text);

                        let expected = exhaustive_pattern_match(
                            checked_pattern.tokens(),
                            checked_candidate.tokens(),
                            0,
                            0,
                        );
                        let actual =
                            match_pattern(checked_pattern.tokens(), checked_candidate.tokens())
                                .unwrap()
                                .0;

                        assert_eq!(
                            actual,
                            expected,
                            "pattern {} against candidate {}",
                            checked_pattern.as_str(),
                            checked_candidate.as_str()
                        );
                    }
                }
            }
        }
    }
}
