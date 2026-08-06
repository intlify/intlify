// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Message-linker configuration.
//!
//! This module owns the raw `messages` section validation, cross-section scope
//! references, and immutable construction of linker policy and producer inputs.
//! It performs no filesystem discovery, source parsing, artifact loading, or
//! linking.

#[cfg(feature = "benchmark")]
#[doc(hidden)]
pub mod benchmark;
mod completeness;
mod config;
#[allow(dead_code)] // Activated with the public delivery config and emit command.
mod delivery;
#[allow(dead_code)] // Activated with one exporter factory per selected target.
mod exporter_registry;
#[allow(dead_code)] // Consumed when the project-link command orchestration is wired.
mod inventory;
mod observation;
#[allow(dead_code)] // Kept internal until a user-facing message command consumes it.
pub(crate) mod orchestration;
mod physical;
#[allow(dead_code)] // Consumed when project-link orchestration is wired.
pub(crate) mod reference;

pub use config::{
    validate_messages_config, MessageCatalogKeyDomain, MessageDynamicReferenceMode,
    MessageJsCallKind, MessageJsKeySyntax, MessageJsProducerConfig, MessageJsRecognizerConfig,
    MessageProducersConfig, MessageRootConfig, MessageSelectorConfig, MessagesConfig,
    MessagesConfigError, MessagesConfigReason, MessagesConfigViolation, ResolvedJsProducerConfig,
    ResolvedMessageProducers, ResolvedMessagesConfig,
};

#[cfg(test)]
mod tests {
    use intlify_contract::{LinkLimits, Locale};
    use intlify_linker::{DynamicReferenceMode, LinkPolicy, PlacementPolicy};

    use super::delivery::{DeliveryTargetInput, ResolvedDeliveryTargets};
    use super::exporter_registry::BuiltInExporterRegistry;

    #[test]
    fn selected_targets_construct_exactly_one_fresh_exporter_each() {
        let policy = LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            Vec::new(),
            Vec::new(),
            Vec::new(),
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap();
        let eager = Vec::new();
        let resolved = ResolvedDeliveryTargets::try_new(
            &[
                DeliveryTargetInput::new("web", "esm", "dist/web", &eager, None),
                DeliveryTargetInput::new("native", "esm", "dist/native", &eager, None),
            ],
            &policy,
        )
        .unwrap();
        let selected = resolved.select(None).unwrap();
        let registry = BuiltInExporterRegistry::new();

        let exporters = selected
            .targets()
            .iter()
            .map(|target| registry.create(target, &policy).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(exporters.len(), selected.targets().len());
    }
}
