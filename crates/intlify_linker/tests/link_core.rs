// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_contract::{
    ArtifactNamespace, ArtifactVersion, CatalogKey, CatalogKeyDomain, CatalogKeyPattern,
    CatalogKeyPrefix, CatalogScopeId, CatalogScopeName, DeliveryUnitId, DeliveryUnitSegment,
    EntryReference, EntryStructuralPath, FingerprintAlgorithm, FingerprintDigest, InputFingerprint,
    LinkLimitCounter, LinkLimitObservation, LinkLimits, Locale, MessageDefinition,
    MessageDefinitionArtifact, MessagePayload, MessageReference, MessageReferenceArtifact,
    MessageSelector, PortablePathSegment, PortableRelativePath, ProducerId, ProducerIdentity,
    ProducerRevision, ReferenceArtifactIdentity, ReferenceArtifactSegment, SourceDocumentIdentity,
};
use intlify_linker::{
    link, ConfiguredRoot, DegradedAnalysisFinding, DeliveryUnitEdge, DeliveryUnitGraph,
    DynamicReferenceMode, InputCompleteness, LinkFinding, LinkFindingKind, LinkFindingRecord,
    LinkOperationalError, LinkOutcome, LinkPolicy, LinkRequest, PartialReason, PlacementPolicy,
    ScopeCompleteness, ScopeCompletenessTable, ScopeMapping, ScopeMappingTable,
};

const DOMAIN: CatalogKeyDomain = CatalogKeyDomain::JsonPointer;

fn scope(name: &str) -> CatalogScopeId {
    CatalogScopeId::new(
        ArtifactNamespace::Project,
        CatalogScopeName::try_new(name).unwrap(),
    )
}

fn locale(value: &str) -> Locale {
    Locale::try_new(value).unwrap()
}

fn key(value: &str) -> CatalogKey {
    CatalogKey::try_new(DOMAIN, value).unwrap()
}

fn unit(value: &str) -> DeliveryUnitId {
    DeliveryUnitId::try_new(vec![DeliveryUnitSegment::try_new(value).unwrap()]).unwrap()
}

fn producer() -> ProducerIdentity {
    ProducerIdentity::new(
        ProducerId::try_new("dev.example/link-core-test").unwrap(),
        ProducerRevision::try_new("test").unwrap(),
    )
}

fn definition(
    scope: CatalogScopeId,
    key_value: &str,
    locale_value: &str,
    message: &str,
    occurrence: u32,
) -> MessageDefinition {
    MessageDefinition::try_new(
        scope,
        DOMAIN,
        key(key_value),
        locale(locale_value),
        MessagePayload::try_new(message).unwrap(),
        EntryReference::new(EntryStructuralPath::try_new(key_value).unwrap(), occurrence),
    )
    .unwrap()
}

fn definition_artifact(
    source: &str,
    definitions: Vec<MessageDefinition>,
) -> MessageDefinitionArtifact {
    let path =
        PortableRelativePath::try_new(vec![PortablePathSegment::try_new(source).unwrap()]).unwrap();
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

fn reference(scope: CatalogScopeId, selector: MessageSelector) -> MessageReference {
    MessageReference::try_new(scope, DOMAIN, selector, None, None).unwrap()
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

fn policy(
    locales: &[&str],
    roots: Vec<ConfiguredRoot>,
    dynamic_mode: DynamicReferenceMode,
) -> LinkPolicy {
    LinkPolicy::try_new(
        locales.iter().map(|value| locale(value)).collect(),
        roots,
        dynamic_mode,
        PlacementPolicy::Duplicate,
        &LinkLimits::default(),
    )
    .unwrap()
}

fn configured_root(scope: CatalogScopeId, selector: MessageSelector) -> ConfiguredRoot {
    ConfiguredRoot::try_new(scope, DOMAIN, selector, None).unwrap()
}

fn mappings(scopes: &[CatalogScopeId], entries: Vec<ScopeMapping>) -> ScopeMappingTable {
    ScopeMappingTable::try_new(scopes, entries, &LinkLimits::default()).unwrap()
}

fn completeness(
    scopes: &[CatalogScopeId],
    entries: Vec<(CatalogScopeId, InputCompleteness, InputCompleteness)>,
) -> ScopeCompletenessTable {
    let mut entries = entries
        .into_iter()
        .map(|(scope, definitions, references)| {
            ScopeCompleteness::try_new(scope, definitions, references).unwrap()
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| left.scope().cmp(right.scope()));
    ScopeCompletenessTable::try_new(scopes, entries, &LinkLimits::default()).unwrap()
}

fn closed_completeness(scopes: &[CatalogScopeId]) -> ScopeCompletenessTable {
    completeness(
        scopes,
        scopes
            .iter()
            .cloned()
            .map(|scope| (scope, InputCompleteness::Closed, InputCompleteness::Closed))
            .collect(),
    )
}

fn do_link(
    scope: CatalogScopeId,
    references: Vec<MessageReferenceArtifact>,
    definitions: Vec<MessageDefinitionArtifact>,
    policy: LinkPolicy,
    completeness: ScopeCompletenessTable,
    graph: DeliveryUnitGraph,
    limits: LinkLimits,
) -> Result<LinkOutcome, LinkOperationalError> {
    do_link_multi_scopes(
        &[scope],
        Vec::new(),
        references,
        definitions,
        policy,
        completeness,
        graph,
        limits,
    )
}

// Own the fixtures for the complete request-and-link call so tests can pass
// constructed values directly without introducing lifetime-only locals.
#[allow(clippy::needless_pass_by_value, clippy::too_many_arguments)]
fn do_link_multi_scopes(
    scopes: &[CatalogScopeId],
    mapping_entries: Vec<ScopeMapping>,
    references: Vec<MessageReferenceArtifact>,
    definitions: Vec<MessageDefinitionArtifact>,
    policy: LinkPolicy,
    completeness: ScopeCompletenessTable,
    graph: DeliveryUnitGraph,
    limits: LinkLimits,
) -> Result<LinkOutcome, LinkOperationalError> {
    let mappings = mappings(scopes, mapping_entries);
    let request = LinkRequest::try_new(
        &references,
        &definitions,
        &policy,
        &mappings,
        &completeness,
        &graph,
        &limits,
    )?;
    link(&request)
}

fn assert_limit(
    error: LinkOperationalError,
    counter: LinkLimitCounter,
    observation: LinkLimitObservation,
) {
    let LinkOperationalError::Limit(evidence) = error else {
        panic!("expected a limit error, got {error:?}");
    };
    assert_eq!(evidence.counter(), counter);
    assert_eq!(evidence.observation(), observation);
}

#[test]
fn exact_resolution_retains_exact_locale_snapshots_and_reports_unused_definitions() {
    let app = scope("app");
    let definitions = vec![
        definition_artifact(
            "en.json",
            vec![
                definition(app.clone(), "/used", "en", "Hello", 0),
                definition(app.clone(), "/unused", "en", "Old", 0),
            ],
        ),
        definition_artifact(
            "ja.json",
            vec![definition(app.clone(), "/used", "ja", "こんにちは", 0)],
        ),
    ];
    let references = vec![reference_artifact(
        "entry",
        DeliveryUnitId::main(),
        vec![reference(app.clone(), MessageSelector::Exact(key("/used")))],
    )];
    let outcome = do_link(
        app.clone(),
        references,
        definitions,
        policy(&["ja", "en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
        LinkLimits::default(),
    )
    .unwrap();
    assert!(!outcome.generation_blocked());
    assert_eq!(outcome.findings().len(), 1);
    assert_eq!(outcome.findings()[0].kind(), LinkFindingKind::UnusedMessage);
    let LinkFindingRecord::UnusedMessage(unused) = outcome.findings()[0].record() else {
        panic!("expected unused-message");
    };
    assert_eq!(unused.subject().key().as_str(), "/unused");
    assert_eq!(unused.subject().locale().as_str(), "en");

    let plans = outcome.bundle_plans().unwrap();
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].locale().as_str(), "en");
    assert_eq!(plans[1].locale().as_str(), "ja");
    for plan in plans {
        assert_eq!(plan.delivery_unit(), &DeliveryUnitId::main());
        assert_eq!(plan.messages().len(), 1);
        assert_eq!(plan.messages()[0].key().as_str(), "/used");
        assert_eq!(
            plan.messages()[0].definition_locale(),
            plan.locale(),
            "M0 retains only the exact locale snapshot in each requested-locale plan"
        );
    }
    assert_eq!(plans[0].messages()[0].message().as_str(), "Hello");
    assert_eq!(plans[1].messages()[0].message().as_str(), "こんにちは");
}

#[test]
fn fallback_blind_union_resolution_does_not_invent_a_requested_locale_snapshot() {
    let app = scope("app");
    let outcome = do_link(
        app.clone(),
        vec![reference_artifact(
            "entry",
            DeliveryUnitId::main(),
            vec![reference(
                app.clone(),
                MessageSelector::Exact(key("/message")),
            )],
        )],
        vec![definition_artifact(
            "en.json",
            vec![definition(app.clone(), "/message", "en", "English", 0)],
        )],
        policy(&["ja", "en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
        LinkLimits::default(),
    )
    .unwrap();
    assert!(outcome.findings().is_empty());
    let plans = outcome.bundle_plans().unwrap();
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].locale().as_str(), "en");
    assert_eq!(plans[0].messages().len(), 1);
    assert_eq!(plans[0].messages()[0].definition_locale().as_str(), "en");
    assert_eq!(plans[1].locale().as_str(), "ja");
    assert!(
        plans[1].messages().is_empty(),
        "fallback-blind linking must not copy the English snapshot into the Japanese plan"
    );
}

#[test]
fn unresolved_reference_retains_one_failure_per_canonical_production_locale() {
    let app = scope("app");
    let outcome = do_link(
        app.clone(),
        vec![reference_artifact(
            "entry",
            DeliveryUnitId::main(),
            vec![reference(
                app.clone(),
                MessageSelector::Exact(key("/missing")),
            )],
        )],
        Vec::new(),
        policy(&["ja", "en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
        LinkLimits::default(),
    )
    .unwrap();
    assert!(outcome.generation_blocked());
    assert_eq!(outcome.findings().len(), 1);
    let LinkFindingRecord::UnresolvedMessage(unresolved) = outcome.findings()[0].record() else {
        panic!("expected unresolved-message");
    };
    assert_eq!(unresolved.evidence().failures().len(), 2);
    for (failure, expected_locale) in unresolved.evidence().failures().iter().zip(["en", "ja"]) {
        assert_eq!(failure.requested_locale().as_str(), expected_locale);
        assert_eq!(failure.probed_locales(), [locale(expected_locale)]);
    }
}

#[test]
fn semantic_result_is_independent_of_artifact_and_definition_enumeration_order() {
    let app = scope("app");
    let first_definition =
        definition_artifact("a.json", vec![definition(app.clone(), "/a", "en", "A", 0)]);
    let second_definition =
        definition_artifact("b.json", vec![definition(app.clone(), "/b", "en", "B", 0)]);
    let first_reference = reference_artifact(
        "a",
        DeliveryUnitId::main(),
        vec![reference(app.clone(), MessageSelector::Exact(key("/a")))],
    );
    let second_reference = reference_artifact(
        "b",
        DeliveryUnitId::main(),
        vec![reference(app.clone(), MessageSelector::Exact(key("/b")))],
    );

    let do_link_with_order = |references, definitions| {
        do_link(
            app.clone(),
            references,
            definitions,
            policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
            closed_completeness(std::slice::from_ref(&app)),
            DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
            LinkLimits::default(),
        )
    };
    let forward = do_link_with_order(
        vec![first_reference.clone(), second_reference.clone()],
        vec![first_definition.clone(), second_definition.clone()],
    )
    .unwrap();
    let reverse = do_link_with_order(
        vec![second_reference, first_reference],
        vec![second_definition, first_definition],
    )
    .unwrap();

    assert_eq!(forward, reverse);
    let messages = forward.bundle_plans().unwrap()[0].messages();
    assert_eq!(
        messages
            .iter()
            .map(|message| message.key().as_str())
            .collect::<Vec<_>>(),
        ["/a", "/b"]
    );
}

#[test]
fn mapped_ambiguity_blocks_generation_and_suppresses_secondary_findings() {
    let source_scope = scope("source");
    let target_scope = scope("target");
    let scopes = vec![source_scope.clone(), target_scope.clone()];
    let definitions = vec![
        definition_artifact(
            "a.json",
            vec![
                definition(source_scope.clone(), "/same", "en", "First", 0),
                definition(source_scope.clone(), "/same", "ja", "第一", 1),
            ],
        ),
        definition_artifact(
            "b.json",
            vec![definition(target_scope.clone(), "/same", "en", "Second", 0)],
        ),
    ];
    let references = vec![reference_artifact(
        "entry",
        DeliveryUnitId::main(),
        vec![reference(
            source_scope.clone(),
            MessageSelector::Exact(key("/same")),
        )],
    )];
    let outcome = do_link_multi_scopes(
        &scopes,
        vec![ScopeMapping::new(
            source_scope.clone(),
            target_scope.clone(),
        )],
        references,
        definitions,
        policy(&["en", "ja"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(&scopes),
        DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
        LinkLimits::default(),
    )
    .unwrap();
    assert!(outcome.generation_blocked());
    assert!(outcome.bundle_plans().is_none());
    assert_eq!(outcome.findings().len(), 1);
    let LinkFindingRecord::AmbiguousMessageDefinition(ambiguity) = outcome.findings()[0].record()
    else {
        panic!("expected mapped ambiguity");
    };
    assert_eq!(
        ambiguity.subject().resolved_scope().as_catalog_scope(),
        &target_scope
    );
    assert_eq!(ambiguity.subject().locale().as_str(), "en");
    assert_eq!(ambiguity.evidence().definitions().len(), 2);
    assert_eq!(
        ambiguity.evidence().definitions()[0]
            .source()
            .path()
            .segments()[0]
            .as_str(),
        "a.json"
    );
    assert_eq!(
        ambiguity.evidence().definitions()[1]
            .source()
            .path()
            .segments()[0]
            .as_str(),
        "b.json"
    );
}

#[test]
fn blocking_ambiguity_does_not_stop_unrelated_analysis_or_change_kind_precedence() {
    let app = scope("app");
    let outcome = do_link(
        app.clone(),
        vec![reference_artifact(
            "entry",
            DeliveryUnitId::main(),
            vec![
                reference(app.clone(), MessageSelector::Exact(key("/same"))),
                reference(app.clone(), MessageSelector::Exact(key("/missing"))),
            ],
        )],
        vec![
            definition_artifact(
                "a.json",
                vec![definition(app.clone(), "/same", "en", "First", 0)],
            ),
            definition_artifact(
                "b.json",
                vec![definition(app.clone(), "/same", "en", "Second", 0)],
            ),
            definition_artifact(
                "c.json",
                vec![definition(app.clone(), "/unused", "en", "Unused", 0)],
            ),
        ],
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
        LinkLimits::default(),
    )
    .unwrap();
    assert!(outcome.generation_blocked());
    assert_eq!(
        outcome
            .findings()
            .iter()
            .map(LinkFinding::kind)
            .collect::<Vec<_>>(),
        [
            LinkFindingKind::AmbiguousMessageDefinition,
            LinkFindingKind::UnresolvedMessage,
            LinkFindingKind::UnusedMessage,
        ]
    );
    let LinkFindingRecord::UnresolvedMessage(unresolved) = outcome.findings()[1].record() else {
        panic!("expected unrelated unresolved-message");
    };
    assert_eq!(
        unresolved.evidence().selector(),
        &MessageSelector::Exact(key("/missing"))
    );
    let LinkFindingRecord::UnusedMessage(unused) = outcome.findings()[2].record() else {
        panic!("expected unrelated unused-message");
    };
    assert_eq!(unused.subject().key().as_str(), "/unused");
}

#[test]
fn partial_completeness_suppresses_absence_findings_and_remains_distinct_from_wide_selector() {
    let app = scope("app");
    let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();

    let definition_partial = do_link(
        app.clone(),
        vec![reference_artifact(
            "missing",
            DeliveryUnitId::main(),
            vec![reference(
                app.clone(),
                MessageSelector::Exact(key("/missing")),
            )],
        )],
        Vec::new(),
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        completeness(
            std::slice::from_ref(&app),
            vec![(
                app.clone(),
                InputCompleteness::Partial(PartialReason::SourceFailed),
                InputCompleteness::Closed,
            )],
        ),
        graph.clone(),
        LinkLimits::default(),
    )
    .unwrap();
    assert_eq!(definition_partial.findings().len(), 1);
    assert!(matches!(
        definition_partial.findings()[0].record(),
        LinkFindingRecord::DegradedAnalysis(DegradedAnalysisFinding::PartialCompleteness(_))
    ));
    assert!(definition_partial.generation_blocked());

    let reference_partial = do_link(
        app.clone(),
        Vec::new(),
        vec![definition_artifact(
            "messages.json",
            vec![definition(app.clone(), "/unused", "en", "Unused", 0)],
        )],
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        completeness(
            std::slice::from_ref(&app),
            vec![(
                app.clone(),
                InputCompleteness::Closed,
                InputCompleteness::Partial(PartialReason::ProducerFailed),
            )],
        ),
        graph.clone(),
        LinkLimits::default(),
    )
    .unwrap();
    assert_eq!(reference_partial.findings().len(), 1);
    assert!(matches!(
        reference_partial.findings()[0].record(),
        LinkFindingRecord::DegradedAnalysis(DegradedAnalysisFinding::PartialCompleteness(_))
    ));

    let wide_and_partial = do_link(
        app.clone(),
        vec![reference_artifact(
            "wide",
            DeliveryUnitId::main(),
            vec![reference(app.clone(), MessageSelector::AllInScope)],
        )],
        vec![definition_artifact(
            "messages.json",
            vec![definition(app.clone(), "/kept", "en", "Kept", 0)],
        )],
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        completeness(
            std::slice::from_ref(&app),
            vec![(
                app.clone(),
                InputCompleteness::Partial(PartialReason::SourceFailed),
                InputCompleteness::Closed,
            )],
        ),
        graph,
        LinkLimits::default(),
    )
    .unwrap();
    assert_eq!(wide_and_partial.findings().len(), 2);
    assert!(matches!(
        wide_and_partial.findings()[0].record(),
        LinkFindingRecord::DegradedAnalysis(DegradedAnalysisFinding::WideSelector(_))
    ));
    assert!(!wide_and_partial.findings()[0].blocking());
    assert!(matches!(
        wide_and_partial.findings()[1].record(),
        LinkFindingRecord::DegradedAnalysis(DegradedAnalysisFinding::PartialCompleteness(_))
    ));
    assert!(wide_and_partial.findings()[1].blocking());
    assert!(wide_and_partial.generation_blocked());
}

#[test]
fn strict_and_compat_dynamic_policies_change_only_widening_and_disposition() {
    let app = scope("app");
    let definitions = vec![definition_artifact(
        "messages.json",
        vec![
            definition(app.clone(), "/a", "en", "A", 0),
            definition(app.clone(), "/b", "en", "B", 0),
        ],
    )];
    let references = vec![reference_artifact(
        "dynamic",
        DeliveryUnitId::main(),
        vec![reference(app.clone(), MessageSelector::UnboundedDynamic)],
    )];
    let do_link_with_mode = |mode| {
        do_link(
            app.clone(),
            references.clone(),
            definitions.clone(),
            policy(&["en"], Vec::new(), mode),
            closed_completeness(std::slice::from_ref(&app)),
            DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
            LinkLimits::default(),
        )
    };

    let compat = do_link_with_mode(DynamicReferenceMode::Compat).unwrap();
    assert_eq!(compat.findings().len(), 1);
    assert!(matches!(
        compat.findings()[0].record(),
        LinkFindingRecord::UnboundedDynamicReference(_)
    ));
    assert!(!compat.findings()[0].blocking());
    assert_eq!(compat.bundle_plans().unwrap()[0].messages().len(), 2);

    let strict = do_link_with_mode(DynamicReferenceMode::Strict).unwrap();
    assert_eq!(strict.findings().len(), 1);
    assert!(matches!(
        strict.findings()[0].record(),
        LinkFindingRecord::UnboundedDynamicReference(_)
    ));
    assert!(strict.findings()[0].blocking());
    assert!(strict.bundle_plans().is_none());
}

#[test]
fn prefix_pattern_and_all_in_scope_select_canonical_deduplicated_message_sets() {
    let app = scope("app");
    let all_unit = unit("all");
    let pattern_unit = unit("pattern");
    let prefix_unit = unit("prefix");
    let graph = DeliveryUnitGraph::try_new(
        vec![prefix_unit.clone(), all_unit.clone(), pattern_unit.clone()],
        Vec::new(),
        &LinkLimits::default(),
    )
    .unwrap();
    let definitions = vec![definition_artifact(
        "messages.json",
        vec![
            definition(app.clone(), "/settings", "en", "Settings", 0),
            definition(app.clone(), "/checkout/title", "en", "Title", 0),
            definition(app.clone(), "/checkout", "en", "Checkout", 0),
        ],
    )];
    let references = vec![
        reference_artifact(
            "prefix",
            prefix_unit,
            vec![
                reference(
                    app.clone(),
                    MessageSelector::Prefix(
                        CatalogKeyPrefix::try_new(DOMAIN, "/checkout").unwrap(),
                    ),
                ),
                reference(
                    app.clone(),
                    MessageSelector::Prefix(
                        CatalogKeyPrefix::try_new(DOMAIN, "/checkout").unwrap(),
                    ),
                ),
            ],
        ),
        reference_artifact(
            "all",
            all_unit,
            vec![reference(app.clone(), MessageSelector::AllInScope)],
        ),
        reference_artifact(
            "pattern",
            pattern_unit,
            vec![reference(
                app.clone(),
                MessageSelector::Pattern(CatalogKeyPattern::try_new(DOMAIN, "/settings").unwrap()),
            )],
        ),
    ];
    let outcome = do_link(
        app.clone(),
        references,
        definitions,
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        graph,
        LinkLimits::default(),
    )
    .unwrap();
    assert_eq!(outcome.findings().len(), 1);
    assert!(matches!(
        outcome.findings()[0].record(),
        LinkFindingRecord::DegradedAnalysis(DegradedAnalysisFinding::WideSelector(_))
    ));
    assert!(!outcome.findings()[0].blocking());

    let plans = outcome.bundle_plans().unwrap();
    assert_eq!(plans.len(), 3);
    assert_eq!(plans[0].delivery_unit(), &unit("all"));
    assert_eq!(plans[0].messages().len(), 3);
    assert_eq!(plans[1].delivery_unit(), &unit("pattern"));
    assert_eq!(
        plans[1].messages()[0].key().as_str(),
        "/settings",
        "anchored literal pattern selects one exact structural key"
    );
    assert_eq!(plans[2].delivery_unit(), &unit("prefix"));
    assert_eq!(
        plans[2]
            .messages()
            .iter()
            .map(|message| message.key().as_str())
            .collect::<Vec<_>>(),
        ["/checkout", "/checkout/title"],
        "equal repeated references deduplicate by logical message identity"
    );
}

#[test]
fn configured_roots_are_duplicated_only_into_real_graph_roots() {
    let app = scope("app");
    let child = unit("child");
    let root_a = unit("root-a");
    let root_b = unit("root-b");
    let graph = DeliveryUnitGraph::try_new(
        vec![child.clone(), root_b.clone(), root_a.clone()],
        vec![DeliveryUnitEdge::new(root_a.clone(), child.clone())],
        &LinkLimits::default(),
    )
    .unwrap();
    let outcome = do_link(
        app.clone(),
        Vec::new(),
        vec![definition_artifact(
            "messages.json",
            vec![definition(app.clone(), "/root", "en", "Root", 0)],
        )],
        policy(
            &["en"],
            vec![configured_root(
                app.clone(),
                MessageSelector::Exact(key("/root")),
            )],
            DynamicReferenceMode::Compat,
        ),
        closed_completeness(std::slice::from_ref(&app)),
        graph,
        LinkLimits::default(),
    )
    .unwrap();
    assert!(outcome.findings().is_empty());
    let plans = outcome.bundle_plans().unwrap();
    assert_eq!(plans.len(), 3);
    assert_eq!(plans[0].delivery_unit(), &child);
    assert!(plans[0].messages().is_empty());
    assert_eq!(plans[1].delivery_unit(), &root_a);
    assert_eq!(plans[1].messages().len(), 1);
    assert_eq!(plans[2].delivery_unit(), &root_b);
    assert_eq!(plans[2].messages().len(), 1);
}

#[test]
fn pattern_work_uses_distinct_canonical_candidates_and_request_wide_accounting() {
    let app = scope("app");
    let definitions = vec![
        definition_artifact(
            "en.json",
            vec![
                definition(app.clone(), "/a", "en", "A", 0),
                definition(app.clone(), "/b", "en", "B", 0),
            ],
        ),
        definition_artifact(
            "ja.json",
            vec![definition(app.clone(), "/a", "ja", "あ", 0)],
        ),
    ];
    let references = vec![reference_artifact(
        "pattern",
        DeliveryUnitId::main(),
        vec![reference(
            app.clone(),
            MessageSelector::Pattern(CatalogKeyPattern::try_new(DOMAIN, "/**").unwrap()),
        )],
    )];
    let limits = LinkLimits::default()
        .try_with_limit(LinkLimitCounter::PatternMatchStatesTotal, 4)
        .unwrap();
    let result = do_link(
        app.clone(),
        references,
        definitions,
        policy(&["en", "ja"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
        limits,
    );

    assert_limit(
        result.unwrap_err(),
        LinkLimitCounter::PatternMatchStatesTotal,
        LinkLimitObservation::Exact(8),
    );
}

#[test]
fn result_limits_follow_finding_then_plan_precedence_and_exact_byte_accounting() {
    let app = scope("app");
    let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();

    let findings_zero = do_link(
        app.clone(),
        vec![reference_artifact(
            "missing",
            DeliveryUnitId::main(),
            vec![reference(
                app.clone(),
                MessageSelector::Exact(key("/missing")),
            )],
        )],
        Vec::new(),
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        graph.clone(),
        LinkLimits::default()
            .try_with_limit(LinkLimitCounter::FindingsTotal, 0)
            .unwrap()
            .try_with_limit(LinkLimitCounter::FindingBytesTotal, 0)
            .unwrap()
            .try_with_limit(LinkLimitCounter::BundlePlansTotal, 0)
            .unwrap(),
    );
    assert_limit(
        findings_zero.unwrap_err(),
        LinkLimitCounter::FindingsTotal,
        LinkLimitObservation::Exact(1),
    );

    let finding_bytes = do_link(
        app.clone(),
        Vec::new(),
        vec![definition_artifact(
            "messages.json",
            vec![definition(app.clone(), "/unused", "en", "Unused", 0)],
        )],
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        graph.clone(),
        LinkLimits::default()
            .try_with_limit(LinkLimitCounter::FindingBytesTotal, 2)
            .unwrap(),
    );
    assert_limit(
        finding_bytes.unwrap_err(),
        LinkLimitCounter::FindingBytesTotal,
        LinkLimitObservation::Exact(3),
    );

    let plan_count = do_link(
        app.clone(),
        vec![reference_artifact(
            "used",
            DeliveryUnitId::main(),
            vec![reference(app.clone(), MessageSelector::Exact(key("/used")))],
        )],
        vec![definition_artifact(
            "messages.json",
            vec![definition(app.clone(), "/used", "en", "Used", 0)],
        )],
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        graph.clone(),
        LinkLimits::default()
            .try_with_limit(LinkLimitCounter::BundlePlansTotal, 0)
            .unwrap()
            .try_with_limit(LinkLimitCounter::ResolvedMessagesTotal, 0)
            .unwrap()
            .try_with_limit(LinkLimitCounter::BundlePlanBytesTotal, 0)
            .unwrap(),
    );
    assert_limit(
        plan_count.unwrap_err(),
        LinkLimitCounter::BundlePlansTotal,
        LinkLimitObservation::Exact(1),
    );

    let resolved_messages = do_link(
        app.clone(),
        vec![reference_artifact(
            "used",
            DeliveryUnitId::main(),
            vec![reference(app.clone(), MessageSelector::Exact(key("/used")))],
        )],
        vec![
            definition_artifact(
                "en.json",
                vec![definition(app.clone(), "/used", "en", "Used", 0)],
            ),
            definition_artifact(
                "ja.json",
                vec![definition(app.clone(), "/used", "ja", "使用", 0)],
            ),
        ],
        policy(&["en", "ja"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        graph.clone(),
        LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ResolvedMessagesTotal, 1)
            .unwrap()
            .try_with_limit(LinkLimitCounter::BundlePlanBytesTotal, 0)
            .unwrap(),
    );
    assert_limit(
        resolved_messages.unwrap_err(),
        LinkLimitCounter::ResolvedMessagesTotal,
        LinkLimitObservation::Exact(2),
    );

    let plan_bytes = do_link(
        app.clone(),
        Vec::new(),
        Vec::new(),
        policy(&["en", "ja"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        graph,
        LinkLimits::default()
            .try_with_limit(LinkLimitCounter::BundlePlanBytesTotal, 8)
            .unwrap(),
    );
    assert_limit(
        plan_bytes.unwrap_err(),
        LinkLimitCounter::BundlePlanBytesTotal,
        LinkLimitObservation::Exact(10),
    );

    let blocking_skips_plan_limits = do_link(
        app.clone(),
        vec![reference_artifact(
            "missing-with-plan-limits",
            DeliveryUnitId::main(),
            vec![reference(
                app.clone(),
                MessageSelector::Exact(key("/missing")),
            )],
        )],
        Vec::new(),
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
        LinkLimits::default()
            .try_with_limit(LinkLimitCounter::BundlePlansTotal, 0)
            .unwrap()
            .try_with_limit(LinkLimitCounter::ResolvedMessagesTotal, 0)
            .unwrap()
            .try_with_limit(LinkLimitCounter::BundlePlanBytesTotal, 0)
            .unwrap(),
    )
    .unwrap();
    assert!(blocking_skips_plan_limits.generation_blocked());
    assert_eq!(blocking_skips_plan_limits.findings().len(), 1);
}

#[test]
fn valid_empty_graph_preserves_some_empty_plan_state() {
    let app = scope("app");
    let outcome = do_link(
        app.clone(),
        Vec::new(),
        Vec::new(),
        policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
        closed_completeness(std::slice::from_ref(&app)),
        DeliveryUnitGraph::try_new(Vec::new(), Vec::new(), &LinkLimits::default()).unwrap(),
        LinkLimits::default()
            .try_with_limit(LinkLimitCounter::BundlePlansTotal, 0)
            .unwrap()
            .try_with_limit(LinkLimitCounter::BundlePlanBytesTotal, 0)
            .unwrap(),
    )
    .unwrap();
    assert!(!outcome.generation_blocked());
    assert_eq!(outcome.bundle_plans(), Some([].as_slice()));
}

#[test]
fn independent_calls_are_reentrant_across_threads() {
    let app = scope("app");
    let references = vec![reference_artifact(
        "entry",
        DeliveryUnitId::main(),
        vec![reference(
            app.clone(),
            MessageSelector::Exact(key("/message")),
        )],
    )];
    let definitions = vec![definition_artifact(
        "messages.json",
        vec![definition(app.clone(), "/message", "en", "Message", 0)],
    )];
    let policy = policy(&["en"], Vec::new(), DynamicReferenceMode::Compat);
    let mappings = mappings(std::slice::from_ref(&app), Vec::new());
    let completeness = closed_completeness(std::slice::from_ref(&app));
    let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();
    let limits = LinkLimits::default();
    let request = LinkRequest::try_new(
        &references,
        &definitions,
        &policy,
        &mappings,
        &completeness,
        &graph,
        &limits,
    )
    .unwrap();
    let expected = link(&request).unwrap();

    std::thread::scope(|scope| {
        let handles = (0..4)
            .map(|_| scope.spawn(|| link(&request).unwrap()))
            .collect::<Vec<_>>();
        for handle in handles {
            assert_eq!(handle.join().unwrap(), expected);
        }
    });
}

#[test]
fn outcome_owns_payload_and_location_after_every_request_input_is_dropped() {
    let outcome = {
        let app = scope("app");
        do_link(
            app.clone(),
            vec![reference_artifact(
                "entry",
                DeliveryUnitId::main(),
                vec![reference(
                    app.clone(),
                    MessageSelector::Exact(key("/message")),
                )],
            )],
            vec![definition_artifact(
                "messages.json",
                vec![definition(app.clone(), "/message", "en", "Owned", 0)],
            )],
            policy(&["en"], Vec::new(), DynamicReferenceMode::Compat),
            closed_completeness(std::slice::from_ref(&app)),
            DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap(),
            LinkLimits::default(),
        )
        .unwrap()
    };

    let message = &outcome.bundle_plans().unwrap()[0].messages()[0];
    assert_eq!(message.message().as_str(), "Owned");
    assert_eq!(
        message.definition().source().path().segments()[0].as_str(),
        "messages.json"
    );
    assert_eq!(
        message.definition().entry().structural_path().as_str(),
        "/message"
    );
}
