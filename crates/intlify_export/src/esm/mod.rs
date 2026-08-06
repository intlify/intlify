// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Built-in data-only ESM and TypeScript-accessor representation.
//!
//! This module owns checked immutable ESM options, built-in batch capability
//! preflight, deterministic logical artifact assembly, and the one common
//! [`PlatformExporter`] implementation. It does not select targets, read raw
//! configuration, perform runtime formatting, or register filesystem output.

mod path;
mod source;

use std::collections::BTreeSet;
use std::error::Error;
use std::fmt;

use intlify_contract::{DeliveryUnitId, Locale};
use intlify_linker::{LinkPolicy, LocaleFallback, MessageBundlePlan};

use crate::writer::ExportPayloadBudget;
use crate::{
    ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
    ExportArtifactPath, ExportArtifactRelationship, ExportArtifactRelationshipKind,
    ExportArtifactSet, ExportError, ExportMediaType, InternalInvariantEvidence,
    InternalInvariantViolation, InvalidOutputEvidence, InvalidOutputViolation, PlatformExporter,
    UnsupportedBatchEvidence, UnsupportedBatchFeature, UnsupportedBatchLocation,
    ValidatedExportBatch, ValidatedTypedOutput,
};

const ESM_MODULE_KIND: &str = "dev.intlify/esm-module";
const LOADER_MAP_KIND: &str = "dev.intlify/loader-map";
const TYPESCRIPT_ACCESSOR_KIND: &str = "dev.intlify/typescript-accessor";
const JAVASCRIPT_MEDIA_TYPE: &str = "text/javascript";
const TYPESCRIPT_MEDIA_TYPE: &str = "text/typescript";
const FORMAT_VERSION: ExportArtifactFormatVersion = ExportArtifactFormatVersion::DRAFT_V0_1;

/// Canonical production-locale subset loaded statically by the generated map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EagerLocales(Box<[Locale]>);

impl EagerLocales {
    /// Return eager locales in exact canonical locale order.
    #[must_use]
    pub const fn locales(&self) -> &[Locale] {
        &self.0
    }
}

/// Immutable built-in exporter snapshot derived from one checked link policy.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EsmExporterOptions {
    production_locales: Box<[Locale]>,
    fallbacks: Box<[LocaleFallback]>,
    eager_locales: EagerLocales,
}

impl EsmExporterOptions {
    /// Copy and revalidate the policy-owned locale semantics for one exporter.
    pub fn try_new(
        policy: &LinkPolicy,
        mut eager_locales: Vec<Locale>,
    ) -> Result<Self, EsmExporterOptionsError> {
        validate_policy_snapshot(policy)?;

        eager_locales.sort();
        if let Some(locale) = eager_locales
            .windows(2)
            .find_map(|pair| (pair[0] == pair[1]).then(|| pair[0].clone()))
        {
            return Err(EsmExporterOptionsError::DuplicateEagerLocale(locale));
        }
        if let Some(locale) = eager_locales
            .iter()
            .find(|locale| policy.production_locales().binary_search(locale).is_err())
        {
            return Err(EsmExporterOptionsError::EagerLocaleNotProduction(
                (*locale).clone(),
            ));
        }

        Ok(Self {
            production_locales: policy.production_locales().to_vec().into_boxed_slice(),
            fallbacks: policy.fallbacks().to_vec().into_boxed_slice(),
            eager_locales: EagerLocales(eager_locales.into_boxed_slice()),
        })
    }

    /// Return the exact canonical non-empty production-locale set.
    #[must_use]
    pub const fn production_locales(&self) -> &[Locale] {
        &self.production_locales
    }

    /// Return fallback sources in canonical source order and target priority.
    #[must_use]
    pub const fn fallbacks(&self) -> &[LocaleFallback] {
        &self.fallbacks
    }

    /// Return the canonical eager-locale subset.
    #[must_use]
    pub const fn eager_locales(&self) -> &EagerLocales {
        &self.eager_locales
    }

    fn fallback_targets(&self, locale: &Locale) -> &[Locale] {
        self.fallbacks
            .binary_search_by(|fallback| fallback.source().cmp(locale))
            .ok()
            .map_or(&[], |index| self.fallbacks[index].targets())
    }
}

/// Failure while constructing checked exporter options before export begins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EsmExporterOptionsError {
    /// The supplied checked policy contradicts its canonical locale contract.
    InvalidPolicySnapshot,
    /// The submitted eager set repeats one exact locale.
    DuplicateEagerLocale(Locale),
    /// One eager locale is absent from the production-locale set.
    EagerLocaleNotProduction(Locale),
}

impl fmt::Display for EsmExporterOptionsError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicySnapshot => {
                formatter.write_str("invalid ESM exporter policy snapshot")
            }
            Self::DuplicateEagerLocale(locale) => {
                write!(formatter, "duplicate eager locale: {}", locale.as_str())
            }
            Self::EagerLocaleNotProduction(locale) => write!(
                formatter,
                "eager locale is not a production locale: {}",
                locale.as_str()
            ),
        }
    }
}

impl Error for EsmExporterOptionsError {}

/// Built-in immutable data-only ESM exporter.
#[derive(Debug)]
pub struct EsmExporter {
    options: EsmExporterOptions,
}

impl EsmExporter {
    /// Construct one exporter instance from already checked options.
    #[must_use]
    pub const fn new(options: EsmExporterOptions) -> Self {
        Self { options }
    }

    /// Return the exact immutable option snapshot used by this instance.
    #[must_use]
    pub const fn options(&self) -> &EsmExporterOptions {
        &self.options
    }
}

impl PlatformExporter for EsmExporter {
    fn export(&self, batch: &ValidatedExportBatch<'_>) -> Result<ExportArtifactSet, ExportError> {
        preflight_batch_contract(batch)?;
        preflight_builtin_batch(&self.options, batch)?;

        let locale_assets = build_locale_assets(&self.options, batch.plans())?;
        let accessor_assets = build_accessor_assets(batch.typed_output())?;
        let loader_path = path::loader_path()?;
        let mut payload_budget = ExportPayloadBudget::new();

        let esm_kind = checked_kind(ESM_MODULE_KIND)?;
        let loader_kind = checked_kind(LOADER_MAP_KIND)?;
        let accessor_kind = checked_kind(TYPESCRIPT_ACCESSOR_KIND)?;
        let javascript_media = checked_media_type(JAVASCRIPT_MEDIA_TYPE)?;
        let typescript_media = checked_media_type(TYPESCRIPT_MEDIA_TYPE)?;

        let expected_count = locale_assets
            .len()
            .checked_add(1)
            .and_then(|count| count.checked_add(accessor_assets.len()))
            .ok_or_else(artifact_assembly_error)?;
        let mut artifacts = Vec::with_capacity(expected_count);

        for asset in &locale_assets {
            artifacts.push(ExportArtifact::new(
                asset.path.clone(),
                esm_kind.clone(),
                FORMAT_VERSION,
                source::render_locale_module(&mut payload_budget, asset.plan)?,
                ExportArtifactMetadata::try_new(Some(javascript_media.clone()), Vec::new())?,
            ));
        }

        let loader_relationships = locale_assets
            .iter()
            .map(|asset| {
                ExportArtifactRelationship::new(asset.relationship_kind(), asset.path.clone())
            })
            .collect();
        artifacts.push(ExportArtifact::new(
            loader_path.clone(),
            loader_kind,
            FORMAT_VERSION,
            source::render_loader_module(&mut payload_budget, &self.options, &locale_assets)?,
            ExportArtifactMetadata::try_new(Some(javascript_media), loader_relationships)?,
        ));

        for asset in &accessor_assets {
            artifacts.push(ExportArtifact::new(
                asset.path.clone(),
                accessor_kind.clone(),
                FORMAT_VERSION,
                source::render_accessor_module(&mut payload_budget, asset.typed_output)?,
                ExportArtifactMetadata::try_new(Some(typescript_media.clone()), Vec::new())?,
            ));
        }

        validate_artifact_assembly(&artifacts, &locale_assets, &loader_path, &accessor_assets)?;
        ExportArtifactSet::try_new(artifacts)
    }
}

#[derive(Debug)]
pub(super) struct LocaleAsset<'a> {
    plan: &'a MessageBundlePlan,
    path: ExportArtifactPath,
    eager_alias: Option<usize>,
}

impl LocaleAsset<'_> {
    fn relationship_kind(&self) -> ExportArtifactRelationshipKind {
        if self.eager_alias.is_some() {
            ExportArtifactRelationshipKind::EagerLoad
        } else {
            ExportArtifactRelationshipKind::LazyLoad
        }
    }
}

#[derive(Debug)]
struct AccessorAsset<'a> {
    typed_output: &'a ValidatedTypedOutput<'a>,
    path: ExportArtifactPath,
}

fn validate_policy_snapshot(policy: &LinkPolicy) -> Result<(), EsmExporterOptionsError> {
    let production = policy.production_locales();
    if production.is_empty() || production.windows(2).any(|pair| pair[0] >= pair[1]) {
        return Err(EsmExporterOptionsError::InvalidPolicySnapshot);
    }

    let mut previous_source = None;
    for fallback in policy.fallbacks() {
        if previous_source.is_some_and(|previous| previous >= fallback.source())
            || production.binary_search(fallback.source()).is_err()
            || fallback.targets().is_empty()
        {
            return Err(EsmExporterOptionsError::InvalidPolicySnapshot);
        }
        previous_source = Some(fallback.source());

        let mut targets = BTreeSet::new();
        for target in fallback.targets() {
            if target == fallback.source()
                || production.binary_search(target).is_err()
                || !targets.insert(target)
            {
                return Err(EsmExporterOptionsError::InvalidPolicySnapshot);
            }
        }
    }
    Ok(())
}

fn preflight_batch_contract(batch: &ValidatedExportBatch<'_>) -> Result<(), ExportError> {
    if batch.plans().windows(2).any(|pair| {
        (pair[0].delivery_unit(), pair[0].locale()) >= (pair[1].delivery_unit(), pair[1].locale())
    }) {
        return Err(validated_batch_error());
    }
    for plan in batch.plans() {
        if plan
            .messages()
            .iter()
            .any(|message| *message.domain() != message.key().domain())
            || plan.messages().windows(2).any(|pair| {
                (pair[0].resolved_scope(), pair[0].domain(), pair[0].key())
                    >= (pair[1].resolved_scope(), pair[1].domain(), pair[1].key())
            })
        {
            return Err(validated_batch_error());
        }
    }

    let mut previous_scope = None;
    for typed_output in batch.typed_output() {
        if previous_scope.is_some_and(|scope| scope >= typed_output.model().resolved_scope())
            || typed_output.model().keys().len() != typed_output.messages().len()
        {
            return Err(validated_batch_error());
        }
        previous_scope = Some(typed_output.model().resolved_scope());

        for (key, signature) in typed_output
            .model()
            .keys()
            .iter()
            .zip(typed_output.messages())
        {
            if key != signature.key()
                || signature
                    .arguments()
                    .windows(2)
                    .any(|pair| pair[0].name().as_bytes() >= pair[1].name().as_bytes())
            {
                return Err(validated_batch_error());
            }
        }
    }
    Ok(())
}

fn preflight_builtin_batch(
    options: &EsmExporterOptions,
    batch: &ValidatedExportBatch<'_>,
) -> Result<(), ExportError> {
    let main = DeliveryUnitId::main();
    if let Some(plan) = batch
        .plans()
        .iter()
        .find(|plan| plan.delivery_unit() != &main)
    {
        let evidence = UnsupportedBatchEvidence::try_new(
            UnsupportedBatchFeature::DeliveryUnitPartitioning,
            UnsupportedBatchLocation::Plan {
                delivery_unit: plan.delivery_unit().clone(),
                locale: plan.locale().clone(),
            },
        )
        .ok_or_else(capability_preflight_error)?;
        return Err(ExportError::unsupported_batch(evidence));
    }

    if batch.plans().len() != options.production_locales.len()
        || batch
            .plans()
            .iter()
            .zip(options.production_locales.iter())
            .any(|(plan, locale)| plan.locale() != locale)
    {
        return Err(capability_preflight_error());
    }

    for plan in batch.plans() {
        let fallback_targets = options.fallback_targets(plan.locale());
        if plan.messages().iter().any(|message| {
            message.definition_locale() != plan.locale()
                && !fallback_targets.contains(message.definition_locale())
        }) {
            return Err(capability_preflight_error());
        }
    }
    Ok(())
}

fn build_locale_assets<'a>(
    options: &EsmExporterOptions,
    plans: &'a [MessageBundlePlan],
) -> Result<Vec<LocaleAsset<'a>>, ExportError> {
    let mut paths = BTreeSet::new();
    let mut assets = Vec::with_capacity(plans.len());
    for plan in plans {
        let path = path::locale_path(plan.locale())?;
        if !paths.insert(path.clone()) {
            return Err(duplicate_artifact_path_error());
        }
        let eager_alias = options.eager_locales.0.binary_search(plan.locale()).ok();
        assets.push(LocaleAsset {
            plan,
            path,
            eager_alias,
        });
    }
    Ok(assets)
}

fn build_accessor_assets<'a>(
    typed_output: &'a [ValidatedTypedOutput<'a>],
) -> Result<Vec<AccessorAsset<'a>>, ExportError> {
    let mut paths = BTreeSet::new();
    let mut assets = Vec::with_capacity(typed_output.len());
    for output in typed_output {
        let path = path::accessor_path(output.model().resolved_scope())?;
        if !paths.insert(path.clone()) {
            return Err(duplicate_artifact_path_error());
        }
        assets.push(AccessorAsset {
            typed_output: output,
            path,
        });
    }
    Ok(assets)
}

fn validate_artifact_assembly(
    artifacts: &[ExportArtifact],
    locale_assets: &[LocaleAsset<'_>],
    loader_path: &ExportArtifactPath,
    accessor_assets: &[AccessorAsset<'_>],
) -> Result<(), ExportError> {
    let Some(expected_count) = locale_assets
        .len()
        .checked_add(1)
        .and_then(|count| count.checked_add(accessor_assets.len()))
    else {
        return Err(artifact_assembly_error());
    };
    if artifacts.len() != expected_count {
        return Err(artifact_assembly_error());
    }

    for (artifact, asset) in artifacts.iter().zip(locale_assets) {
        if artifact.logical_path() != &asset.path
            || artifact.kind().as_str() != ESM_MODULE_KIND
            || artifact.format_version() != FORMAT_VERSION
            || artifact
                .metadata()
                .media_type()
                .map(ExportMediaType::as_str)
                != Some(JAVASCRIPT_MEDIA_TYPE)
            || !artifact.metadata().relationships().is_empty()
        {
            return Err(artifact_assembly_error());
        }
    }

    let loader = &artifacts[locale_assets.len()];
    if loader.logical_path() != loader_path
        || loader.kind().as_str() != LOADER_MAP_KIND
        || loader.format_version() != FORMAT_VERSION
        || loader.metadata().media_type().map(ExportMediaType::as_str)
            != Some(JAVASCRIPT_MEDIA_TYPE)
        || loader.metadata().relationships().len() != locale_assets.len()
        || loader
            .metadata()
            .relationships()
            .iter()
            .zip(locale_assets)
            .any(|(relationship, asset)| {
                relationship.kind() != asset.relationship_kind()
                    || relationship.target() != &asset.path
            })
    {
        return Err(artifact_assembly_error());
    }

    for (artifact, asset) in artifacts[locale_assets.len() + 1..]
        .iter()
        .zip(accessor_assets)
    {
        if artifact.logical_path() != &asset.path
            || artifact.kind().as_str() != TYPESCRIPT_ACCESSOR_KIND
            || artifact.format_version() != FORMAT_VERSION
            || artifact
                .metadata()
                .media_type()
                .map(ExportMediaType::as_str)
                != Some(TYPESCRIPT_MEDIA_TYPE)
            || !artifact.metadata().relationships().is_empty()
        {
            return Err(artifact_assembly_error());
        }
    }
    Ok(())
}

fn checked_kind(value: &str) -> Result<ExportArtifactKind, ExportError> {
    ExportArtifactKind::try_new(value).map_err(|_| artifact_assembly_error())
}

fn checked_media_type(value: &str) -> Result<ExportMediaType, ExportError> {
    ExportMediaType::try_new(value).map_err(|_| artifact_assembly_error())
}

fn validated_batch_error() -> ExportError {
    ExportError::internal_invariant(InternalInvariantEvidence::new(
        InternalInvariantViolation::ValidatedBatchContract,
    ))
}

fn capability_preflight_error() -> ExportError {
    ExportError::internal_invariant(InternalInvariantEvidence::new(
        InternalInvariantViolation::CapabilityPreflightContract,
    ))
}

pub(super) fn artifact_assembly_error() -> ExportError {
    ExportError::internal_invariant(InternalInvariantEvidence::new(
        InternalInvariantViolation::ArtifactAssemblyState,
    ))
}

fn duplicate_artifact_path_error() -> ExportError {
    ExportError::invalid_output(InvalidOutputEvidence::new(
        InvalidOutputViolation::DuplicateArtifactPath,
    ))
}

#[cfg(test)]
mod tests {
    use intlify_contract::{LinkLimits, Locale};
    use intlify_linker::{DynamicReferenceMode, LinkPolicy, PlacementPolicy};

    use super::{EsmExporterOptions, EsmExporterOptionsError};

    fn locale(value: &str) -> Locale {
        Locale::try_new(value).unwrap()
    }

    fn policy(locales: &[&str]) -> LinkPolicy {
        LinkPolicy::try_new(
            locales.iter().map(|value| locale(value)).collect(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn eager_locale_permutations_construct_one_canonical_option_snapshot() {
        let policy = policy(&["ja", "en", "fr"]);
        let first = EsmExporterOptions::try_new(&policy, vec![locale("ja"), locale("en")]).unwrap();
        let second =
            EsmExporterOptions::try_new(&policy, vec![locale("en"), locale("ja")]).unwrap();

        assert_eq!(first, second);
        assert_eq!(
            first
                .eager_locales()
                .locales()
                .iter()
                .map(Locale::as_str)
                .collect::<Vec<_>>(),
            ["en", "ja"]
        );
    }

    #[test]
    fn eager_locale_validation_rejects_duplicates_before_non_members() {
        let policy = policy(&["en"]);
        assert_eq!(
            EsmExporterOptions::try_new(&policy, vec![locale("ja"), locale("ja")]),
            Err(EsmExporterOptionsError::DuplicateEagerLocale(locale("ja")))
        );
        assert_eq!(
            EsmExporterOptions::try_new(&policy, vec![locale("ja")]),
            Err(EsmExporterOptionsError::EagerLocaleNotProduction(locale(
                "ja"
            )))
        );
    }

    #[test]
    fn eager_locale_set_accepts_the_complete_production_boundary() {
        let locale_values = (0..1_024)
            .map(|index| format!("locale-{index:04}"))
            .collect::<Vec<_>>();
        let locale_refs = locale_values.iter().map(String::as_str).collect::<Vec<_>>();
        let policy = policy(&locale_refs);
        let options = EsmExporterOptions::try_new(
            &policy,
            locale_values
                .iter()
                .rev()
                .map(|value| locale(value))
                .collect(),
        )
        .unwrap();

        assert_eq!(options.eager_locales().locales().len(), 1_024);
        assert_eq!(
            options.eager_locales().locales(),
            policy.production_locales()
        );
    }
}
