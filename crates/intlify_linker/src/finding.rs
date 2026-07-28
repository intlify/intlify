// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Closed, typed semantic finding model and canonical ordering.
//!
//! Every finding owns one coherent subject/evidence pair. Kind, blocking
//! disposition, and resolved scope are derived from that record so reporter or
//! lint policy cannot create a contradictory semantic result.

use std::cmp::Ordering;

use intlify_contract::{
    CatalogKey, CatalogKeyDomain, DeliveryUnitId, Locale, MessageSelector, ReasonText,
    ReferenceRecordIdentity, SourceOrigin,
};

use crate::{
    CompletenessContributor, CompletenessSide, DefinitionLocation, DynamicReferenceMode,
    PartialReason, ResolvedCatalogScopeId,
};

/// Subject of one ambiguous-definition collision.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AmbiguousMessageDefinitionSubject {
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
    locale: Locale,
}

impl AmbiguousMessageDefinitionSubject {
    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the exact canonical catalog key.
    #[must_use]
    pub const fn key(&self) -> &CatalogKey {
        &self.key
    }

    /// Return the exact colliding definition locale.
    #[must_use]
    pub const fn locale(&self) -> &Locale {
        &self.locale
    }
}

/// Complete canonical locations colliding at one definition identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AmbiguousMessageDefinitionEvidence {
    definitions: Box<[DefinitionLocation]>,
}

impl AmbiguousMessageDefinitionEvidence {
    /// Return every colliding location in source-then-entry order.
    #[must_use]
    pub const fn definitions(&self) -> &[DefinitionLocation] {
        &self.definitions
    }
}

/// One always-blocking ambiguous-definition finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AmbiguousMessageDefinitionFinding {
    subject: AmbiguousMessageDefinitionSubject,
    evidence: AmbiguousMessageDefinitionEvidence,
}

impl AmbiguousMessageDefinitionFinding {
    pub(crate) fn new(
        resolved_scope: ResolvedCatalogScopeId,
        domain: CatalogKeyDomain,
        key: CatalogKey,
        locale: Locale,
        definitions: Vec<DefinitionLocation>,
    ) -> Self {
        Self {
            subject: AmbiguousMessageDefinitionSubject {
                resolved_scope,
                domain,
                key,
                locale,
            },
            evidence: AmbiguousMessageDefinitionEvidence {
                definitions: definitions.into_boxed_slice(),
            },
        }
    }

    /// Return the exact collision identity.
    #[must_use]
    pub const fn subject(&self) -> &AmbiguousMessageDefinitionSubject {
        &self.subject
    }

    /// Return complete colliding-definition evidence.
    #[must_use]
    pub const fn evidence(&self) -> &AmbiguousMessageDefinitionEvidence {
        &self.evidence
    }
}

/// One requested locale and the exact locales probed without success.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResolutionFailure {
    requested_locale: Locale,
    probed_locales: Box<[Locale]>,
}

impl ResolutionFailure {
    pub(crate) fn fallback_blind(locale: Locale) -> Self {
        Self {
            probed_locales: vec![locale.clone()].into_boxed_slice(),
            requested_locale: locale,
        }
    }

    /// Return the requested production locale.
    #[must_use]
    pub const fn requested_locale(&self) -> &Locale {
        &self.requested_locale
    }

    /// Return the complete non-empty locale probe sequence.
    #[must_use]
    pub const fn probed_locales(&self) -> &[Locale] {
        &self.probed_locales
    }
}

/// Reference-record identity of one unresolved observation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnresolvedMessageSubject {
    reference: ReferenceRecordIdentity,
}

impl UnresolvedMessageSubject {
    /// Return the exact artifact-and-ordinal reference identity.
    #[must_use]
    pub const fn reference(&self) -> &ReferenceRecordIdentity {
        &self.reference
    }
}

/// Complete lookup evidence for one unresolved reference record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnresolvedMessageEvidence {
    delivery_unit: DeliveryUnitId,
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    selector: MessageSelector,
    reason: Option<ReasonText>,
    origin: Option<SourceOrigin>,
    failures: Box<[ResolutionFailure]>,
}

impl UnresolvedMessageEvidence {
    /// Return the owning delivery unit.
    #[must_use]
    pub const fn delivery_unit(&self) -> &DeliveryUnitId {
        &self.delivery_unit
    }

    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the exact checked selector.
    #[must_use]
    pub const fn selector(&self) -> &MessageSelector {
        &self.selector
    }

    /// Return optional selector rationale.
    #[must_use]
    pub const fn reason(&self) -> Option<&ReasonText> {
        self.reason.as_ref()
    }

    /// Return optional source-location evidence.
    #[must_use]
    pub const fn origin(&self) -> Option<&SourceOrigin> {
        self.origin.as_ref()
    }

    /// Return every failing requested-locale probe in canonical locale order.
    #[must_use]
    pub const fn failures(&self) -> &[ResolutionFailure] {
        &self.failures
    }
}

/// One always-blocking unresolved-reference finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnresolvedMessageFinding {
    subject: UnresolvedMessageSubject,
    evidence: UnresolvedMessageEvidence,
}

impl UnresolvedMessageFinding {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        reference: ReferenceRecordIdentity,
        delivery_unit: DeliveryUnitId,
        resolved_scope: ResolvedCatalogScopeId,
        domain: CatalogKeyDomain,
        selector: MessageSelector,
        reason: Option<ReasonText>,
        origin: Option<SourceOrigin>,
        failures: Vec<ResolutionFailure>,
    ) -> Self {
        Self {
            subject: UnresolvedMessageSubject { reference },
            evidence: UnresolvedMessageEvidence {
                delivery_unit,
                resolved_scope,
                domain,
                selector,
                reason,
                origin,
                failures: failures.into_boxed_slice(),
            },
        }
    }

    /// Return the reference-record subject.
    #[must_use]
    pub const fn subject(&self) -> &UnresolvedMessageSubject {
        &self.subject
    }

    /// Return complete resolution evidence.
    #[must_use]
    pub const fn evidence(&self) -> &UnresolvedMessageEvidence {
        &self.evidence
    }
}

/// One reference, requested locale, and resolved key with a coverage gap.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MissingTranslationSubject {
    reference: ReferenceRecordIdentity,
    requested_locale: Locale,
    key: CatalogKey,
}

impl MissingTranslationSubject {
    /// Return the exact reference-record identity.
    #[must_use]
    pub const fn reference(&self) -> &ReferenceRecordIdentity {
        &self.reference
    }

    /// Return the requested production locale.
    #[must_use]
    pub const fn requested_locale(&self) -> &Locale {
        &self.requested_locale
    }

    /// Return the resolved canonical key.
    #[must_use]
    pub const fn key(&self) -> &CatalogKey {
        &self.key
    }
}

/// Selected fallback definition evidence for one translation gap.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MissingTranslationEvidence {
    delivery_unit: DeliveryUnitId,
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    probed_locales: Box<[Locale]>,
    selected_locale: Locale,
    definition: DefinitionLocation,
    origin: Option<SourceOrigin>,
}

impl MissingTranslationEvidence {
    /// Return the owning delivery unit.
    #[must_use]
    pub const fn delivery_unit(&self) -> &DeliveryUnitId {
        &self.delivery_unit
    }

    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the exact probed locale sequence.
    #[must_use]
    pub const fn probed_locales(&self) -> &[Locale] {
        &self.probed_locales
    }

    /// Return the locale that supplied the selected definition.
    #[must_use]
    pub const fn selected_locale(&self) -> &Locale {
        &self.selected_locale
    }

    /// Return the selected definition location.
    #[must_use]
    pub const fn definition(&self) -> &DefinitionLocation {
        &self.definition
    }

    /// Return optional originating reference evidence.
    #[must_use]
    pub const fn origin(&self) -> Option<&SourceOrigin> {
        self.origin.as_ref()
    }
}

/// One non-blocking fallback coverage finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MissingTranslationFinding {
    subject: MissingTranslationSubject,
    evidence: MissingTranslationEvidence,
}

impl MissingTranslationFinding {
    /// Return the exact reference-locale-key subject.
    #[must_use]
    pub const fn subject(&self) -> &MissingTranslationSubject {
        &self.subject
    }

    /// Return selected fallback-definition evidence.
    #[must_use]
    pub const fn evidence(&self) -> &MissingTranslationEvidence {
        &self.evidence
    }
}

/// One non-baseline locale definition absent from a coverage baseline.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrphanedTranslationSubject {
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
    locale: Locale,
}

impl OrphanedTranslationSubject {
    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the canonical catalog key.
    #[must_use]
    pub const fn key(&self) -> &CatalogKey {
        &self.key
    }

    /// Return the affected definition locale.
    #[must_use]
    pub const fn locale(&self) -> &Locale {
        &self.locale
    }
}

/// Coverage-baseline evidence for one orphaned definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrphanedTranslationEvidence {
    baseline_locale: Locale,
    definition: DefinitionLocation,
}

impl OrphanedTranslationEvidence {
    /// Return the configured baseline locale.
    #[must_use]
    pub const fn baseline_locale(&self) -> &Locale {
        &self.baseline_locale
    }

    /// Return the affected definition location.
    #[must_use]
    pub const fn definition(&self) -> &DefinitionLocation {
        &self.definition
    }
}

/// One non-blocking orphaned-translation finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct OrphanedTranslationFinding {
    subject: OrphanedTranslationSubject,
    evidence: OrphanedTranslationEvidence,
}

impl OrphanedTranslationFinding {
    /// Return the exact definition identity.
    #[must_use]
    pub const fn subject(&self) -> &OrphanedTranslationSubject {
        &self.subject
    }

    /// Return baseline and location evidence.
    #[must_use]
    pub const fn evidence(&self) -> &OrphanedTranslationEvidence {
        &self.evidence
    }
}

/// One locale-bearing definition whose logical key is unreachable.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnusedMessageSubject {
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
    locale: Locale,
}

impl UnusedMessageSubject {
    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the canonical catalog key.
    #[must_use]
    pub const fn key(&self) -> &CatalogKey {
        &self.key
    }

    /// Return the exact definition locale.
    #[must_use]
    pub const fn locale(&self) -> &Locale {
        &self.locale
    }
}

/// Definition-location evidence for one unused entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnusedMessageEvidence {
    definition: DefinitionLocation,
}

impl UnusedMessageEvidence {
    /// Return the exact unreachable definition location.
    #[must_use]
    pub const fn definition(&self) -> &DefinitionLocation {
        &self.definition
    }
}

/// One non-blocking unreachable-definition finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnusedMessageFinding {
    subject: UnusedMessageSubject,
    evidence: UnusedMessageEvidence,
}

impl UnusedMessageFinding {
    pub(crate) const fn new(
        resolved_scope: ResolvedCatalogScopeId,
        domain: CatalogKeyDomain,
        key: CatalogKey,
        locale: Locale,
        definition: DefinitionLocation,
    ) -> Self {
        Self {
            subject: UnusedMessageSubject {
                resolved_scope,
                domain,
                key,
                locale,
            },
            evidence: UnusedMessageEvidence { definition },
        }
    }

    /// Return the exact locale-bearing definition identity.
    #[must_use]
    pub const fn subject(&self) -> &UnusedMessageSubject {
        &self.subject
    }

    /// Return the affected definition location.
    #[must_use]
    pub const fn evidence(&self) -> &UnusedMessageEvidence {
        &self.evidence
    }
}

/// Reference-record identity of one unbounded dynamic lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnboundedDynamicReferenceSubject {
    reference: ReferenceRecordIdentity,
}

impl UnboundedDynamicReferenceSubject {
    /// Return the exact artifact-and-ordinal reference identity.
    #[must_use]
    pub const fn reference(&self) -> &ReferenceRecordIdentity {
        &self.reference
    }
}

/// Policy and source evidence for one unbounded dynamic lookup.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnboundedDynamicReferenceEvidence {
    delivery_unit: DeliveryUnitId,
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    reason: Option<ReasonText>,
    origin: Option<SourceOrigin>,
    dynamic_mode: DynamicReferenceMode,
}

impl UnboundedDynamicReferenceEvidence {
    /// Return the owning delivery unit.
    #[must_use]
    pub const fn delivery_unit(&self) -> &DeliveryUnitId {
        &self.delivery_unit
    }

    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return optional producer-supplied rationale.
    #[must_use]
    pub const fn reason(&self) -> Option<&ReasonText> {
        self.reason.as_ref()
    }

    /// Return optional source-location evidence.
    #[must_use]
    pub const fn origin(&self) -> Option<&SourceOrigin> {
        self.origin.as_ref()
    }

    /// Return the exact checked dynamic-reference mode.
    #[must_use]
    pub const fn dynamic_mode(&self) -> DynamicReferenceMode {
        self.dynamic_mode
    }
}

/// One unbounded-dynamic finding with policy-derived disposition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct UnboundedDynamicReferenceFinding {
    subject: UnboundedDynamicReferenceSubject,
    evidence: UnboundedDynamicReferenceEvidence,
}

impl UnboundedDynamicReferenceFinding {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        reference: ReferenceRecordIdentity,
        delivery_unit: DeliveryUnitId,
        resolved_scope: ResolvedCatalogScopeId,
        domain: CatalogKeyDomain,
        reason: Option<ReasonText>,
        origin: Option<SourceOrigin>,
        dynamic_mode: DynamicReferenceMode,
    ) -> Self {
        Self {
            subject: UnboundedDynamicReferenceSubject { reference },
            evidence: UnboundedDynamicReferenceEvidence {
                delivery_unit,
                resolved_scope,
                domain,
                reason,
                origin,
                dynamic_mode,
            },
        }
    }

    /// Return the exact reference-record subject.
    #[must_use]
    pub const fn subject(&self) -> &UnboundedDynamicReferenceSubject {
        &self.subject
    }

    /// Return dynamic policy and source evidence.
    #[must_use]
    pub const fn evidence(&self) -> &UnboundedDynamicReferenceEvidence {
        &self.evidence
    }
}

/// Reference-record identity of one intentional all-in-scope selector.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WideSelectorSubject {
    reference: ReferenceRecordIdentity,
}

impl WideSelectorSubject {
    /// Return the exact artifact-and-ordinal reference identity.
    #[must_use]
    pub const fn reference(&self) -> &ReferenceRecordIdentity {
        &self.reference
    }
}

/// Source evidence for one intentional all-in-scope selector.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WideSelectorEvidence {
    delivery_unit: DeliveryUnitId,
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    reason: Option<ReasonText>,
    origin: Option<SourceOrigin>,
}

impl WideSelectorEvidence {
    /// Return the owning delivery unit.
    #[must_use]
    pub const fn delivery_unit(&self) -> &DeliveryUnitId {
        &self.delivery_unit
    }

    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return optional producer-supplied rationale.
    #[must_use]
    pub const fn reason(&self) -> Option<&ReasonText> {
        self.reason.as_ref()
    }

    /// Return optional source-location evidence.
    #[must_use]
    pub const fn origin(&self) -> Option<&SourceOrigin> {
        self.origin.as_ref()
    }
}

/// One non-blocking all-in-scope degradation record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WideSelectorDegradation {
    subject: WideSelectorSubject,
    evidence: WideSelectorEvidence,
}

impl WideSelectorDegradation {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        reference: ReferenceRecordIdentity,
        delivery_unit: DeliveryUnitId,
        resolved_scope: ResolvedCatalogScopeId,
        domain: CatalogKeyDomain,
        reason: Option<ReasonText>,
        origin: Option<SourceOrigin>,
    ) -> Self {
        Self {
            subject: WideSelectorSubject { reference },
            evidence: WideSelectorEvidence {
                delivery_unit,
                resolved_scope,
                domain,
                reason,
                origin,
            },
        }
    }

    /// Return the exact reference-record subject.
    #[must_use]
    pub const fn subject(&self) -> &WideSelectorSubject {
        &self.subject
    }

    /// Return selector and source evidence.
    #[must_use]
    pub const fn evidence(&self) -> &WideSelectorEvidence {
        &self.evidence
    }
}

/// One resolved scope side whose input world is not closed.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PartialCompletenessSubject {
    resolved_scope: ResolvedCatalogScopeId,
    side: CompletenessSide,
}

impl PartialCompletenessSubject {
    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the partial input side.
    #[must_use]
    pub const fn side(&self) -> CompletenessSide {
        self.side
    }
}

/// Complete original-scope evidence contributing to partial completeness.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PartialCompletenessEvidence {
    contributors: Box<[CompletenessContributor]>,
}

impl PartialCompletenessEvidence {
    /// Return every canonical original-scope contributor.
    #[must_use]
    pub const fn contributors(&self) -> &[CompletenessContributor] {
        &self.contributors
    }
}

/// One blocking partial-completeness degradation record.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PartialCompletenessDegradation {
    subject: PartialCompletenessSubject,
    evidence: PartialCompletenessEvidence,
}

impl PartialCompletenessDegradation {
    pub(crate) fn new(
        resolved_scope: ResolvedCatalogScopeId,
        side: CompletenessSide,
        contributors: Vec<CompletenessContributor>,
    ) -> Self {
        Self {
            subject: PartialCompletenessSubject {
                resolved_scope,
                side,
            },
            evidence: PartialCompletenessEvidence {
                contributors: contributors.into_boxed_slice(),
            },
        }
    }

    /// Return the resolved-scope and side subject.
    #[must_use]
    pub const fn subject(&self) -> &PartialCompletenessSubject {
        &self.subject
    }

    /// Return complete partial-input contributors.
    #[must_use]
    pub const fn evidence(&self) -> &PartialCompletenessEvidence {
        &self.evidence
    }
}

/// Closed degradation subkind union in explicit canonical order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DegradedAnalysisFinding {
    /// One intentional `AllInScope` reference record.
    WideSelector(WideSelectorDegradation),
    /// One partial resolved-scope input side.
    PartialCompleteness(PartialCompletenessDegradation),
}

impl DegradedAnalysisFinding {
    const fn precedence(&self) -> u8 {
        match self {
            Self::WideSelector(_) => 0,
            Self::PartialCompleteness(_) => 1,
        }
    }

    const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        match self {
            Self::WideSelector(finding) => finding.evidence().resolved_scope(),
            Self::PartialCompleteness(finding) => finding.subject().resolved_scope(),
        }
    }

    const fn blocking(&self) -> bool {
        matches!(self, Self::PartialCompleteness(_))
    }
}

impl PartialOrd for DegradedAnalysisFinding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for DegradedAnalysisFinding {
    fn cmp(&self, other: &Self) -> Ordering {
        self.precedence()
            .cmp(&other.precedence())
            .then_with(|| match (self, other) {
                (Self::WideSelector(left), Self::WideSelector(right)) => {
                    compare_wide_selector(left, right)
                }
                (Self::PartialCompleteness(left), Self::PartialCompleteness(right)) => {
                    compare_partial_completeness(left, right)
                }
                (Self::WideSelector(_), Self::PartialCompleteness(_))
                | (Self::PartialCompleteness(_), Self::WideSelector(_)) => Ordering::Equal,
            })
    }
}

/// Closed public finding-kind vocabulary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LinkFindingKind {
    /// Two or more exact definitions collide.
    AmbiguousMessageDefinition,
    /// A bounded reference selects no definition.
    UnresolvedMessage,
    /// Fallback supplied a definition for a missing requested locale.
    MissingTranslation,
    /// A non-baseline locale defines a key absent from the baseline.
    OrphanedTranslation,
    /// A locale-bearing definition is unreachable.
    UnusedMessage,
    /// A producer could not statically bound one lookup.
    UnboundedDynamicReference,
    /// An intentional wide selector or partial input weakened analysis.
    DegradedAnalysis,
}

impl LinkFindingKind {
    /// Return the compatibility-stable canonical kind precedence.
    #[must_use]
    pub const fn precedence(self) -> u8 {
        match self {
            Self::AmbiguousMessageDefinition => 0,
            Self::UnresolvedMessage => 1,
            Self::MissingTranslation => 2,
            Self::OrphanedTranslation => 3,
            Self::UnusedMessage => 4,
            Self::UnboundedDynamicReference => 5,
            Self::DegradedAnalysis => 6,
        }
    }
}

impl PartialOrd for LinkFindingKind {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LinkFindingKind {
    fn cmp(&self, other: &Self) -> Ordering {
        self.precedence().cmp(&other.precedence())
    }
}

/// Closed typed finding-record union.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LinkFindingRecord {
    /// Exact definition collision.
    AmbiguousMessageDefinition(AmbiguousMessageDefinitionFinding),
    /// Bounded reference with no matching definition.
    UnresolvedMessage(UnresolvedMessageFinding),
    /// Requested-locale coverage gap resolved through fallback.
    MissingTranslation(MissingTranslationFinding),
    /// Definition absent from an explicit coverage baseline.
    OrphanedTranslation(OrphanedTranslationFinding),
    /// Unreachable locale-bearing definition.
    UnusedMessage(UnusedMessageFinding),
    /// Unbounded producer observation.
    UnboundedDynamicReference(UnboundedDynamicReferenceFinding),
    /// Intentional or input-derived analysis degradation.
    DegradedAnalysis(DegradedAnalysisFinding),
}

impl LinkFindingRecord {
    const fn kind(&self) -> LinkFindingKind {
        match self {
            Self::AmbiguousMessageDefinition(_) => LinkFindingKind::AmbiguousMessageDefinition,
            Self::UnresolvedMessage(_) => LinkFindingKind::UnresolvedMessage,
            Self::MissingTranslation(_) => LinkFindingKind::MissingTranslation,
            Self::OrphanedTranslation(_) => LinkFindingKind::OrphanedTranslation,
            Self::UnusedMessage(_) => LinkFindingKind::UnusedMessage,
            Self::UnboundedDynamicReference(_) => LinkFindingKind::UnboundedDynamicReference,
            Self::DegradedAnalysis(_) => LinkFindingKind::DegradedAnalysis,
        }
    }

    const fn blocking(&self) -> bool {
        match self {
            Self::AmbiguousMessageDefinition(_) | Self::UnresolvedMessage(_) => true,
            Self::MissingTranslation(_) | Self::OrphanedTranslation(_) | Self::UnusedMessage(_) => {
                false
            }
            Self::UnboundedDynamicReference(finding) => {
                matches!(
                    finding.evidence().dynamic_mode(),
                    DynamicReferenceMode::Strict
                )
            }
            Self::DegradedAnalysis(finding) => finding.blocking(),
        }
    }

    const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        match self {
            Self::AmbiguousMessageDefinition(finding) => finding.subject().resolved_scope(),
            Self::UnresolvedMessage(finding) => finding.evidence().resolved_scope(),
            Self::MissingTranslation(finding) => finding.evidence().resolved_scope(),
            Self::OrphanedTranslation(finding) => finding.subject().resolved_scope(),
            Self::UnusedMessage(finding) => finding.subject().resolved_scope(),
            Self::UnboundedDynamicReference(finding) => finding.evidence().resolved_scope(),
            Self::DegradedAnalysis(finding) => finding.resolved_scope(),
        }
    }
}

/// One immutable typed semantic finding.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LinkFinding {
    record: LinkFindingRecord,
}

impl LinkFinding {
    pub(crate) const fn new(record: LinkFindingRecord) -> Self {
        Self { record }
    }

    /// Return the derived closed finding kind.
    #[must_use]
    pub const fn kind(&self) -> LinkFindingKind {
        self.record.kind()
    }

    /// Return the semantic generation-blocking disposition.
    #[must_use]
    pub const fn blocking(&self) -> bool {
        self.record.blocking()
    }

    /// Return the one post-mapping scope carried by this finding.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        self.record.resolved_scope()
    }

    /// Return the exact typed subject/evidence record.
    #[must_use]
    pub const fn record(&self) -> &LinkFindingRecord {
        &self.record
    }
}

impl PartialOrd for LinkFinding {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for LinkFinding {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind()
            .cmp(&other.kind())
            .then_with(|| match (&self.record, &other.record) {
                (
                    LinkFindingRecord::AmbiguousMessageDefinition(left),
                    LinkFindingRecord::AmbiguousMessageDefinition(right),
                ) => compare_ambiguous(left, right),
                (
                    LinkFindingRecord::UnresolvedMessage(left),
                    LinkFindingRecord::UnresolvedMessage(right),
                ) => compare_unresolved(left, right),
                (
                    LinkFindingRecord::MissingTranslation(left),
                    LinkFindingRecord::MissingTranslation(right),
                ) => compare_missing_translation(left, right),
                (
                    LinkFindingRecord::OrphanedTranslation(left),
                    LinkFindingRecord::OrphanedTranslation(right),
                ) => compare_orphaned_translation(left, right),
                (
                    LinkFindingRecord::UnusedMessage(left),
                    LinkFindingRecord::UnusedMessage(right),
                ) => compare_unused(left, right),
                (
                    LinkFindingRecord::UnboundedDynamicReference(left),
                    LinkFindingRecord::UnboundedDynamicReference(right),
                ) => compare_unbounded_dynamic(left, right),
                (
                    LinkFindingRecord::DegradedAnalysis(left),
                    LinkFindingRecord::DegradedAnalysis(right),
                ) => left.cmp(right),
                _ => Ordering::Equal,
            })
    }
}

fn compare_ambiguous(
    left: &AmbiguousMessageDefinitionFinding,
    right: &AmbiguousMessageDefinitionFinding,
) -> Ordering {
    left.subject
        .resolved_scope
        .cmp(&right.subject.resolved_scope)
        .then_with(|| left.subject.domain.cmp(&right.subject.domain))
        .then_with(|| left.subject.key.cmp(&right.subject.key))
        .then_with(|| left.subject.locale.cmp(&right.subject.locale))
        .then_with(|| left.evidence.definitions.cmp(&right.evidence.definitions))
}

fn compare_unresolved(
    left: &UnresolvedMessageFinding,
    right: &UnresolvedMessageFinding,
) -> Ordering {
    left.subject
        .reference
        .cmp(&right.subject.reference)
        .then_with(|| {
            left.evidence
                .delivery_unit
                .cmp(&right.evidence.delivery_unit)
        })
        .then_with(|| {
            left.evidence
                .resolved_scope
                .cmp(&right.evidence.resolved_scope)
        })
        .then_with(|| left.evidence.domain.cmp(&right.evidence.domain))
        .then_with(|| left.evidence.selector.cmp(&right.evidence.selector))
        .then_with(|| left.evidence.reason.cmp(&right.evidence.reason))
        .then_with(|| left.evidence.origin.cmp(&right.evidence.origin))
        .then_with(|| {
            compare_slices_by(
                &left.evidence.failures,
                &right.evidence.failures,
                compare_resolution_failure,
            )
        })
}

fn compare_resolution_failure(left: &ResolutionFailure, right: &ResolutionFailure) -> Ordering {
    left.requested_locale
        .cmp(&right.requested_locale)
        .then_with(|| left.probed_locales.cmp(&right.probed_locales))
}

fn compare_missing_translation(
    left: &MissingTranslationFinding,
    right: &MissingTranslationFinding,
) -> Ordering {
    left.subject
        .reference
        .cmp(&right.subject.reference)
        .then_with(|| {
            left.subject
                .requested_locale
                .cmp(&right.subject.requested_locale)
        })
        .then_with(|| left.subject.key.cmp(&right.subject.key))
        .then_with(|| {
            left.evidence
                .delivery_unit
                .cmp(&right.evidence.delivery_unit)
        })
        .then_with(|| {
            left.evidence
                .resolved_scope
                .cmp(&right.evidence.resolved_scope)
        })
        .then_with(|| left.evidence.domain.cmp(&right.evidence.domain))
        .then_with(|| {
            left.evidence
                .probed_locales
                .cmp(&right.evidence.probed_locales)
        })
        .then_with(|| {
            left.evidence
                .selected_locale
                .cmp(&right.evidence.selected_locale)
        })
        .then_with(|| left.evidence.definition.cmp(&right.evidence.definition))
        .then_with(|| left.evidence.origin.cmp(&right.evidence.origin))
}

fn compare_orphaned_translation(
    left: &OrphanedTranslationFinding,
    right: &OrphanedTranslationFinding,
) -> Ordering {
    left.subject
        .resolved_scope
        .cmp(&right.subject.resolved_scope)
        .then_with(|| left.subject.domain.cmp(&right.subject.domain))
        .then_with(|| left.subject.key.cmp(&right.subject.key))
        .then_with(|| left.subject.locale.cmp(&right.subject.locale))
        .then_with(|| {
            left.evidence
                .baseline_locale
                .cmp(&right.evidence.baseline_locale)
        })
        .then_with(|| left.evidence.definition.cmp(&right.evidence.definition))
}

fn compare_unused(left: &UnusedMessageFinding, right: &UnusedMessageFinding) -> Ordering {
    left.subject
        .resolved_scope
        .cmp(&right.subject.resolved_scope)
        .then_with(|| left.subject.domain.cmp(&right.subject.domain))
        .then_with(|| left.subject.key.cmp(&right.subject.key))
        .then_with(|| left.subject.locale.cmp(&right.subject.locale))
        .then_with(|| left.evidence.definition.cmp(&right.evidence.definition))
}

fn compare_unbounded_dynamic(
    left: &UnboundedDynamicReferenceFinding,
    right: &UnboundedDynamicReferenceFinding,
) -> Ordering {
    left.subject
        .reference
        .cmp(&right.subject.reference)
        .then_with(|| {
            left.evidence
                .delivery_unit
                .cmp(&right.evidence.delivery_unit)
        })
        .then_with(|| {
            left.evidence
                .resolved_scope
                .cmp(&right.evidence.resolved_scope)
        })
        .then_with(|| left.evidence.domain.cmp(&right.evidence.domain))
        .then_with(|| left.evidence.reason.cmp(&right.evidence.reason))
        .then_with(|| left.evidence.origin.cmp(&right.evidence.origin))
        .then_with(|| {
            dynamic_mode_precedence(left.evidence.dynamic_mode)
                .cmp(&dynamic_mode_precedence(right.evidence.dynamic_mode))
        })
}

fn compare_wide_selector(
    left: &WideSelectorDegradation,
    right: &WideSelectorDegradation,
) -> Ordering {
    left.subject
        .reference
        .cmp(&right.subject.reference)
        .then_with(|| {
            left.evidence
                .delivery_unit
                .cmp(&right.evidence.delivery_unit)
        })
        .then_with(|| {
            left.evidence
                .resolved_scope
                .cmp(&right.evidence.resolved_scope)
        })
        .then_with(|| left.evidence.domain.cmp(&right.evidence.domain))
        .then_with(|| left.evidence.reason.cmp(&right.evidence.reason))
        .then_with(|| left.evidence.origin.cmp(&right.evidence.origin))
}

fn compare_partial_completeness(
    left: &PartialCompletenessDegradation,
    right: &PartialCompletenessDegradation,
) -> Ordering {
    left.subject
        .resolved_scope
        .cmp(&right.subject.resolved_scope)
        .then_with(|| {
            completeness_side_precedence(left.subject.side)
                .cmp(&completeness_side_precedence(right.subject.side))
        })
        .then_with(|| {
            compare_slices_by(
                &left.evidence.contributors,
                &right.evidence.contributors,
                compare_completeness_contributor,
            )
        })
}

fn compare_completeness_contributor(
    left: &CompletenessContributor,
    right: &CompletenessContributor,
) -> Ordering {
    left.scope().cmp(right.scope()).then_with(|| {
        partial_reason_precedence(left.reason()).cmp(&partial_reason_precedence(right.reason()))
    })
}

const fn dynamic_mode_precedence(mode: DynamicReferenceMode) -> u8 {
    match mode {
        DynamicReferenceMode::Compat => 0,
        DynamicReferenceMode::Strict => 1,
    }
}

const fn completeness_side_precedence(side: CompletenessSide) -> u8 {
    match side {
        CompletenessSide::Definitions => 0,
        CompletenessSide::References => 1,
    }
}

const fn partial_reason_precedence(reason: PartialReason) -> u8 {
    match reason {
        PartialReason::OpenEditorWorld => 0,
        PartialReason::SourceOmitted => 1,
        PartialReason::SourceFailed => 2,
        PartialReason::ProducerOmitted => 3,
        PartialReason::ProducerFailed => 4,
        PartialReason::ExternalArtifactUnverified => 5,
    }
}

fn compare_slices_by<T>(
    left: &[T],
    right: &[T],
    mut compare: impl FnMut(&T, &T) -> Ordering,
) -> Ordering {
    for (left, right) in left.iter().zip(right) {
        let ordering = compare(left, right);
        if ordering != Ordering::Equal {
            return ordering;
        }
    }
    left.len().cmp(&right.len())
}
