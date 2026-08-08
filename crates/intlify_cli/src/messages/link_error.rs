// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Stable evidence projection for stateless linker operational errors.
//!
//! The adapter preserves checked identities and closed variant names while
//! excluding Rust debug output and implementation-specific source chains.

use intlify_contract::{
    ArtifactNamespace, CatalogScopeId, DeliveryUnitId, LinkLimitEvidence, LinkLimitObservation,
    MessageSelector, ReferenceArtifactIdentity, SourceDocumentIdentity,
};
use intlify_linker::{
    ArtifactContractSubject, ArtifactKind, ConfiguredRootIdentity, DeliveryEdgeEndpoint,
    InvalidRequestError, ResolvedCatalogScopeId, ScopeEndpoint, ScopeUse, UnsupportedContractError,
};
use serde_json::{json, Value};

pub(super) fn invalid_request_value(error: &InvalidRequestError) -> Value {
    match error {
        InvalidRequestError::EmptyProductionLocales => {
            json!({ "reason": "empty_production_locales" })
        }
        InvalidRequestError::DuplicateProductionLocale(locale) => json!({
            "reason": "duplicate_production_locale",
            "locale": locale.as_str(),
        }),
        InvalidRequestError::DuplicateFallbackSource(locale) => json!({
            "reason": "duplicate_fallback_source",
            "source": locale.as_str(),
        }),
        InvalidRequestError::FallbackSourceNotProduction(locale) => json!({
            "reason": "fallback_source_not_production",
            "source": locale.as_str(),
        }),
        InvalidRequestError::EmptyFallbackSequence(locale) => json!({
            "reason": "empty_fallback_sequence",
            "source": locale.as_str(),
        }),
        InvalidRequestError::FallbackTargetNotProduction { source, target } => json!({
            "reason": "fallback_target_not_production",
            "source": source.as_str(),
            "target": target.as_str(),
        }),
        InvalidRequestError::FallbackSelfReference(locale) => json!({
            "reason": "fallback_self_reference",
            "source": locale.as_str(),
        }),
        InvalidRequestError::DuplicateFallbackTarget { source, target } => json!({
            "reason": "duplicate_fallback_target",
            "source": source.as_str(),
            "target": target.as_str(),
        }),
        InvalidRequestError::DuplicateConfiguredRoot(root) => json!({
            "reason": "duplicate_configured_root",
            "root": configured_root_value(root),
        }),
        InvalidRequestError::DuplicateCoverageBaseline(scope) => json!({
            "reason": "duplicate_coverage_baseline",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::CoverageBaselineLocaleNotProduction { scope, locale } => json!({
            "reason": "coverage_baseline_locale_not_production",
            "scope": scope_value(scope),
            "locale": locale.as_str(),
        }),
        InvalidRequestError::DuplicateDeclaredScope(scope) => json!({
            "reason": "duplicate_declared_scope",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::UndeclaredMappingEndpoint { endpoint, scope } => json!({
            "reason": "undeclared_mapping_endpoint",
            "endpoint": scope_endpoint_token(*endpoint),
            "scope": scope_value(scope),
        }),
        InvalidRequestError::DuplicateScopeMappingSource(scope) => json!({
            "reason": "duplicate_scope_mapping_source",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::ScopeMappingSelfMap(scope) => json!({
            "reason": "scope_mapping_self_map",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::ScopeMappingTargetIsSource(scope) => json!({
            "reason": "scope_mapping_target_is_source",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::DuplicateCompletenessScope(scope) => json!({
            "reason": "duplicate_completeness_scope",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::NonCanonicalCompletenessOrder { previous, current } => json!({
            "reason": "non_canonical_completeness_order",
            "previous": scope_value(previous),
            "current": scope_value(current),
        }),
        InvalidRequestError::MissingCompletenessScope(scope) => json!({
            "reason": "missing_completeness_scope",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::ExtraCompletenessScope(scope) => json!({
            "reason": "extra_completeness_scope",
            "scope": scope_value(scope),
        }),
        InvalidRequestError::DuplicateDeliveryUnit(delivery_unit) => json!({
            "reason": "duplicate_delivery_unit",
            "deliveryUnit": delivery_unit_value(delivery_unit),
        }),
        InvalidRequestError::UnknownDeliveryEdgeEndpoint {
            endpoint,
            delivery_unit,
        } => json!({
            "reason": "unknown_delivery_edge_endpoint",
            "endpoint": delivery_endpoint_token(*endpoint),
            "deliveryUnit": delivery_unit_value(delivery_unit),
        }),
        InvalidRequestError::DuplicateDeliveryEdge { parent, child } => json!({
            "reason": "duplicate_delivery_edge",
            "parent": delivery_unit_value(parent),
            "child": delivery_unit_value(child),
        }),
        InvalidRequestError::DeliverySelfEdge(delivery_unit) => json!({
            "reason": "delivery_self_edge",
            "deliveryUnit": delivery_unit_value(delivery_unit),
        }),
        InvalidRequestError::DeliveryGraphCycle => {
            json!({ "reason": "delivery_graph_cycle" })
        }
        InvalidRequestError::DuplicateReferenceArtifact(identity) => json!({
            "reason": "duplicate_reference_artifact",
            "artifact": reference_identity_value(identity),
        }),
        InvalidRequestError::DuplicateDefinitionArtifact(source) => json!({
            "reason": "duplicate_definition_artifact",
            "source": source_value(source),
        }),
        InvalidRequestError::UndeclaredScope { usage, scope } => json!({
            "reason": "undeclared_scope",
            "usage": scope_use_token(*usage),
            "scope": scope_value(scope),
        }),
        InvalidRequestError::MappingInventoryMismatch => {
            json!({ "reason": "mapping_inventory_mismatch" })
        }
        InvalidRequestError::ResolvedConfiguredRootConflict(root) => json!({
            "reason": "resolved_configured_root_conflict",
            "root": configured_root_value(root),
        }),
        InvalidRequestError::ResolvedCoverageBaselineConflict {
            scope,
            first,
            second,
        } => json!({
            "reason": "resolved_coverage_baseline_conflict",
            "scope": resolved_scope_value(scope),
            "firstLocale": first.as_str(),
            "secondLocale": second.as_str(),
        }),
        InvalidRequestError::MissingReferenceDeliveryUnit {
            artifact,
            delivery_unit,
        } => json!({
            "reason": "missing_reference_delivery_unit",
            "artifact": reference_identity_value(artifact),
            "deliveryUnit": delivery_unit_value(delivery_unit),
        }),
        InvalidRequestError::ConfiguredRootRequiresDeliveryUnit => {
            json!({ "reason": "configured_root_requires_delivery_unit" })
        }
        InvalidRequestError::ResolvedDomainConflict {
            scope,
            first,
            second,
        } => json!({
            "reason": "resolved_domain_conflict",
            "scope": resolved_scope_value(scope),
            "firstDomain": first.as_str(),
            "secondDomain": second.as_str(),
        }),
        InvalidRequestError::ArtifactContract { subject, violation } => json!({
            "reason": "artifact_contract",
            "subject": artifact_subject_value(subject),
            "violation": super::artifact_error::violation_value(violation),
        }),
    }
}

pub(super) fn unsupported_contract_value(error: &UnsupportedContractError) -> Value {
    match error {
        UnsupportedContractError::ArtifactVersion {
            kind,
            subject,
            evidence,
        } => {
            let version = super::artifact_error::version_evidence_value(evidence);
            json!({
                "reason": "artifact_version",
                "artifactKind": artifact_kind_token(*kind),
                "subject": artifact_subject_value(subject),
                "observed": version["observed"].clone(),
                "supported": version["supported"].clone(),
            })
        }
    }
}

pub(super) fn limit_value(evidence: &LinkLimitEvidence) -> Value {
    json!({
        "counter": evidence.counter().as_str(),
        "effectiveLimit": evidence.effective_limit(),
        "observation": match evidence.observation() {
            LinkLimitObservation::Exact(attempted) => {
                json!({ "kind": "exact", "attempted": attempted })
            }
            LinkLimitObservation::ArithmeticOverflow => {
                json!({ "kind": "arithmetic_overflow" })
            }
        },
    })
}

fn configured_root_value(root: &ConfiguredRootIdentity) -> Value {
    json!({
        "scope": scope_value(root.scope()),
        "domain": root.domain().as_str(),
        "selector": selector_value(root.selector()),
    })
}

fn selector_value(selector: &MessageSelector) -> Value {
    match selector {
        MessageSelector::Exact(key) => json!({ "kind": "exact", "key": key.as_str() }),
        MessageSelector::Prefix(prefix) => {
            json!({ "kind": "prefix", "prefix": prefix.as_str() })
        }
        MessageSelector::Pattern(pattern) => {
            json!({ "kind": "pattern", "pattern": pattern.as_str() })
        }
        MessageSelector::AllInScope => json!({ "kind": "all-in-scope" }),
        MessageSelector::UnboundedDynamic => json!({ "kind": "unbounded-dynamic" }),
    }
}

fn artifact_subject_value(subject: &ArtifactContractSubject) -> Value {
    match subject {
        ArtifactContractSubject::Reference(identity) => json!({
            "kind": "reference",
            "identity": reference_identity_value(identity),
        }),
        ArtifactContractSubject::Definition(source) => json!({
            "kind": "definition",
            "source": source_value(source),
        }),
    }
}

fn reference_identity_value(identity: &ReferenceArtifactIdentity) -> Value {
    json!({
        "namespace": namespace_value(identity.namespace()),
        "segments": identity.segments().iter().map(intlify_contract::ReferenceArtifactSegment::as_str).collect::<Vec<_>>(),
    })
}

fn source_value(source: &SourceDocumentIdentity) -> Value {
    json!({
        "namespace": namespace_value(source.namespace()),
        "path": source.path().segments().iter().map(intlify_contract::PortablePathSegment::as_str).collect::<Vec<_>>(),
    })
}

fn resolved_scope_value(scope: &ResolvedCatalogScopeId) -> Value {
    scope_value(scope.as_catalog_scope())
}

fn scope_value(scope: &CatalogScopeId) -> Value {
    json!({
        "namespace": namespace_value(scope.namespace()),
        "name": scope.name().as_str(),
    })
}

fn namespace_value(namespace: ArtifactNamespace) -> Value {
    json!({ "kind": namespace.as_str() })
}

fn delivery_unit_value(delivery_unit: &DeliveryUnitId) -> Value {
    Value::Array(
        delivery_unit
            .segments()
            .iter()
            .map(|segment| Value::String(segment.as_str().to_owned()))
            .collect(),
    )
}

const fn scope_endpoint_token(endpoint: ScopeEndpoint) -> &'static str {
    match endpoint {
        ScopeEndpoint::Source => "source",
        ScopeEndpoint::Target => "target",
    }
}

const fn delivery_endpoint_token(endpoint: DeliveryEdgeEndpoint) -> &'static str {
    match endpoint {
        DeliveryEdgeEndpoint::Parent => "parent",
        DeliveryEdgeEndpoint::Child => "child",
    }
}

const fn scope_use_token(usage: ScopeUse) -> &'static str {
    match usage {
        ScopeUse::Reference => "reference",
        ScopeUse::Definition => "definition",
        ScopeUse::ConfiguredRoot => "configured_root",
        ScopeUse::CoverageBaseline => "coverage_baseline",
    }
}

const fn artifact_kind_token(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Reference => "reference",
        ArtifactKind::Definition => "definition",
    }
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        LinkLimitCounter, LinkLimitEvidence, LinkLimitObservation, LinkLimitSubject,
    };
    use intlify_linker::InvalidRequestError;

    use super::{invalid_request_value, limit_value};

    #[test]
    fn invalid_request_adapter_uses_a_closed_reason_instead_of_debug_text() {
        let evidence = invalid_request_value(&InvalidRequestError::EmptyProductionLocales);

        assert_eq!(
            evidence,
            serde_json::json!({
                "reason": "empty_production_locales"
            })
        );
    }

    #[test]
    fn link_limit_adapter_preserves_the_checked_observation_shape() {
        let evidence = LinkLimitEvidence::try_new(
            LinkLimitCounter::ReferenceArtifacts,
            LinkLimitSubject::Request,
            0,
            LinkLimitObservation::Exact(1),
        )
        .unwrap();
        let value = limit_value(&evidence);

        assert_eq!(value["counter"], "reference_artifacts");
        assert_eq!(value["effectiveLimit"], 0);
        assert_eq!(value["observation"]["kind"], "exact");
        assert_eq!(value["observation"]["attempted"], 1);
    }
}
