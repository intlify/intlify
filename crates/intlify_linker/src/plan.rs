// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Fully owned, deterministic message bundle plans.
//!
//! Plans retain the exact checked definition snapshot selected by the linker.
//! They are language-neutral semantic results, not serialized runtime output
//! and not a platform-exporter construction API.

use intlify_contract::{CatalogKey, CatalogKeyDomain, DeliveryUnitId, Locale, MessagePayload};

use crate::{DefinitionLocation, ResolvedCatalogScopeId};

/// One selected definition snapshot in a delivery plan.
#[derive(Debug, PartialEq, Eq)]
pub struct ResolvedMessage {
    resolved_scope: ResolvedCatalogScopeId,
    domain: CatalogKeyDomain,
    key: CatalogKey,
    definition_locale: Locale,
    message: MessagePayload,
    definition: DefinitionLocation,
}

impl ResolvedMessage {
    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        resolved_scope: ResolvedCatalogScopeId,
        domain: CatalogKeyDomain,
        key: CatalogKey,
        definition_locale: Locale,
        message: MessagePayload,
        definition: DefinitionLocation,
    ) -> Self {
        Self {
            resolved_scope,
            domain,
            key,
            definition_locale,
            message,
            definition,
        }
    }

    /// Return the post-mapping semantic scope.
    #[must_use]
    pub const fn resolved_scope(&self) -> &ResolvedCatalogScopeId {
        &self.resolved_scope
    }

    /// Return the exact catalog-key comparison domain.
    #[must_use]
    pub const fn domain(&self) -> &CatalogKeyDomain {
        &self.domain
    }

    /// Return the canonical catalog key.
    #[must_use]
    pub const fn key(&self) -> &CatalogKey {
        &self.key
    }

    /// Return the locale of the selected definition snapshot.
    #[must_use]
    pub const fn definition_locale(&self) -> &Locale {
        &self.definition_locale
    }

    /// Return the exact opaque MF2 source payload.
    #[must_use]
    pub const fn message(&self) -> &MessagePayload {
        &self.message
    }

    /// Return the selected definition's portable location.
    #[must_use]
    pub const fn definition(&self) -> &DefinitionLocation {
        &self.definition
    }
}

/// Complete message selection for one delivery-unit and requested-locale pair.
#[derive(Debug, PartialEq, Eq)]
pub struct MessageBundlePlan {
    delivery_unit: DeliveryUnitId,
    locale: Locale,
    messages: Box<[ResolvedMessage]>,
}

impl MessageBundlePlan {
    pub(crate) fn new(
        delivery_unit: DeliveryUnitId,
        locale: Locale,
        messages: Vec<ResolvedMessage>,
    ) -> Self {
        Self {
            delivery_unit,
            locale,
            messages: messages.into_boxed_slice(),
        }
    }

    /// Return the exact checked delivery-unit identity.
    #[must_use]
    pub const fn delivery_unit(&self) -> &DeliveryUnitId {
        &self.delivery_unit
    }

    /// Return the requested production locale.
    #[must_use]
    pub const fn locale(&self) -> &Locale {
        &self.locale
    }

    /// Return resolved messages in canonical logical-identity order.
    #[must_use]
    pub const fn messages(&self) -> &[ResolvedMessage] {
        &self.messages
    }
}
