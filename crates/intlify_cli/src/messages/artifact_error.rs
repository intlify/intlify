// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Stable CLI projection of checked message-artifact contract errors.
//!
//! Both definition production and external reference ingestion use this module
//! so reporters cannot expose parser text, debug formatting, or route-specific
//! evidence shapes for the same typed contract failure.

use intlify_contract::{
    ArtifactContractError, ArtifactEnvelopeLocation, ArtifactPathRole, ArtifactVersionSupport,
    DefinitionEnvelopeField, DefinitionField, LinkLimitObservation, ReferenceEnvelopeField,
    ReferenceField,
};
use serde_json::{json, Map, Value};

/// Return the boundary kind and the matching closed evidence body.
pub(super) fn contract_error_parts(error: &ArtifactContractError) -> (&'static str, Value) {
    match error {
        ArtifactContractError::InvalidArtifact(violation) => (
            "invalid_artifact",
            json!({ "violation": violation_value(violation) }),
        ),
        ArtifactContractError::UnsupportedVersion(evidence) => {
            ("unsupported_version", version_evidence_value(evidence))
        }
        ArtifactContractError::Limit(evidence) => (
            "limit",
            json!({
                "counter": evidence.counter().as_str(),
                "effectiveLimit": evidence.effective_limit(),
                "observation": limit_observation_value(evidence.observation()),
            }),
        ),
    }
}

/// Project one artifact violation without its enclosing error kind.
pub(super) fn violation_value(violation: &intlify_contract::ArtifactViolation) -> Value {
    json!({
        "code": violation_code_token(violation.code()),
        "location": violation_location_value(violation.location()),
    })
}

/// Project one checked artifact-version negotiation failure.
pub(super) fn version_evidence_value(
    evidence: &intlify_contract::ArtifactVersionEvidence,
) -> Value {
    json!({
        "observed": version_value(evidence.observed()),
        "supported": version_support_value(evidence.supported()),
    })
}

fn version_value(version: intlify_contract::ArtifactVersion) -> Value {
    json!({ "major": version.major(), "minor": version.minor() })
}

fn version_support_value(support: ArtifactVersionSupport) -> Value {
    match support {
        ArtifactVersionSupport::Exact(version) => {
            json!({ "kind": "exact", "version": version_value(version) })
        }
        ArtifactVersionSupport::StableRange { major, max_minor } => json!({
            "kind": "stable_range",
            "major": major,
            "maxMinor": max_minor,
        }),
    }
}

fn limit_observation_value(observation: LinkLimitObservation) -> Value {
    match observation {
        LinkLimitObservation::Exact(attempted) => {
            json!({ "kind": "exact", "attempted": attempted })
        }
        LinkLimitObservation::ArithmeticOverflow => {
            json!({ "kind": "arithmetic_overflow" })
        }
    }
}

fn violation_location_value(location: &intlify_contract::ArtifactViolationLocation) -> Value {
    use intlify_contract::ArtifactViolationLocation;

    match location {
        ArtifactViolationLocation::Root => json!({ "kind": "root" }),
        ArtifactViolationLocation::Envelope(location) => envelope_location_value(*location),
        ArtifactViolationLocation::EnvelopePathSegment {
            role,
            segment_ordinal,
        } => json!({
            "kind": "envelope_path_segment",
            "role": path_role_token(*role),
            "segmentOrdinal": segment_ordinal,
        }),
        ArtifactViolationLocation::LogicalAlias {
            alias_ordinal,
            segment_ordinal,
        } => {
            let mut object = ordered_object([
                ("kind", Value::String("logical_alias".to_owned())),
                ("aliasOrdinal", Value::from(*alias_ordinal)),
            ]);
            if let Some(segment_ordinal) = segment_ordinal {
                object.insert("segmentOrdinal".to_owned(), Value::from(*segment_ordinal));
            }
            Value::Object(object)
        }
        ArtifactViolationLocation::Definition { ordinal, field } => {
            let mut object = ordered_object([
                ("kind", Value::String("definition".to_owned())),
                ("ordinal", Value::from(*ordinal)),
            ]);
            if let Some(field) = field {
                object.insert(
                    "field".to_owned(),
                    Value::String(definition_field_token(*field).to_owned()),
                );
            }
            Value::Object(object)
        }
        ArtifactViolationLocation::Reference { ordinal, field } => {
            let mut object = ordered_object([
                ("kind", Value::String("reference".to_owned())),
                ("ordinal", Value::from(*ordinal)),
            ]);
            if let Some(field) = field {
                object.insert(
                    "field".to_owned(),
                    Value::String(reference_field_token(*field).to_owned()),
                );
            }
            Value::Object(object)
        }
        ArtifactViolationLocation::ReferenceOriginSegment {
            reference_ordinal,
            segment_ordinal,
        } => json!({
            "kind": "reference_origin_segment",
            "referenceOrdinal": reference_ordinal,
            "segmentOrdinal": segment_ordinal,
        }),
    }
}

fn envelope_location_value(location: ArtifactEnvelopeLocation) -> Value {
    match location {
        ArtifactEnvelopeLocation::Reference { field } => {
            let mut object = ordered_object([
                ("kind", Value::String("envelope".to_owned())),
                ("artifact", Value::String("reference".to_owned())),
            ]);
            if let Some(field) = field {
                object.insert(
                    "field".to_owned(),
                    Value::String(reference_envelope_field_token(field).to_owned()),
                );
            }
            Value::Object(object)
        }
        ArtifactEnvelopeLocation::Definition { field } => {
            let mut object = ordered_object([
                ("kind", Value::String("envelope".to_owned())),
                ("artifact", Value::String("definition".to_owned())),
            ]);
            if let Some(field) = field {
                object.insert(
                    "field".to_owned(),
                    Value::String(definition_envelope_field_token(field).to_owned()),
                );
            }
            Value::Object(object)
        }
    }
}

const fn path_role_token(role: ArtifactPathRole) -> &'static str {
    match role {
        ArtifactPathRole::DefinitionSource => "definition_source",
        ArtifactPathRole::ReferenceIdentity => "reference_identity",
        ArtifactPathRole::DeliveryUnit => "delivery_unit",
    }
}

const fn reference_envelope_field_token(field: ReferenceEnvelopeField) -> &'static str {
    match field {
        ReferenceEnvelopeField::Kind => "kind",
        ReferenceEnvelopeField::Version => "version",
        ReferenceEnvelopeField::VersionMajor => "version_major",
        ReferenceEnvelopeField::VersionMinor => "version_minor",
        ReferenceEnvelopeField::Producer => "producer",
        ReferenceEnvelopeField::ProducerId => "producer_id",
        ReferenceEnvelopeField::ProducerRevision => "producer_revision",
        ReferenceEnvelopeField::Identity => "identity",
        ReferenceEnvelopeField::IdentityNamespace => "identity_namespace",
        ReferenceEnvelopeField::IdentitySegments => "identity_segments",
        ReferenceEnvelopeField::DeliveryUnit => "delivery_unit",
        ReferenceEnvelopeField::References => "references",
    }
}

const fn definition_envelope_field_token(field: DefinitionEnvelopeField) -> &'static str {
    match field {
        DefinitionEnvelopeField::Kind => "kind",
        DefinitionEnvelopeField::Version => "version",
        DefinitionEnvelopeField::VersionMajor => "version_major",
        DefinitionEnvelopeField::VersionMinor => "version_minor",
        DefinitionEnvelopeField::Producer => "producer",
        DefinitionEnvelopeField::ProducerId => "producer_id",
        DefinitionEnvelopeField::ProducerRevision => "producer_revision",
        DefinitionEnvelopeField::Source => "source",
        DefinitionEnvelopeField::SourceNamespace => "source_namespace",
        DefinitionEnvelopeField::SourcePath => "source_path",
        DefinitionEnvelopeField::LogicalAliases => "logical_aliases",
        DefinitionEnvelopeField::InputFingerprint => "input_fingerprint",
        DefinitionEnvelopeField::FingerprintAlgorithm => "fingerprint_algorithm",
        DefinitionEnvelopeField::FingerprintDigest => "fingerprint_digest",
        DefinitionEnvelopeField::Definitions => "definitions",
    }
}

const fn definition_field_token(field: DefinitionField) -> &'static str {
    match field {
        DefinitionField::Scope => "scope",
        DefinitionField::ScopeNamespace => "scope_namespace",
        DefinitionField::ScopeName => "scope_name",
        DefinitionField::Domain => "domain",
        DefinitionField::Key => "key",
        DefinitionField::Locale => "locale",
        DefinitionField::Message => "message",
        DefinitionField::Source => "source",
        DefinitionField::SourceStructuralPath => "source_structural_path",
        DefinitionField::SourceOccurrence => "source_occurrence",
    }
}

const fn reference_field_token(field: ReferenceField) -> &'static str {
    match field {
        ReferenceField::Scope => "scope",
        ReferenceField::ScopeNamespace => "scope_namespace",
        ReferenceField::ScopeName => "scope_name",
        ReferenceField::Domain => "domain",
        ReferenceField::Selector => "selector",
        ReferenceField::SelectorKind => "selector_kind",
        ReferenceField::SelectorKey => "selector_key",
        ReferenceField::SelectorPrefix => "selector_prefix",
        ReferenceField::SelectorPattern => "selector_pattern",
        ReferenceField::Reason => "reason",
        ReferenceField::Origin => "origin",
        ReferenceField::OriginSource => "origin_source",
        ReferenceField::OriginSourceNamespace => "origin_source_namespace",
        ReferenceField::OriginSourcePath => "origin_source_path",
        ReferenceField::OriginSpan => "origin_span",
        ReferenceField::OriginSpanStart => "origin_span_start",
        ReferenceField::OriginSpanEnd => "origin_span_end",
    }
}

const fn violation_code_token(code: intlify_contract::ArtifactViolationCode) -> &'static str {
    use intlify_contract::ArtifactViolationCode;

    match code {
        ArtifactViolationCode::InvalidUtf8 => "invalid_utf8",
        ArtifactViolationCode::InvalidJsonSyntax => "invalid_json_syntax",
        ArtifactViolationCode::TrailingData => "trailing_data",
        ArtifactViolationCode::MissingMember => "missing_member",
        ArtifactViolationCode::DuplicateMember => "duplicate_member",
        ArtifactViolationCode::UnknownMember => "unknown_member",
        ArtifactViolationCode::NullNotAllowed => "null_not_allowed",
        ArtifactViolationCode::TypeMismatch => "type_mismatch",
        ArtifactViolationCode::InvalidInteger => "invalid_integer",
        ArtifactViolationCode::KindMismatch => "kind_mismatch",
        ArtifactViolationCode::UnknownTag => "unknown_tag",
        ArtifactViolationCode::InvalidValueGrammar => "invalid_value_grammar",
        ArtifactViolationCode::NonCanonicalOrder => "non_canonical_order",
        ArtifactViolationCode::DuplicateValue => "duplicate_value",
        ArtifactViolationCode::InconsistentValue => "inconsistent_value",
        ArtifactViolationCode::DiscontinuousOccurrence => "discontinuous_occurrence",
    }
}

fn ordered_object<const N: usize>(fields: [(&str, Value); N]) -> Map<String, Value> {
    fields
        .into_iter()
        .map(|(name, value)| (name.to_owned(), value))
        .collect()
}

#[cfg(test)]
mod tests {
    use intlify_contract::{decode_reference_artifact, LinkLimitCounter, LinkLimits};

    use super::contract_error_parts;

    #[test]
    fn limit_adapter_preserves_the_exact_structured_contract() {
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceArtifactWireBytes, 0)
            .unwrap();
        let error = decode_reference_artifact(b"{}", &limits).unwrap_err();

        let (kind, evidence) = contract_error_parts(&error);
        assert_eq!(kind, "limit");
        assert_eq!(evidence["counter"], "reference_artifact_wire_bytes");
        assert_eq!(evidence["effectiveLimit"], 0);
        assert_eq!(evidence["observation"]["kind"], "exact");
        assert_eq!(evidence["observation"]["attempted"], 1);
        assert!(evidence.get("limit").is_none());
        assert!(evidence.get("observed").is_none());
    }
}
