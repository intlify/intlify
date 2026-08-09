// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private built-in exporter registry and typed exporter factories.
//!
//! This module maps one already checked delivery target to exactly one new
//! immutable [`PlatformExporter`] instance. The registry is closed to the exact
//! initial `esm` identifier and passes only checked linker policy plus checked
//! eager locales into the typed ESM factory. It performs no raw option lookup,
//! environment access, global mutation, exporter invocation, or filesystem I/O.

use intlify_export::{EsmExporter, EsmExporterOptions, EsmExporterOptionsError, PlatformExporter};
use intlify_linker::LinkPolicy;

use super::delivery::{BuiltInExporterId, ResolvedDeliveryTarget};

const BUILT_IN_EXPORTER_IDS: [BuiltInExporterId; 1] = [BuiltInExporterId::Esm];

/// Immutable closed registry for built-in platform exporters.
#[derive(Debug, Clone, Copy)]
pub(crate) struct BuiltInExporterRegistry {
    ids: &'static [BuiltInExporterId],
}

impl Default for BuiltInExporterRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl BuiltInExporterRegistry {
    /// Construct the stateless built-in registry.
    pub(crate) const fn new() -> Self {
        Self {
            ids: &BUILT_IN_EXPORTER_IDS,
        }
    }

    /// Return the complete registry identities in canonical token order.
    #[cfg(test)]
    pub(crate) const fn ids(&self) -> &'static [BuiltInExporterId] {
        self.ids
    }

    /// Construct exactly one fresh exporter for one checked target.
    pub(crate) fn create(
        &self,
        target: &ResolvedDeliveryTarget,
        policy: &LinkPolicy,
    ) -> Result<Box<dyn PlatformExporter>, BuiltInExporterFactoryError> {
        debug_assert!(self.ids.contains(&target.exporter()));
        match target.exporter() {
            BuiltInExporterId::Esm => Ok(Box::new(create_esm_exporter(target, policy)?)),
        }
    }
}

/// Failure while reconstructing typed built-in exporter options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BuiltInExporterFactoryError {
    /// The checked target and supplied link policy do not form valid ESM options.
    EsmOptions(EsmExporterOptionsError),
}

fn create_esm_exporter(
    target: &ResolvedDeliveryTarget,
    policy: &LinkPolicy,
) -> Result<EsmExporter, BuiltInExporterFactoryError> {
    let options = EsmExporterOptions::try_new(policy, target.eager_locales().locales().to_vec())
        .map_err(BuiltInExporterFactoryError::EsmOptions)?;
    Ok(EsmExporter::new(options))
}

#[cfg(test)]
mod tests {
    use intlify_contract::{LinkLimits, Locale};
    use intlify_export::{EsmExporterOptionsError, PlatformExporter};
    use intlify_linker::{DynamicReferenceMode, LinkPolicy, LocaleFallback, PlacementPolicy};

    use super::{create_esm_exporter, BuiltInExporterFactoryError, BuiltInExporterRegistry};
    use crate::messages::delivery::{
        BuiltInExporterId, DeliveryTargetInput, ResolvedDeliveryTargets,
    };

    fn locale(value: &str) -> Locale {
        Locale::try_new(value).unwrap()
    }

    fn policy(locales: &[&str], fallbacks: Vec<LocaleFallback>) -> LinkPolicy {
        LinkPolicy::try_new(
            locales.iter().map(|value| locale(value)).collect(),
            fallbacks,
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap()
    }

    #[test]
    fn registry_contains_only_the_exact_initial_esm_identity() {
        let registry = BuiltInExporterRegistry::new();
        assert_eq!(registry.ids(), [BuiltInExporterId::Esm]);
        assert_eq!(registry.ids()[0].as_str(), "esm");
    }

    #[test]
    fn typed_factory_copies_exact_policy_and_checked_eager_locales() {
        let policy = policy(
            &["ja", "en"],
            vec![LocaleFallback::new(locale("ja"), vec![locale("en")])],
        );
        let eager = vec!["en".to_owned()];
        let targets = ResolvedDeliveryTargets::try_new(
            &[DeliveryTargetInput::new(
                "web",
                "esm",
                "dist/messages",
                &eager,
                Some("duplicate"),
            )],
            &policy,
        )
        .unwrap();
        let target = &targets.targets()[0];

        let exporter = create_esm_exporter(target, &policy).unwrap();
        assert_eq!(
            exporter.options().production_locales(),
            policy.production_locales()
        );
        assert_eq!(exporter.options().fallbacks(), policy.fallbacks());
        assert_eq!(exporter.options().eager_locales().locales(), [locale("en")]);

        let boxed: Box<dyn PlatformExporter> = BuiltInExporterRegistry::new()
            .create(target, &policy)
            .unwrap();
        drop(boxed);
    }

    #[test]
    fn factory_revalidates_target_options_against_the_supplied_policy() {
        let configured_policy = policy(&["en", "fr"], Vec::new());
        let eager = vec!["fr".to_owned()];
        let targets = ResolvedDeliveryTargets::try_new(
            &[DeliveryTargetInput::new(
                "web",
                "esm",
                "dist/messages",
                &eager,
                None,
            )],
            &configured_policy,
        )
        .unwrap();
        let different_policy = policy(&["en"], Vec::new());

        assert!(matches!(
            BuiltInExporterRegistry::new().create(&targets.targets()[0], &different_policy),
            Err(BuiltInExporterFactoryError::EsmOptions(
                EsmExporterOptionsError::EagerLocaleNotProduction(ref locale)
            )) if locale.as_str() == "fr"
        ));
    }

    #[test]
    fn registry_and_factory_products_preserve_thread_capabilities() {
        fn assert_send_sync<T: Send + Sync>() {}
        fn assert_send<T: Send>() {}

        assert_send_sync::<BuiltInExporterRegistry>();
        assert_send::<Box<dyn PlatformExporter>>();
    }
}
