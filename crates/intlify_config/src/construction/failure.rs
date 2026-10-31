// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's construction failures, and the order construction reports
//! them in.
//!
//! Construction reports only the first failing check. The order is one table
//! of rows, each a component, a check, and the reason that check gives; the
//! table is the construction-admission body's precedence, written from these
//! values. A check's spelling names one column cell of design 015's table; what
//! the check does is that row's, so one spelling can mean different work on
//! different components.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Why construction refuses its input.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ConstructionFailure {
    ConstructionInputLimitExceeded,
    ProfileSpecificationSchemaMissing,
    ProfileSpecificationSchemaInvalid,
    ProfileSpecificationSchemaIdentityMismatch,
    ProfileSpecificationSchemaRevisionUnsupported,
    ProfileSpecificationSchemaDigestMismatch,
    ProfileSpecificationMissing,
    ProfileSpecificationBodySchemaInvalid,
    ProfileSpecificationIdentityMismatch,
    ProfileSpecificationRevisionUnsupported,
    ProfileSpecificationDigestMismatch,
    ConfigurationSchemaAuthoritySetMissing,
    ConfigurationSchemaAuthoritySetSchemaInvalid,
    ConfigurationSchemaAuthoritySetIdentityMismatch,
    ConfigurationSchemaAuthoritySetRevisionUnsupported,
    ConfigurationSchemaAuthoritySetDigestMismatch,
    ConfigurationSchemaMemberIdentityMismatch,
    ConfigurationSchemaMemberRevisionUnsupported,
    ConfigurationSchemaMemberDialectUnsupported,
    ConfigurationSchemaMemberDigestMismatch,
    ConfigurationSchemaMemberSchemaInvalid,
    ResolverCapabilitySpecificationMissing,
    ResolverCapabilitySpecificationSchemaInvalid,
    ResolverCapabilitySpecificationIdentityMismatch,
    ResolverCapabilitySpecificationRevisionUnsupported,
    ResolverCapabilitySpecificationDigestMismatch,
    RegistryPackageIncomplete,
    RegistryPackageSchemaInvalid,
    RegistryPackageIdentityMismatch,
    RegistryPackageRevisionUnsupported,
    RegistryPackageDigestMismatch,
    RegistryMemberIdentityMismatch,
    RegistryMemberRevisionUnsupported,
    RegistryMemberDigestMismatch,
    RegistryMemberSchemaInvalid,
    RegistryCrossReferenceInvalid,
    ConstructionCrossAuthorityReferenceInvalid,
    ConstructionAuthorityNotImplemented,
}

/// What a precedence row checks: every input at once, one of the five
/// components, or each member of a container in its order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Component {
    All,
    ProfileSpecificationSchema,
    ProfileSpecification,
    ConfigurationSchemaAuthoritySet,
    ConfigurationSchemaMember,
    ResolverCapabilitySpecification,
    RegistryPackage,
    RegistryMember,
}

/// One check cell of design 015's construction-failure table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Check {
    // The construction-root bounds, in their canonical order.
    ConstructionBounds,
    // The component, or a container member's bytes, is present.
    Presence,
    // The declared tuple and strict UTF-8 JSON, with what the row adds: a
    // schema's Draft 7 `$schema`, or a container's envelope schema.
    Envelope,
    Identity,
    Revision,
    FileDigest,
    // Strict UTF-8 JSON, a `$schema` naming Draft 7, and the Draft 7
    // meta-schema.
    MetaSchema,
    // The body against the schema that admits it.
    BodySchema,
    SemanticDigest,
    Dialect,
    // A member's strict UTF-8 JSON, then a schema member's Draft 7 checks or a
    // registry against its admitted schema.
    MemberSchema,
    CrossReferences,
    CrossAuthorityRules,
    // The Profile Specification's semantic digest and every other file digest
    // equal the ones the resolver was compiled from.
    ImplementedAuthority,
}

/// One row of the construction-failure precedence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrecedenceRow {
    pub(crate) component: Component,
    pub(crate) check: Check,
    pub(crate) reason: ConstructionFailure,
}

const fn row(component: Component, check: Check, reason: ConstructionFailure) -> PrecedenceRow {
    PrecedenceRow {
        component,
        check,
        reason,
    }
}

/// Design 015's construction-failure precedence, row for row.
#[rustfmt::skip]
pub(crate) const PRECEDENCE: [PrecedenceRow; 43] = {
    use Check as K;
    use Component as C;
    use ConstructionFailure as F;
    [
        row(C::All, K::ConstructionBounds, F::ConstructionInputLimitExceeded),
        row(C::ProfileSpecificationSchema, K::Presence, F::ProfileSpecificationSchemaMissing),
        row(C::ProfileSpecificationSchema, K::Envelope, F::ProfileSpecificationSchemaInvalid),
        row(C::ProfileSpecificationSchema, K::Identity, F::ProfileSpecificationSchemaIdentityMismatch),
        row(C::ProfileSpecificationSchema, K::Revision, F::ProfileSpecificationSchemaRevisionUnsupported),
        row(C::ProfileSpecificationSchema, K::FileDigest, F::ProfileSpecificationSchemaDigestMismatch),
        row(C::ProfileSpecificationSchema, K::MetaSchema, F::ProfileSpecificationSchemaInvalid),
        row(C::ProfileSpecification, K::Presence, F::ProfileSpecificationMissing),
        row(C::ProfileSpecification, K::Envelope, F::ProfileSpecificationBodySchemaInvalid),
        row(C::ProfileSpecification, K::Identity, F::ProfileSpecificationIdentityMismatch),
        row(C::ProfileSpecification, K::Revision, F::ProfileSpecificationRevisionUnsupported),
        row(C::ProfileSpecification, K::FileDigest, F::ProfileSpecificationDigestMismatch),
        row(C::ProfileSpecification, K::BodySchema, F::ProfileSpecificationBodySchemaInvalid),
        row(C::ProfileSpecification, K::SemanticDigest, F::ProfileSpecificationDigestMismatch),
        row(C::ConfigurationSchemaAuthoritySet, K::Presence, F::ConfigurationSchemaAuthoritySetMissing),
        row(C::ConfigurationSchemaAuthoritySet, K::Envelope, F::ConfigurationSchemaAuthoritySetSchemaInvalid),
        row(C::ConfigurationSchemaAuthoritySet, K::Identity, F::ConfigurationSchemaAuthoritySetIdentityMismatch),
        row(C::ConfigurationSchemaAuthoritySet, K::Revision, F::ConfigurationSchemaAuthoritySetRevisionUnsupported),
        row(C::ConfigurationSchemaAuthoritySet, K::FileDigest, F::ConfigurationSchemaAuthoritySetDigestMismatch),
        row(C::ConfigurationSchemaMember, K::Identity, F::ConfigurationSchemaMemberIdentityMismatch),
        row(C::ConfigurationSchemaMember, K::Revision, F::ConfigurationSchemaMemberRevisionUnsupported),
        row(C::ConfigurationSchemaMember, K::Dialect, F::ConfigurationSchemaMemberDialectUnsupported),
        row(C::ConfigurationSchemaMember, K::FileDigest, F::ConfigurationSchemaMemberDigestMismatch),
        row(C::ConfigurationSchemaMember, K::MetaSchema, F::ConfigurationSchemaMemberSchemaInvalid),
        row(C::ResolverCapabilitySpecification, K::Presence, F::ResolverCapabilitySpecificationMissing),
        row(C::ResolverCapabilitySpecification, K::Envelope, F::ResolverCapabilitySpecificationSchemaInvalid),
        row(C::ResolverCapabilitySpecification, K::Identity, F::ResolverCapabilitySpecificationIdentityMismatch),
        row(C::ResolverCapabilitySpecification, K::Revision, F::ResolverCapabilitySpecificationRevisionUnsupported),
        row(C::ResolverCapabilitySpecification, K::FileDigest, F::ResolverCapabilitySpecificationDigestMismatch),
        row(C::ResolverCapabilitySpecification, K::BodySchema, F::ResolverCapabilitySpecificationSchemaInvalid),
        row(C::RegistryPackage, K::Presence, F::RegistryPackageIncomplete),
        row(C::RegistryPackage, K::Envelope, F::RegistryPackageSchemaInvalid),
        row(C::RegistryPackage, K::Identity, F::RegistryPackageIdentityMismatch),
        row(C::RegistryPackage, K::Revision, F::RegistryPackageRevisionUnsupported),
        row(C::RegistryPackage, K::FileDigest, F::RegistryPackageDigestMismatch),
        row(C::RegistryMember, K::Presence, F::RegistryPackageIncomplete),
        row(C::RegistryMember, K::Identity, F::RegistryMemberIdentityMismatch),
        row(C::RegistryMember, K::Revision, F::RegistryMemberRevisionUnsupported),
        row(C::RegistryMember, K::FileDigest, F::RegistryMemberDigestMismatch),
        row(C::RegistryMember, K::MemberSchema, F::RegistryMemberSchemaInvalid),
        row(C::RegistryPackage, K::CrossReferences, F::RegistryCrossReferenceInvalid),
        row(C::All, K::CrossAuthorityRules, F::ConstructionCrossAuthorityReferenceInvalid),
        row(C::All, K::ImplementedAuthority, F::ConstructionAuthorityNotImplemented),
    ]
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    fn spelled<T: Serialize>(value: T) -> String {
        serde_json::to_value(value)
            .unwrap()
            .as_str()
            .unwrap()
            .to_owned()
    }

    #[test]
    fn the_precedence_has_015s_forty_three_rows_and_thirty_eight_reasons() {
        assert_eq!(PRECEDENCE.len(), 43);
        let reasons = PRECEDENCE
            .iter()
            .map(|row| row.reason)
            .collect::<BTreeSet<_>>();
        assert_eq!(reasons.len(), 38);
    }

    #[test]
    fn every_reason_is_spelled_as_015_writes_it() {
        assert_eq!(
            spelled(ConstructionFailure::ConstructionInputLimitExceeded),
            "construction-input-limit-exceeded"
        );
        assert_eq!(
            spelled(ConstructionFailure::ProfileSpecificationSchemaRevisionUnsupported),
            "profile-specification-schema-revision-unsupported"
        );
        assert_eq!(
            spelled(ConstructionFailure::ConstructionCrossAuthorityReferenceInvalid),
            "construction-cross-authority-reference-invalid"
        );
        assert_eq!(
            spelled(ConstructionFailure::ConstructionAuthorityNotImplemented),
            "construction-authority-not-implemented"
        );
        // Every component's own reasons name it, and only the first and the
        // last two rows speak for all components at once.
        for row in PRECEDENCE {
            let reason = spelled(row.reason);
            let prefix = match row.component {
                Component::All => "construction-",
                Component::ProfileSpecificationSchema => "profile-specification-schema-",
                Component::ProfileSpecification => "profile-specification-",
                Component::ConfigurationSchemaAuthoritySet => "configuration-schema-authority-set-",
                Component::ConfigurationSchemaMember => "configuration-schema-member-",
                Component::ResolverCapabilitySpecification => "resolver-capability-specification-",
                Component::RegistryPackage | Component::RegistryMember => "registry-",
            };
            assert!(reason.starts_with(prefix), "{reason}");
        }
    }

    #[test]
    fn the_rows_keep_015s_order_of_components_and_checks() {
        let components = PRECEDENCE
            .iter()
            .map(|row| row.component)
            .collect::<Vec<_>>();
        // Each component's rows are contiguous, in 015's order, except the
        // registry cross-references, which follow the members they relate.
        let mut order = Vec::new();
        for component in &components {
            if order.last() != Some(component) {
                order.push(*component);
            }
        }
        assert_eq!(
            order,
            [
                Component::All,
                Component::ProfileSpecificationSchema,
                Component::ProfileSpecification,
                Component::ConfigurationSchemaAuthoritySet,
                Component::ConfigurationSchemaMember,
                Component::ResolverCapabilitySpecification,
                Component::RegistryPackage,
                Component::RegistryMember,
                Component::RegistryPackage,
                Component::All,
            ]
        );
        // Presence comes first for every component that can be absent, and
        // the bounds come before everything.
        assert_eq!(PRECEDENCE[0].check, Check::ConstructionBounds);
        for index in [1, 7, 14, 24, 30, 35] {
            assert_eq!(
                PRECEDENCE[index].check,
                Check::Presence,
                "row {}",
                index + 1
            );
        }
        assert_eq!(PRECEDENCE[42].check, Check::ImplementedAuthority);
    }

    #[test]
    fn a_row_is_written_with_its_three_cells() {
        assert_eq!(
            serde_json::to_value(PRECEDENCE[21]).unwrap(),
            serde_json::json!({
                "component": "configurationSchemaMember",
                "check": "dialect",
                "reason": "configuration-schema-member-dialect-unsupported"
            })
        );
    }
}
