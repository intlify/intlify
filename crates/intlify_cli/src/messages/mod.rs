// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Message-linker configuration.
//!
//! This module owns the raw `messages` section validation, cross-section scope
//! references, and immutable construction of linker policy and producer inputs.
//! It performs no filesystem discovery, source parsing, artifact loading, or
//! linking.

mod artifact_error;
#[cfg(feature = "benchmark")]
#[doc(hidden)]
pub mod benchmark;
mod completeness;
mod config;
mod delivery;
pub(crate) mod emit;
mod exporter_registry;
mod inventory;
mod link_error;
mod observation;
pub(crate) mod orchestration;
mod physical;
pub(crate) mod reference;
mod registration;

/// Check an explicit delivery target operand before configuration I/O.
pub(crate) fn is_valid_delivery_target_name(value: &str) -> bool {
    delivery::DeliveryTargetName::try_new(value).is_ok()
}

pub use config::{
    validate_messages_config, MessageCatalogKeyDomain, MessageDeliveryConfig,
    MessageDeliveryTargetConfig, MessageDynamicReferenceMode, MessageJsCallKind,
    MessageJsKeySyntax, MessageJsProducerConfig, MessageJsRecognizerConfig, MessageProducersConfig,
    MessageRootConfig, MessageSelectorConfig, MessagesConfig, MessagesConfigError,
    MessagesConfigReason, MessagesConfigViolation, ResolvedJsProducerConfig,
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
