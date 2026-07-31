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
