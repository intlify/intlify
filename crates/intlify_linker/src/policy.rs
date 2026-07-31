// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked link policy and post-mapping policy canonicalization.
//!
//! This module owns production locales, configured roots, coverage baselines,
//! dynamic-reference behavior, and duplicate placement. It deliberately
//! contains no fallback member, raw configuration parser, exporter options, or
//! environment lookup.

use std::error::Error;
use std::fmt;

use intlify_contract::{
    CatalogKeyDomain, CatalogScopeId, LinkLimitCounter, LinkLimitSubject, LinkLimits, Locale,
    MessageSelector, ReasonText,
};

use crate::error::ConfiguredRootIdentity;
use crate::scope::{ResolvedCatalogScopeId, ScopeMappingTable};
use crate::validation::{check_exact, check_first_over, usize_count};
use crate::{InvalidRequestError, LinkOperationalError};

/// Treatment of an unbounded dynamic reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DynamicReferenceMode {
    /// Report non-blocking degradation and conservatively retain the scope-domain set.
    Compat,
    /// Report blocking degradation and withhold bundle plans.
    Strict,
}

/// Shared-message placement policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PlacementPolicy {
    /// Retain one selected message in every referencing delivery unit.
    Duplicate,
}

/// Invalid composition of one configured-root record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfiguredRootConstructionError {
    /// `UnboundedDynamic` is producer evidence and cannot be configured as a root.
    UnboundedDynamicSelector,
    /// The selector payload belongs to another catalog-key domain.
    SelectorDomainMismatch,
}

impl fmt::Display for ConfiguredRootConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "invalid configured root: {self:?}")
    }
}

impl Error for ConfiguredRootConstructionError {}

/// One exceptional reachability declaration before scope mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConfiguredRoot {
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    selector: MessageSelector,
    reason: Option<ReasonText>,
}

impl ConfiguredRoot {
    /// Construct one checked configured root.
    pub fn try_new(
        scope: CatalogScopeId,
        domain: CatalogKeyDomain,
        selector: MessageSelector,
        reason: Option<ReasonText>,
    ) -> Result<Self, ConfiguredRootConstructionError> {
        if matches!(selector, MessageSelector::UnboundedDynamic) {
            return Err(ConfiguredRootConstructionError::UnboundedDynamicSelector);
        }
        if !selector.is_for_domain(domain) {
            return Err(ConfiguredRootConstructionError::SelectorDomainMismatch);
        }
        Ok(Self {
            scope,
            domain,
            selector,
            reason,
        })
    }

    /// Return the declared catalog scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the checked finite selector.
    #[must_use]
    pub const fn selector(&self) -> &MessageSelector {
        &self.selector
    }

    /// Return optional declaration evidence.
    #[must_use]
    pub const fn reason(&self) -> Option<&ReasonText> {
        self.reason.as_ref()
    }

    pub(crate) fn identity(&self) -> ConfiguredRootIdentity {
        ConfiguredRootIdentity {
            scope: self.scope.clone(),
            domain: self.domain,
            selector: self.selector.clone(),
        }
    }
}

/// One declared-scope locale selected as the coverage and typed-key baseline.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CoverageBaseline {
    scope: CatalogScopeId,
    locale: Locale,
}

impl CoverageBaseline {
    /// Compose one baseline from independently checked scope and locale values.
    #[must_use]
    pub const fn new(scope: CatalogScopeId, locale: Locale) -> Self {
        Self { scope, locale }
    }

    /// Return the declared catalog scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the selected production locale.
    #[must_use]
    pub const fn locale(&self) -> &Locale {
        &self.locale
    }
}

/// Immutable, canonical policy supplied to one link request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinkPolicy {
    production_locales: Box<[Locale]>,
    configured_roots: Box<[ConfiguredRoot]>,
    coverage_baselines: Box<[CoverageBaseline]>,
    dynamic_references: DynamicReferenceMode,
    placement: PlacementPolicy,
}

impl LinkPolicy {
    /// Validate occurrence-preserving policy inputs and retain canonical sets.
    pub fn try_new(
        production_locales: Vec<Locale>,
        configured_roots: Vec<ConfiguredRoot>,
        coverage_baselines: Vec<CoverageBaseline>,
        dynamic_references: DynamicReferenceMode,
        placement: PlacementPolicy,
        limits: &LinkLimits,
    ) -> Result<Self, LinkOperationalError> {
        validate_policy_counts(&production_locales, &configured_roots, limits)?;
        validate_production_locale_limits(&production_locales, limits)?;

        if production_locales.is_empty() {
            return Err(InvalidRequestError::EmptyProductionLocales.into());
        }

        let mut production_locales = production_locales;
        production_locales.sort();
        if let Some(locale) = first_equal_adjacent(&production_locales) {
            return Err(InvalidRequestError::DuplicateProductionLocale(locale.clone()).into());
        }

        validate_configured_root_limits(&configured_roots, limits)?;
        let mut configured_roots = configured_roots;
        configured_roots.sort_by_key(ConfiguredRoot::identity);
        if let Some(root) = configured_roots
            .windows(2)
            .find(|pair| pair[0].identity() == pair[1].identity())
        {
            return Err(InvalidRequestError::DuplicateConfiguredRoot(root[0].identity()).into());
        }

        validate_coverage_baseline_limits(&coverage_baselines, limits)?;
        let mut coverage_baselines = coverage_baselines;
        coverage_baselines.sort();
        if let Some(baseline) = coverage_baselines
            .iter()
            .find(|baseline| production_locales.binary_search(baseline.locale()).is_err())
        {
            return Err(InvalidRequestError::CoverageBaselineLocaleNotProduction {
                scope: baseline.scope().clone(),
                locale: baseline.locale().clone(),
            }
            .into());
        }
        if let Some(pair) = coverage_baselines
            .windows(2)
            .find(|pair| pair[0].scope() == pair[1].scope())
        {
            return Err(
                InvalidRequestError::DuplicateCoverageBaseline(pair[0].scope().clone()).into(),
            );
        }

        Ok(Self {
            production_locales: production_locales.into_boxed_slice(),
            configured_roots: configured_roots.into_boxed_slice(),
            coverage_baselines: coverage_baselines.into_boxed_slice(),
            dynamic_references,
            placement,
        })
    }

    /// Return the canonical non-empty production-locale set.
    #[must_use]
    pub const fn production_locales(&self) -> &[Locale] {
        &self.production_locales
    }

    /// Return configured roots in canonical identity order.
    #[must_use]
    pub const fn configured_roots(&self) -> &[ConfiguredRoot] {
        &self.configured_roots
    }

    /// Return declared coverage baselines in canonical scope order.
    #[must_use]
    pub const fn coverage_baselines(&self) -> &[CoverageBaseline] {
        &self.coverage_baselines
    }

    /// Return the exact dynamic-reference mode.
    #[must_use]
    pub const fn dynamic_references(&self) -> DynamicReferenceMode {
        self.dynamic_references
    }

    /// Return the exact placement policy.
    #[must_use]
    pub const fn placement(&self) -> PlacementPolicy {
        self.placement
    }

    pub(crate) fn revalidate(&self, limits: &LinkLimits) -> Result<(), LinkOperationalError> {
        validate_policy_counts(&self.production_locales, &self.configured_roots, limits)?;
        validate_production_locale_limits(&self.production_locales, limits)?;
        validate_configured_root_limits(&self.configured_roots, limits)?;
        validate_coverage_baseline_limits(&self.coverage_baselines, limits)
    }
}

/// One configured root after uniform one-hop scope mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedConfiguredRoot {
    scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    selector: MessageSelector,
    reason: Option<ReasonText>,
}

impl ResolvedConfiguredRoot {
    /// Return the resolved semantic scope.
    #[must_use]
    pub const fn scope(&self) -> &ResolvedCatalogScopeId {
        &self.scope
    }

    /// Return the exact catalog-key domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the checked selector.
    #[must_use]
    pub const fn selector(&self) -> &MessageSelector {
        &self.selector
    }

    /// Return optional declaration evidence.
    #[must_use]
    pub const fn reason(&self) -> Option<&ReasonText> {
        self.reason.as_ref()
    }

    fn identity(&self) -> ConfiguredRootIdentity {
        ConfiguredRootIdentity {
            scope: self.scope.as_catalog_scope().clone(),
            domain: self.domain,
            selector: self.selector.clone(),
        }
    }
}

/// One coverage baseline after uniform one-hop scope mapping.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResolvedCoverageBaseline {
    scope: ResolvedCatalogScopeId,
    locale: Locale,
}

impl ResolvedCoverageBaseline {
    /// Return the resolved semantic scope.
    #[must_use]
    pub const fn scope(&self) -> &ResolvedCatalogScopeId {
        &self.scope
    }

    /// Return the selected production locale.
    #[must_use]
    pub const fn locale(&self) -> &Locale {
        &self.locale
    }
}

/// Canonical policy after scope mapping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLinkPolicy {
    production_locales: Box<[Locale]>,
    configured_roots: Box<[ResolvedConfiguredRoot]>,
    coverage_baselines: Box<[ResolvedCoverageBaseline]>,
    dynamic_references: DynamicReferenceMode,
    placement: PlacementPolicy,
}

impl ResolvedLinkPolicy {
    pub(crate) fn resolve(
        policy: &LinkPolicy,
        mappings: &ScopeMappingTable,
    ) -> Result<Self, LinkOperationalError> {
        let mut submitted = policy
            .configured_roots()
            .iter()
            .map(|root| ResolvedConfiguredRoot {
                scope: mappings.resolve(root.scope()),
                domain: root.domain(),
                selector: root.selector().clone(),
                reason: root.reason().cloned(),
            })
            .collect::<Vec<_>>();
        submitted.sort_by_key(ResolvedConfiguredRoot::identity);

        let mut configured_roots: Vec<ResolvedConfiguredRoot> = Vec::with_capacity(submitted.len());
        for root in submitted {
            if let Some(previous) = configured_roots.last() {
                if previous.identity() == root.identity() {
                    if previous.reason != root.reason {
                        return Err(InvalidRequestError::ResolvedConfiguredRootConflict(
                            root.identity(),
                        )
                        .into());
                    }
                    continue;
                }
            }
            configured_roots.push(root);
        }

        let mut submitted = policy
            .coverage_baselines()
            .iter()
            .map(|baseline| ResolvedCoverageBaseline {
                scope: mappings.resolve(baseline.scope()),
                locale: baseline.locale().clone(),
            })
            .collect::<Vec<_>>();
        submitted.sort_by(|left, right| {
            left.scope
                .cmp(&right.scope)
                .then_with(|| left.locale.cmp(&right.locale))
        });

        let mut coverage_baselines =
            Vec::<ResolvedCoverageBaseline>::with_capacity(submitted.len());
        for baseline in submitted {
            if let Some(previous) = coverage_baselines.last() {
                if previous.scope == baseline.scope {
                    if previous.locale != baseline.locale {
                        return Err(InvalidRequestError::ResolvedCoverageBaselineConflict {
                            scope: baseline.scope,
                            first: previous.locale.clone(),
                            second: baseline.locale,
                        }
                        .into());
                    }
                    continue;
                }
            }
            coverage_baselines.push(baseline);
        }

        Ok(Self {
            production_locales: policy.production_locales.to_vec().into_boxed_slice(),
            configured_roots: configured_roots.into_boxed_slice(),
            coverage_baselines: coverage_baselines.into_boxed_slice(),
            dynamic_references: policy.dynamic_references,
            placement: policy.placement,
        })
    }

    /// Return canonical production locales.
    #[must_use]
    pub const fn production_locales(&self) -> &[Locale] {
        &self.production_locales
    }

    /// Return canonical post-mapping roots.
    #[must_use]
    pub const fn configured_roots(&self) -> &[ResolvedConfiguredRoot] {
        &self.configured_roots
    }

    /// Return canonical post-mapping coverage baselines.
    #[must_use]
    pub const fn coverage_baselines(&self) -> &[ResolvedCoverageBaseline] {
        &self.coverage_baselines
    }

    /// Return the exact dynamic-reference mode.
    #[must_use]
    pub const fn dynamic_references(&self) -> DynamicReferenceMode {
        self.dynamic_references
    }

    /// Return the exact placement policy.
    #[must_use]
    pub const fn placement(&self) -> PlacementPolicy {
        self.placement
    }
}

fn validate_policy_counts(
    production_locales: &[Locale],
    configured_roots: &[ConfiguredRoot],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let subject = LinkLimitSubject::ResolvedPolicy;
    check_first_over(
        LinkLimitCounter::ProductionLocales,
        subject.clone(),
        usize_count(production_locales.len()),
        limits,
    )?;
    check_first_over(
        LinkLimitCounter::ConfiguredRoots,
        subject.clone(),
        usize_count(configured_roots.len()),
        limits,
    )
}

fn validate_production_locale_limits(
    production_locales: &[Locale],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let subject = LinkLimitSubject::ResolvedPolicy;
    for locale in production_locales {
        check_first_over(
            LinkLimitCounter::LocaleBytes,
            subject.clone(),
            usize_count(locale.as_str().len()),
            limits,
        )?;
    }
    Ok(())
}

fn validate_configured_root_limits(
    configured_roots: &[ConfiguredRoot],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let subject = LinkLimitSubject::ResolvedPolicy;
    for root in configured_roots {
        check_first_over(
            LinkLimitCounter::CatalogScopeNameBytes,
            subject.clone(),
            usize_count(root.scope().name().as_str().len()),
            limits,
        )?;
    }
    Ok(())
}

fn validate_coverage_baseline_limits(
    coverage_baselines: &[CoverageBaseline],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let subject = LinkLimitSubject::ResolvedPolicy;
    check_first_over(
        LinkLimitCounter::CoverageBaselines,
        subject.clone(),
        usize_count(coverage_baselines.len()),
        limits,
    )?;

    let mut canonical = coverage_baselines.iter().collect::<Vec<_>>();
    canonical.sort();
    for baseline in &canonical {
        check_first_over(
            LinkLimitCounter::CatalogScopeNameBytes,
            subject.clone(),
            usize_count(baseline.scope().name().as_str().len()),
            limits,
        )?;
    }
    for baseline in &canonical {
        check_first_over(
            LinkLimitCounter::LocaleBytes,
            subject.clone(),
            usize_count(baseline.locale().as_str().len()),
            limits,
        )?;
    }

    let mut total = 0_u64;
    for baseline in canonical {
        total = total
            .checked_add(usize_count(baseline.scope().name().as_str().len()))
            .and_then(|value| value.checked_add(usize_count(baseline.locale().as_str().len())))
            .ok_or(LinkOperationalError::InternalInvariant)?;
        check_exact(
            LinkLimitCounter::CoverageBaselineBytesTotal,
            subject.clone(),
            total,
            limits,
        )?;
    }
    Ok(())
}

fn first_equal_adjacent<T: PartialEq>(values: &[T]) -> Option<&T> {
    values
        .windows(2)
        .find(|pair| pair[0] == pair[1])
        .map(|pair| &pair[0])
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        ArtifactNamespace, CatalogKey, CatalogKeyDomain, CatalogScopeId, CatalogScopeName,
        LinkLimitCounter, LinkLimitObservation, LinkLimits, Locale, MessageSelector,
    };

    use super::{
        ConfiguredRoot, ConfiguredRootConstructionError, CoverageBaseline, DynamicReferenceMode,
        LinkPolicy, PlacementPolicy,
    };
    use crate::{
        InvalidRequestError, LinkOperationalError, ResolvedLinkPolicy, ScopeMapping,
        ScopeMappingTable,
    };

    fn scope(name: &str) -> CatalogScopeId {
        CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new(name).unwrap(),
        )
    }

    fn root(name: &str, key: &str) -> ConfiguredRoot {
        ConfiguredRoot::try_new(
            scope(name),
            CatalogKeyDomain::JsonPointer,
            MessageSelector::Exact(
                CatalogKey::try_new(CatalogKeyDomain::JsonPointer, key).unwrap(),
            ),
            None,
        )
        .unwrap()
    }

    fn baseline(name: &str, locale: &str) -> CoverageBaseline {
        CoverageBaseline::new(scope(name), Locale::try_new(locale).unwrap())
    }

    #[test]
    fn policy_canonicalizes_sets_without_fallback_state() {
        let policy = LinkPolicy::try_new(
            vec![
                Locale::try_new("ja").unwrap(),
                Locale::try_new("en").unwrap(),
            ],
            vec![root("z", "/b"), root("a", "/a")],
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap();

        assert_eq!(
            policy
                .production_locales()
                .iter()
                .map(Locale::as_str)
                .collect::<Vec<_>>(),
            ["en", "ja"]
        );
        assert_eq!(policy.configured_roots()[0].scope().name().as_str(), "a");
    }

    #[test]
    fn policy_rejects_empty_and_duplicate_semantic_sets() {
        let empty = LinkPolicy::try_new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap_err();
        assert_eq!(
            empty,
            LinkOperationalError::InvalidRequest(InvalidRequestError::EmptyProductionLocales)
        );

        let duplicate = LinkPolicy::try_new(
            vec![
                Locale::try_new("en").unwrap(),
                Locale::try_new("en").unwrap(),
            ],
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Strict,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap_err();
        assert!(matches!(
            duplicate,
            LinkOperationalError::InvalidRequest(InvalidRequestError::DuplicateProductionLocale(_))
        ));

        let duplicate_root = root("app", "/a");
        assert!(matches!(
            LinkPolicy::try_new(
                vec![Locale::try_new("en").unwrap()],
                vec![duplicate_root.clone(), duplicate_root],
                Vec::new(),
                DynamicReferenceMode::Compat,
                PlacementPolicy::Duplicate,
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::DuplicateConfiguredRoot(_)
            ))
        ));
    }

    #[test]
    fn policy_limit_preflights_precede_semantic_validation() {
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ProductionLocales, 1)
            .unwrap();
        let error = LinkPolicy::try_new(
            vec![
                Locale::try_new("en").unwrap(),
                Locale::try_new("en").unwrap(),
            ],
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &limits,
        )
        .unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::ProductionLocales);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(2));
    }

    #[test]
    fn locale_semantics_precede_configured_root_scope_limits() {
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogScopeNameBytes, 1)
            .unwrap();
        let error = LinkPolicy::try_new(
            vec![
                Locale::try_new("en").unwrap(),
                Locale::try_new("en").unwrap(),
            ],
            vec![root("app", "/a")],
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &limits,
        )
        .unwrap_err();
        assert!(matches!(
            error,
            LinkOperationalError::InvalidRequest(InvalidRequestError::DuplicateProductionLocale(_))
        ));
    }

    #[test]
    fn configured_roots_reject_producer_only_and_cross_domain_selectors() {
        assert_eq!(
            ConfiguredRoot::try_new(
                scope("app"),
                CatalogKeyDomain::JsonPointer,
                MessageSelector::UnboundedDynamic,
                None,
            ),
            Err(ConfiguredRootConstructionError::UnboundedDynamicSelector)
        );
        let yaml_key = CatalogKey::try_new(CatalogKeyDomain::YamlTypedPath, "").unwrap();
        assert_eq!(
            ConfiguredRoot::try_new(
                scope("app"),
                CatalogKeyDomain::JsonPointer,
                MessageSelector::Exact(yaml_key),
                None,
            ),
            Err(ConfiguredRootConstructionError::SelectorDomainMismatch)
        );
    }

    #[test]
    fn policy_canonicalizes_and_validates_declared_coverage_baselines() {
        let policy = LinkPolicy::try_new(
            vec![
                Locale::try_new("ja").unwrap(),
                Locale::try_new("en").unwrap(),
            ],
            Vec::new(),
            vec![baseline("vendor", "en"), baseline("app", "ja")],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap();
        assert_eq!(
            policy
                .coverage_baselines()
                .iter()
                .map(|value| (value.scope().name().as_str(), value.locale().as_str()))
                .collect::<Vec<_>>(),
            [("app", "ja"), ("vendor", "en")]
        );

        let duplicate = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            vec![baseline("app", "en"), baseline("app", "en")],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap_err();
        assert_eq!(
            duplicate,
            LinkOperationalError::InvalidRequest(InvalidRequestError::DuplicateCoverageBaseline(
                scope("app")
            ))
        );

        let outside_production = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            vec![baseline("app", "ja")],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap_err();
        assert_eq!(
            outside_production,
            LinkOperationalError::InvalidRequest(
                InvalidRequestError::CoverageBaselineLocaleNotProduction {
                    scope: scope("app"),
                    locale: Locale::try_new("ja").unwrap(),
                }
            )
        );
    }

    #[test]
    fn coverage_baseline_limits_precede_semantic_validation() {
        let count_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CoverageBaselines, 1)
            .unwrap();
        let error = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            vec![baseline("app", "ja"), baseline("app", "ja")],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &count_limits,
        )
        .unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected count limit evidence");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::CoverageBaselines);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(2));

        let byte_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CoverageBaselineBytesTotal, 4)
            .unwrap();
        let error = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            vec![baseline("app", "en")],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &byte_limits,
        )
        .unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected aggregate byte limit evidence");
        };
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::CoverageBaselineBytesTotal
        );
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(5));
    }

    #[test]
    fn exact_coverage_baseline_limits_do_not_change_policy_identity() {
        let inputs = || vec![baseline("app", "en")];
        let defaults = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            inputs(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap();
        let exact_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CoverageBaselines, 1)
            .unwrap()
            .try_with_limit(LinkLimitCounter::CoverageBaselineBytesTotal, 5)
            .unwrap();
        let exact = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            inputs(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &exact_limits,
        )
        .unwrap();

        assert_eq!(defaults, exact);
        assert!(exact.revalidate(&exact_limits).is_ok());
    }

    #[test]
    fn resolved_coverage_baselines_merge_equal_locales_and_reject_conflicts() {
        let app = scope("app");
        let vendor = scope("vendor");
        let shared = scope("shared");
        let declared = vec![app.clone(), vendor.clone(), shared.clone()];
        let mappings = ScopeMappingTable::try_new(
            &declared,
            vec![
                ScopeMapping::new(app.clone(), shared.clone()),
                ScopeMapping::new(vendor.clone(), shared.clone()),
            ],
            &LinkLimits::default(),
        )
        .unwrap();

        let equal = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            vec![
                CoverageBaseline::new(app.clone(), Locale::try_new("en").unwrap()),
                CoverageBaseline::new(vendor.clone(), Locale::try_new("en").unwrap()),
            ],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap();
        let equal = ResolvedLinkPolicy::resolve(&equal, &mappings).unwrap();
        assert_eq!(equal.coverage_baselines().len(), 1);
        assert_eq!(
            equal.coverage_baselines()[0].scope().as_catalog_scope(),
            &shared
        );
        assert_eq!(equal.coverage_baselines()[0].locale().as_str(), "en");

        let conflict = LinkPolicy::try_new(
            vec![
                Locale::try_new("ja").unwrap(),
                Locale::try_new("en").unwrap(),
            ],
            Vec::new(),
            vec![
                CoverageBaseline::new(app, Locale::try_new("ja").unwrap()),
                CoverageBaseline::new(vendor, Locale::try_new("en").unwrap()),
            ],
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap();
        assert_eq!(
            ResolvedLinkPolicy::resolve(&conflict, &mappings).unwrap_err(),
            LinkOperationalError::InvalidRequest(
                InvalidRequestError::ResolvedCoverageBaselineConflict {
                    scope: mappings.resolve(&shared),
                    first: Locale::try_new("en").unwrap(),
                    second: Locale::try_new("ja").unwrap(),
                }
            )
        );
    }
}
