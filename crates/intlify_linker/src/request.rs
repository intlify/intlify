// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable admission boundary for one complete link invocation.
//!
//! This module coordinates defensive artifact revalidation, request aggregate
//! limits, scope binding and resolution, delivery-unit binding, and
//! cross-input domain consistency. It does not perform selector matching,
//! construct findings, or build bundle plans.

use std::collections::{BTreeMap, BTreeSet};

use intlify_contract::{
    ArtifactContractError, ArtifactLimitEvidence, CatalogKeyDomain, LinkLimitCounter,
    LinkLimitEvidence, LinkLimitSubject, LinkLimits, MessageDefinitionArtifact,
    MessageReferenceArtifact, ReferenceArtifactIdentity,
};

use crate::validation::{arithmetic_overflow, check_exact, check_first_over, usize_count};
use crate::{
    ArtifactContractSubject, ArtifactKind, DeliveryUnitGraph, InvalidRequestError,
    LinkOperationalError, LinkPolicy, ResolvedCatalogScopeId, ResolvedLinkPolicy,
    ResolvedScopeCompletenessTable, ScopeCompletenessTable, ScopeMappingTable, ScopeUse,
    UnsupportedContractError,
};

/// Fully admitted immutable inputs for one semantic link operation.
#[derive(Debug)]
pub struct LinkRequest<'a> {
    reference_artifacts: &'a [MessageReferenceArtifact],
    definition_artifacts: &'a [MessageDefinitionArtifact],
    policy: &'a LinkPolicy,
    resolved_policy: ResolvedLinkPolicy,
    scope_mappings: &'a ScopeMappingTable,
    scope_completeness: &'a ScopeCompletenessTable,
    resolved_scope_completeness: ResolvedScopeCompletenessTable,
    delivery_graph: &'a DeliveryUnitGraph,
    limits: &'a LinkLimits,
}

impl<'a> LinkRequest<'a> {
    /// Admit one complete request or return one fail-complete operational error.
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        reference_artifacts: &'a [MessageReferenceArtifact],
        definition_artifacts: &'a [MessageDefinitionArtifact],
        policy: &'a LinkPolicy,
        scope_mappings: &'a ScopeMappingTable,
        scope_completeness: &'a ScopeCompletenessTable,
        delivery_graph: &'a DeliveryUnitGraph,
        limits: &'a LinkLimits,
    ) -> Result<Self, LinkOperationalError> {
        validate_reference_artifacts(reference_artifacts, limits)?;
        validate_definition_artifacts(definition_artifacts, limits)?;
        policy.revalidate(limits)?;
        scope_mappings.revalidate(limits)?;
        delivery_graph.revalidate(limits)?;

        validate_scope_inventory(scope_mappings, scope_completeness)?;
        validate_scope_bindings(
            reference_artifacts,
            definition_artifacts,
            policy,
            scope_mappings,
        )?;
        validate_delivery_bindings(reference_artifacts, policy, delivery_graph)?;
        validate_resolved_domains(
            reference_artifacts,
            definition_artifacts,
            policy,
            scope_mappings,
        )?;

        let resolved_policy = ResolvedLinkPolicy::resolve(policy, scope_mappings)?;
        let resolved_scope_completeness =
            ResolvedScopeCompletenessTable::resolve(scope_completeness, scope_mappings);

        Ok(Self {
            reference_artifacts,
            definition_artifacts,
            policy,
            resolved_policy,
            scope_mappings,
            scope_completeness,
            resolved_scope_completeness,
            delivery_graph,
            limits,
        })
    }

    /// Return admitted reference artifacts in caller-owned semantic order.
    #[must_use]
    pub const fn reference_artifacts(&self) -> &[MessageReferenceArtifact] {
        self.reference_artifacts
    }

    /// Return admitted definition artifacts in caller-owned semantic order.
    #[must_use]
    pub const fn definition_artifacts(&self) -> &[MessageDefinitionArtifact] {
        self.definition_artifacts
    }

    /// Return the complete canonical pre-mapping policy.
    #[must_use]
    pub const fn policy(&self) -> &LinkPolicy {
        self.policy
    }

    /// Return the canonical post-mapping policy.
    #[must_use]
    pub const fn resolved_policy(&self) -> &ResolvedLinkPolicy {
        &self.resolved_policy
    }

    /// Return the exact checked one-hop mapping table.
    #[must_use]
    pub const fn scope_mappings(&self) -> &ScopeMappingTable {
        self.scope_mappings
    }

    /// Return execution completeness before scope resolution.
    #[must_use]
    pub const fn scope_completeness(&self) -> &ScopeCompletenessTable {
        self.scope_completeness
    }

    /// Return execution completeness after uniform scope resolution.
    #[must_use]
    pub const fn resolved_scope_completeness(&self) -> &ResolvedScopeCompletenessTable {
        &self.resolved_scope_completeness
    }

    /// Return the complete checked delivery graph.
    #[must_use]
    pub const fn delivery_graph(&self) -> &DeliveryUnitGraph {
        self.delivery_graph
    }

    /// Return the immutable request-local effective limits.
    #[must_use]
    pub const fn limits(&self) -> &LinkLimits {
        self.limits
    }
}

fn validate_reference_artifacts(
    artifacts: &[MessageReferenceArtifact],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    check_first_over(
        LinkLimitCounter::ReferenceArtifacts,
        LinkLimitSubject::Request,
        usize_count(artifacts.len()),
        limits,
    )?;

    let mut canonical = artifacts.iter().collect::<Vec<_>>();
    canonical.sort_by(|left, right| left.identity().cmp(right.identity()));
    for artifact in &canonical {
        artifact.revalidate(limits).map_err(|error| {
            map_artifact_error(
                ArtifactKind::Reference,
                ArtifactContractSubject::Reference(artifact.identity().clone()),
                LinkLimitSubject::ReferenceArtifactGroup(artifact.identity().clone()),
                error,
            )
        })?;
    }

    aggregate_reference_counter(
        &canonical,
        LinkLimitCounter::ReferenceIdentityBytesTotal,
        limits,
        |artifact| reference_identity_bytes(artifact.identity()),
    )?;
    aggregate_reference_counter(
        &canonical,
        LinkLimitCounter::ReferenceRecordsTotal,
        limits,
        |artifact| usize_count(artifact.references().len()),
    )?;
    aggregate_reference_counter(
        &canonical,
        LinkLimitCounter::ReferenceArtifactDecodedBytesTotal,
        limits,
        MessageReferenceArtifact::decoded_bytes,
    )?;

    if let Some(pair) = canonical
        .windows(2)
        .find(|pair| pair[0].identity() == pair[1].identity())
    {
        return Err(
            InvalidRequestError::DuplicateReferenceArtifact(pair[0].identity().clone()).into(),
        );
    }
    Ok(())
}

fn validate_definition_artifacts(
    artifacts: &[MessageDefinitionArtifact],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    check_first_over(
        LinkLimitCounter::DefinitionArtifacts,
        LinkLimitSubject::Request,
        usize_count(artifacts.len()),
        limits,
    )?;

    let mut canonical = artifacts.iter().collect::<Vec<_>>();
    canonical.sort_by(|left, right| left.source().cmp(right.source()));
    for artifact in &canonical {
        artifact.revalidate(limits).map_err(|error| {
            let limit_subject = definition_artifact_limit_subject(artifact, &error, limits);
            map_artifact_error(
                ArtifactKind::Definition,
                ArtifactContractSubject::Definition(artifact.source().clone()),
                limit_subject,
                error,
            )
        })?;
    }

    aggregate_definition_counter(
        &canonical,
        LinkLimitCounter::DefinitionsTotal,
        limits,
        |artifact| usize_count(artifact.definitions().len()),
    )?;
    aggregate_definition_counter(
        &canonical,
        LinkLimitCounter::DefinitionArtifactDecodedBytesTotal,
        limits,
        MessageDefinitionArtifact::decoded_bytes,
    )?;

    if let Some(pair) = canonical
        .windows(2)
        .find(|pair| pair[0].source() == pair[1].source())
    {
        return Err(
            InvalidRequestError::DuplicateDefinitionArtifact(pair[0].source().clone()).into(),
        );
    }
    Ok(())
}

fn aggregate_reference_counter(
    artifacts: &[&MessageReferenceArtifact],
    counter: LinkLimitCounter,
    limits: &LinkLimits,
    charge: impl Fn(&MessageReferenceArtifact) -> u64,
) -> Result<(), LinkOperationalError> {
    let mut total = 0_u64;
    let mut index = 0;
    while index < artifacts.len() {
        let identity = artifacts[index].identity();
        let subject = LinkLimitSubject::ReferenceArtifactGroup(identity.clone());
        let mut group_total = 0_u64;
        while index < artifacts.len() && artifacts[index].identity() == identity {
            group_total = group_total
                .checked_add(charge(artifacts[index]))
                .ok_or_else(|| arithmetic_overflow(counter, subject.clone(), limits))?;
            index += 1;
        }
        total = total
            .checked_add(group_total)
            .ok_or_else(|| arithmetic_overflow(counter, subject.clone(), limits))?;
        check_exact(counter, subject, total, limits)?;
    }
    Ok(())
}

fn aggregate_definition_counter(
    artifacts: &[&MessageDefinitionArtifact],
    counter: LinkLimitCounter,
    limits: &LinkLimits,
    charge: impl Fn(&MessageDefinitionArtifact) -> u64,
) -> Result<(), LinkOperationalError> {
    let mut total = 0_u64;
    let mut index = 0;
    while index < artifacts.len() {
        let source = artifacts[index].source();
        let subject = LinkLimitSubject::DefinitionArtifactGroup(source.clone());
        let mut group_total = 0_u64;
        while index < artifacts.len() && artifacts[index].source() == source {
            group_total = group_total
                .checked_add(charge(artifacts[index]))
                .ok_or_else(|| arithmetic_overflow(counter, subject.clone(), limits))?;
            index += 1;
        }
        total = total
            .checked_add(group_total)
            .ok_or_else(|| arithmetic_overflow(counter, subject.clone(), limits))?;
        check_exact(counter, subject, total, limits)?;
    }
    Ok(())
}

fn reference_identity_bytes(identity: &ReferenceArtifactIdentity) -> u64 {
    identity
        .segments()
        .iter()
        .map(|segment| usize_count(segment.as_str().len()))
        .sum()
}

fn map_artifact_error(
    kind: ArtifactKind,
    subject: ArtifactContractSubject,
    limit_subject: LinkLimitSubject,
    error: ArtifactContractError,
) -> LinkOperationalError {
    match error {
        ArtifactContractError::InvalidArtifact(violation) => {
            InvalidRequestError::ArtifactContract { subject, violation }.into()
        }
        ArtifactContractError::UnsupportedVersion(evidence) => {
            LinkOperationalError::UnsupportedContract(UnsupportedContractError::ArtifactVersion {
                kind,
                subject,
                evidence,
            })
        }
        ArtifactContractError::Limit(evidence) => map_artifact_limit(limit_subject, evidence),
    }
}

fn map_artifact_limit(
    subject: LinkLimitSubject,
    evidence: ArtifactLimitEvidence,
) -> LinkOperationalError {
    LinkLimitEvidence::try_new(
        evidence.counter(),
        subject,
        evidence.effective_limit(),
        evidence.observation(),
    )
    .expect("typed artifact and linker subjects share this limit boundary")
    .into()
}

fn definition_artifact_limit_subject(
    artifact: &MessageDefinitionArtifact,
    error: &ArtifactContractError,
    limits: &LinkLimits,
) -> LinkLimitSubject {
    let group = LinkLimitSubject::DefinitionArtifactGroup(artifact.source().clone());
    let ArtifactContractError::Limit(evidence) = error else {
        return group;
    };
    let path = artifact.source().path();
    let primary_failed = match evidence.counter() {
        LinkLimitCounter::PathSegments => {
            usize_count(path.segments().len())
                > limits.effective_limit(LinkLimitCounter::PathSegments)
        }
        LinkLimitCounter::PathSegmentBytes => path.segments().iter().any(|segment| {
            usize_count(segment.as_str().len())
                > limits.effective_limit(LinkLimitCounter::PathSegmentBytes)
        }),
        LinkLimitCounter::PathBytes => {
            let limit = limits.effective_limit(LinkLimitCounter::PathBytes);
            let mut total = 0_u64;
            path.segments().iter().any(|segment| {
                total = total
                    .checked_add(usize_count(segment.as_str().len()))
                    .expect("checked path components cannot overflow u64");
                total > limit
            })
        }
        _ => false,
    };
    if primary_failed {
        LinkLimitSubject::DefinitionArtifactEnvelope
    } else {
        group
    }
}

fn validate_scope_inventory(
    mappings: &ScopeMappingTable,
    completeness: &ScopeCompletenessTable,
) -> Result<(), LinkOperationalError> {
    if mappings.declared_scopes().iter().ne(completeness.scopes()) {
        return Err(InvalidRequestError::MappingInventoryMismatch.into());
    }
    Ok(())
}

fn validate_scope_bindings(
    reference_artifacts: &[MessageReferenceArtifact],
    definition_artifacts: &[MessageDefinitionArtifact],
    policy: &LinkPolicy,
    mappings: &ScopeMappingTable,
) -> Result<(), LinkOperationalError> {
    let declared = mappings.declared_scopes().iter().collect::<BTreeSet<_>>();
    let mut uses = Vec::new();
    for artifact in reference_artifacts {
        for reference in artifact.references() {
            uses.push((reference.scope(), ScopeUse::Reference));
        }
    }
    for artifact in definition_artifacts {
        for definition in artifact.definitions() {
            uses.push((definition.scope(), ScopeUse::Definition));
        }
    }
    for root in policy.configured_roots() {
        uses.push((root.scope(), ScopeUse::ConfiguredRoot));
    }
    uses.sort_by(|left, right| left.0.cmp(right.0).then(left.1.cmp(&right.1)));

    if let Some((scope, usage)) = uses
        .into_iter()
        .find(|(scope, _)| !declared.contains(scope))
    {
        return Err(InvalidRequestError::UndeclaredScope {
            usage,
            scope: scope.clone(),
        }
        .into());
    }
    Ok(())
}

fn validate_delivery_bindings(
    reference_artifacts: &[MessageReferenceArtifact],
    policy: &LinkPolicy,
    graph: &DeliveryUnitGraph,
) -> Result<(), LinkOperationalError> {
    let mut artifacts = reference_artifacts.iter().collect::<Vec<_>>();
    artifacts.sort_by(|left, right| left.identity().cmp(right.identity()));
    if let Some(artifact) = artifacts
        .into_iter()
        .find(|artifact| !graph.contains(artifact.delivery_unit()))
    {
        return Err(InvalidRequestError::MissingReferenceDeliveryUnit {
            artifact: artifact.identity().clone(),
            delivery_unit: artifact.delivery_unit().clone(),
        }
        .into());
    }
    if graph.nodes().is_empty() && !policy.configured_roots().is_empty() {
        return Err(InvalidRequestError::ConfiguredRootRequiresDeliveryUnit.into());
    }
    Ok(())
}

fn validate_resolved_domains(
    reference_artifacts: &[MessageReferenceArtifact],
    definition_artifacts: &[MessageDefinitionArtifact],
    policy: &LinkPolicy,
    mappings: &ScopeMappingTable,
) -> Result<(), LinkOperationalError> {
    let mut domains: BTreeMap<ResolvedCatalogScopeId, BTreeSet<CatalogKeyDomain>> = BTreeMap::new();
    for artifact in reference_artifacts {
        for reference in artifact.references() {
            domains
                .entry(mappings.resolve(reference.scope()))
                .or_default()
                .insert(reference.domain());
        }
    }
    for artifact in definition_artifacts {
        for definition in artifact.definitions() {
            domains
                .entry(mappings.resolve(definition.scope()))
                .or_default()
                .insert(definition.domain());
        }
    }
    for root in policy.configured_roots() {
        domains
            .entry(mappings.resolve(root.scope()))
            .or_default()
            .insert(root.domain());
    }

    for (scope, domains) in domains {
        let mut domains = domains.into_iter();
        let Some(first) = domains.next() else {
            continue;
        };
        if let Some(second) = domains.next() {
            return Err(InvalidRequestError::ResolvedDomainConflict {
                scope,
                first,
                second,
            }
            .into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain, CatalogScopeId,
        CatalogScopeName, DeliveryUnitId, FingerprintAlgorithm, FingerprintDigest,
        InputFingerprint, LinkLimitCounter, LinkLimitObservation, LinkLimitSubject, LinkLimits,
        Locale, MessageDefinition, MessageDefinitionArtifact, MessagePayload, MessageReference,
        MessageReferenceArtifact, MessageSelector, PortablePathSegment, PortableRelativePath,
        ProducerId, ProducerIdentity, ProducerRevision, ReasonText, ReferenceArtifactIdentity,
        ReferenceArtifactSegment, SourceDocumentIdentity,
    };

    use super::LinkRequest;
    use crate::{
        ConfiguredRoot, DeliveryUnitGraph, DynamicReferenceMode, InputCompleteness,
        InvalidRequestError, LinkOperationalError, LinkPolicy, PlacementPolicy, ScopeCompleteness,
        ScopeCompletenessTable, ScopeMapping, ScopeMappingTable,
    };

    fn scope(name: &str) -> CatalogScopeId {
        CatalogScopeId::new(
            ArtifactNamespace::Project,
            CatalogScopeName::try_new(name).unwrap(),
        )
    }

    fn producer() -> ProducerIdentity {
        ProducerIdentity::new(
            ProducerId::try_new("dev.example/linker-test").unwrap(),
            ProducerRevision::try_new("test").unwrap(),
        )
    }

    fn reference_artifact(
        identity: &str,
        delivery_unit: DeliveryUnitId,
        references: Vec<MessageReference>,
    ) -> MessageReferenceArtifact {
        MessageReferenceArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer(),
            ReferenceArtifactIdentity::try_new(
                ArtifactNamespace::Project,
                vec![ReferenceArtifactSegment::try_new(identity).unwrap()],
            )
            .unwrap(),
            delivery_unit,
            references,
            &LinkLimits::default(),
        )
        .unwrap()
    }

    fn definition_artifact(
        source: &str,
        definitions: Vec<MessageDefinition>,
    ) -> MessageDefinitionArtifact {
        let path =
            PortableRelativePath::try_new(vec![PortablePathSegment::try_new(source).unwrap()])
                .unwrap();
        MessageDefinitionArtifact::try_new(
            ArtifactVersion::DRAFT_V0_1,
            producer(),
            SourceDocumentIdentity::new(ArtifactNamespace::Project, path),
            Vec::new(),
            InputFingerprint::new(
                FingerprintAlgorithm::Blake3_256,
                FingerprintDigest::new([0; 32]),
            ),
            definitions,
            &LinkLimits::default(),
        )
        .unwrap()
    }

    fn policy(roots: Vec<crate::ConfiguredRoot>) -> LinkPolicy {
        LinkPolicy::try_new(
            vec![Locale::try_new("en").unwrap()],
            roots,
            DynamicReferenceMode::Compat,
            PlacementPolicy::Duplicate,
            &LinkLimits::default(),
        )
        .unwrap()
    }

    fn configured_root(scope: CatalogScopeId, reason: Option<&str>) -> ConfiguredRoot {
        ConfiguredRoot::try_new(
            scope,
            CatalogKeyDomain::JsonPointer,
            MessageSelector::Exact(
                CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/message").unwrap(),
            ),
            reason.map(|reason| ReasonText::try_new(reason).unwrap()),
        )
        .unwrap()
    }

    fn completeness(scopes: &[CatalogScopeId]) -> ScopeCompletenessTable {
        ScopeCompletenessTable::try_new(
            scopes,
            scopes
                .iter()
                .cloned()
                .map(|scope| {
                    ScopeCompleteness::try_new(
                        scope,
                        InputCompleteness::Closed,
                        InputCompleteness::Closed,
                    )
                    .unwrap()
                })
                .collect(),
        )
        .unwrap()
    }

    #[test]
    fn request_accepts_empty_and_custom_many_to_one_scope_mapping() {
        let a = scope("a");
        let b = scope("b");
        let mappings = ScopeMappingTable::try_new(
            &[a.clone(), b.clone()],
            vec![ScopeMapping::new(a.clone(), b.clone())],
            &LinkLimits::default(),
        )
        .unwrap();
        let completeness = completeness(&[a, b]);
        let policy = policy(Vec::new());
        let graph =
            DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &LinkLimits::default()).unwrap();
        let limits = LinkLimits::default();

        let request =
            LinkRequest::try_new(&[], &[], &policy, &mappings, &completeness, &graph, &limits)
                .unwrap();
        assert_eq!(request.resolved_scope_completeness().entries().len(), 1);
    }

    #[test]
    fn request_rejects_missing_delivery_unit_without_partial_result() {
        let app = scope("app");
        let mappings =
            ScopeMappingTable::empty(std::slice::from_ref(&app), &LinkLimits::default()).unwrap();
        let completeness = completeness(&[app]);
        let policy = policy(Vec::new());
        let graph =
            DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &LinkLimits::default()).unwrap();
        let artifacts = [reference_artifact(
            "source",
            DeliveryUnitId::main(),
            Vec::new(),
        )];

        assert!(matches!(
            LinkRequest::try_new(
                &artifacts,
                &[],
                &policy,
                &mappings,
                &completeness,
                &graph,
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::MissingReferenceDeliveryUnit { .. }
            ))
        ));
    }

    #[test]
    fn request_rejects_mapped_cross_domain_inputs() {
        let a = scope("a");
        let b = scope("b");
        let mappings = ScopeMappingTable::try_new(
            &[a.clone(), b.clone()],
            vec![ScopeMapping::new(a.clone(), b.clone())],
            &LinkLimits::default(),
        )
        .unwrap();
        let completeness = completeness(&[a.clone(), b.clone()]);
        let policy = policy(Vec::new());
        let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();
        let references = [reference_artifact(
            "source",
            DeliveryUnitId::main(),
            vec![MessageReference::try_new(
                a,
                CatalogKeyDomain::JsonPointer,
                MessageSelector::Exact(
                    CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/message").unwrap(),
                ),
                None,
                None,
            )
            .unwrap()],
        )];
        let definitions = [definition_artifact(
            "messages.json",
            vec![MessageDefinition::try_new(
                b,
                CatalogKeyDomain::YamlTypedPath,
                CatalogKey::try_new(CatalogKeyDomain::YamlTypedPath, "/k:str:message").unwrap(),
                Locale::try_new("en").unwrap(),
                MessagePayload::try_new("hello").unwrap(),
                intlify_contract::EntryReference::new(
                    intlify_contract::EntryStructuralPath::try_new("message").unwrap(),
                    0,
                ),
            )
            .unwrap()],
        )];

        assert!(matches!(
            LinkRequest::try_new(
                &references,
                &definitions,
                &policy,
                &mappings,
                &completeness,
                &graph,
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::ResolvedDomainConflict { .. }
            ))
        ));
    }

    #[test]
    fn request_aggregate_limit_precedes_duplicate_identity() {
        let app = scope("app");
        let mappings =
            ScopeMappingTable::empty(std::slice::from_ref(&app), &LinkLimits::default()).unwrap();
        let completeness = completeness(&[app]);
        let policy = policy(Vec::new());
        let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();
        let artifact = reference_artifact("source", DeliveryUnitId::main(), Vec::new());
        let artifacts = [artifact.clone(), artifact];
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReferenceIdentityBytesTotal, 5)
            .unwrap();

        let error = LinkRequest::try_new(
            &artifacts,
            &[],
            &policy,
            &mappings,
            &completeness,
            &graph,
            &limits,
        )
        .unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(
            evidence.counter(),
            LinkLimitCounter::ReferenceIdentityBytesTotal
        );
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(12));
    }

    #[test]
    fn definition_primary_path_limit_uses_envelope_subject() {
        let app = scope("app");
        let mappings =
            ScopeMappingTable::empty(std::slice::from_ref(&app), &LinkLimits::default()).unwrap();
        let completeness = completeness(&[app]);
        let policy = policy(Vec::new());
        let graph =
            DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &LinkLimits::default()).unwrap();
        let definitions = [definition_artifact("messages.json", Vec::new())];
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathSegments, 0)
            .unwrap();

        let error = LinkRequest::try_new(
            &[],
            &definitions,
            &policy,
            &mappings,
            &completeness,
            &graph,
            &limits,
        )
        .unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::PathSegments);
        assert_eq!(
            evidence.subject(),
            &LinkLimitSubject::DefinitionArtifactEnvelope
        );
    }

    #[test]
    fn request_rejects_duplicate_artifact_identities_after_aggregate_admission() {
        let app = scope("app");
        let mappings =
            ScopeMappingTable::empty(std::slice::from_ref(&app), &LinkLimits::default()).unwrap();
        let completeness = completeness(&[app]);
        let policy = policy(Vec::new());
        let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();
        let reference = reference_artifact("source", DeliveryUnitId::main(), Vec::new());
        let references = [reference.clone(), reference];
        let limits = LinkLimits::default();

        assert!(matches!(
            LinkRequest::try_new(
                &references,
                &[],
                &policy,
                &mappings,
                &completeness,
                &graph,
                &limits,
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::DuplicateReferenceArtifact(_)
            ))
        ));

        let definition = definition_artifact("messages.json", Vec::new());
        let definitions = [definition.clone(), definition];
        assert!(matches!(
            LinkRequest::try_new(
                &[],
                &definitions,
                &policy,
                &mappings,
                &completeness,
                &graph,
                &limits,
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::DuplicateDefinitionArtifact(_)
            ))
        ));
    }

    #[test]
    fn request_requires_one_shared_declared_scope_inventory() {
        let app = scope("app");
        let other = scope("other");
        let mappings =
            ScopeMappingTable::empty(std::slice::from_ref(&app), &LinkLimits::default()).unwrap();
        let completeness = completeness(&[other]);
        let policy = policy(Vec::new());
        let graph =
            DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &LinkLimits::default()).unwrap();
        let limits = LinkLimits::default();

        assert_eq!(
            LinkRequest::try_new(&[], &[], &policy, &mappings, &completeness, &graph, &limits,)
                .unwrap_err(),
            LinkOperationalError::InvalidRequest(InvalidRequestError::MappingInventoryMismatch)
        );
    }

    #[test]
    fn request_collapses_only_equal_post_mapping_root_evidence() {
        let a = scope("a");
        let b = scope("b");
        let mappings = ScopeMappingTable::try_new(
            &[a.clone(), b.clone()],
            vec![ScopeMapping::new(a.clone(), b.clone())],
            &LinkLimits::default(),
        )
        .unwrap();
        let completeness = completeness(&[a.clone(), b.clone()]);
        let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();
        let limits = LinkLimits::default();

        let equal_policy = policy(vec![
            configured_root(a.clone(), Some("keep")),
            configured_root(b.clone(), Some("keep")),
        ]);
        let equal = LinkRequest::try_new(
            &[],
            &[],
            &equal_policy,
            &mappings,
            &completeness,
            &graph,
            &limits,
        )
        .unwrap();
        assert_eq!(equal.resolved_policy().configured_roots().len(), 1);

        let conflicting_policy = policy(vec![
            configured_root(a, Some("first")),
            configured_root(b, Some("second")),
        ]);
        assert!(matches!(
            LinkRequest::try_new(
                &[],
                &[],
                &conflicting_policy,
                &mappings,
                &completeness,
                &graph,
                &limits,
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::ResolvedConfiguredRootConflict(_)
            ))
        ));
    }

    #[test]
    fn configured_roots_require_at_least_one_real_delivery_root() {
        let app = scope("app");
        let mappings =
            ScopeMappingTable::empty(std::slice::from_ref(&app), &LinkLimits::default()).unwrap();
        let completeness = completeness(std::slice::from_ref(&app));
        let policy = policy(vec![configured_root(app, None)]);
        let graph =
            DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &LinkLimits::default()).unwrap();
        let limits = LinkLimits::default();

        assert_eq!(
            LinkRequest::try_new(&[], &[], &policy, &mappings, &completeness, &graph, &limits,)
                .unwrap_err(),
            LinkOperationalError::InvalidRequest(
                InvalidRequestError::ConfiguredRootRequiresDeliveryUnit
            )
        );
    }
}
