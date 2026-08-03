// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use intlify_export::{
    ExportArtifact, ExportArtifactFormatVersion, ExportArtifactKind, ExportArtifactMetadata,
    ExportArtifactPath, ExportArtifactPayload, ExportArtifactRelationship,
    ExportArtifactRelationshipKind, ExportArtifactSet, ExportError, ExportErrorEvidence,
    ExportMediaType, InvalidOutputViolation, OutputLimitCounter, OutputLimitObservation,
};

fn path<const N: usize>(segments: [&str; N]) -> ExportArtifactPath {
    ExportArtifactPath::try_new(segments).unwrap()
}

fn relationship(kind: ExportArtifactRelationshipKind, target: &str) -> ExportArtifactRelationship {
    ExportArtifactRelationship::new(kind, path([target]))
}

fn artifact(name: &str, relationships: Vec<ExportArtifactRelationship>) -> ExportArtifact {
    ExportArtifact::new(
        path([name]),
        ExportArtifactKind::try_new("com.example/custom-bundle").unwrap(),
        ExportArtifactFormatVersion::DRAFT_V0_1,
        ExportArtifactPayload::try_new(name.as_bytes().to_vec()).unwrap(),
        ExportArtifactMetadata::try_new(
            Some(ExportMediaType::try_new("application/octet-stream").unwrap()),
            relationships,
        )
        .unwrap(),
    )
}

fn lightweight_artifact(
    name: impl Into<Box<str>>,
    kind: &ExportArtifactKind,
    relationships: Vec<ExportArtifactRelationship>,
) -> ExportArtifact {
    ExportArtifact::new(
        ExportArtifactPath::try_new([name]).unwrap(),
        kind.clone(),
        ExportArtifactFormatVersion::DRAFT_V0_1,
        ExportArtifactPayload::try_new(Box::<[u8]>::default()).unwrap(),
        ExportArtifactMetadata::try_new(None, relationships).unwrap(),
    )
}

fn invalid(error: &ExportError) -> InvalidOutputViolation {
    let ExportErrorEvidence::InvalidOutput(evidence) = error.evidence() else {
        panic!("expected invalid-output evidence, got {error:?}");
    };
    evidence.violation()
}

fn limit(error: &ExportError) -> (OutputLimitCounter, OutputLimitObservation) {
    let ExportErrorEvidence::OutputLimitExceeded(evidence) = error.evidence() else {
        panic!("expected output-limit evidence, got {error:?}");
    };
    (evidence.counter(), evidence.observation())
}

#[test]
fn set_construction_canonicalizes_artifacts_and_relationships() {
    let set = ExportArtifactSet::try_new(vec![
        artifact(
            "loader.mjs",
            vec![
                relationship(ExportArtifactRelationshipKind::LazyLoad, "b.mjs"),
                relationship(ExportArtifactRelationshipKind::EagerLoad, "a.mjs"),
            ],
        ),
        artifact("b.mjs", Vec::new()),
        artifact("a.mjs", Vec::new()),
    ])
    .unwrap();

    let paths: Vec<_> = set
        .artifacts()
        .iter()
        .map(|artifact| artifact.logical_path().segments()[0].as_str())
        .collect();
    assert_eq!(paths, ["a.mjs", "b.mjs", "loader.mjs"]);
    let relationships = set.artifacts()[2].metadata().relationships();
    assert_eq!(relationships[0].kind().tag(), 0);
    assert_eq!(relationships[0].target().segments()[0].as_str(), "a.mjs");
    assert_eq!(relationships[1].kind().tag(), 1);
    assert_eq!(relationships[1].target().segments()[0].as_str(), "b.mjs");

    assert_eq!(
        set.artifacts()[0].kind().as_str(),
        "com.example/custom-bundle"
    );

    let equivalent = ExportArtifactSet::try_new(vec![
        artifact("a.mjs", Vec::new()),
        artifact(
            "loader.mjs",
            vec![
                relationship(ExportArtifactRelationshipKind::EagerLoad, "a.mjs"),
                relationship(ExportArtifactRelationshipKind::LazyLoad, "b.mjs"),
            ],
        ),
        artifact("b.mjs", Vec::new()),
    ])
    .unwrap();
    assert_eq!(set, equivalent);
}

#[test]
fn empty_sets_and_arbitrary_binary_payloads_are_structurally_valid() {
    assert!(ExportArtifactSet::try_new(Vec::new())
        .unwrap()
        .artifacts()
        .is_empty());
    let binary = ExportArtifact::new(
        path(["binary.data"]),
        ExportArtifactKind::try_new("com.example/binary-data").unwrap(),
        ExportArtifactFormatVersion::new(0, 7),
        ExportArtifactPayload::try_new(vec![0x00, 0xff, 0x80]).unwrap(),
        ExportArtifactMetadata::empty(),
    );
    let set = ExportArtifactSet::try_new(vec![binary]).unwrap();
    assert_eq!(set.artifacts()[0].payload().as_bytes(), &[0x00, 0xff, 0x80]);
}

#[test]
fn duplicate_artifact_paths_reject_the_complete_set() {
    let error = ExportArtifactSet::try_new(vec![
        artifact("same", Vec::new()),
        artifact("same", Vec::new()),
    ])
    .unwrap_err();
    assert_eq!(
        invalid(&error),
        InvalidOutputViolation::DuplicateArtifactPath
    );
}

#[test]
fn path_limits_accept_exact_boundaries_and_select_fixed_counter_precedence() {
    let sixty_four = vec!["a"; 64];
    assert_eq!(
        ExportArtifactPath::try_new(sixty_four)
            .unwrap()
            .segments()
            .len(),
        64
    );
    let error = ExportArtifactPath::try_new(vec!["a"; 65]).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::ArtifactPathSegmentsPerPath,
            OutputLimitObservation::Exact(65),
        )
    );

    let exact_segment = "a".repeat(255);
    assert_eq!(
        ExportArtifactPath::try_new([exact_segment.as_str()])
            .unwrap()
            .segments()[0]
            .as_str()
            .len(),
        255
    );
    let over_segment = "a".repeat(256);
    let error = ExportArtifactPath::try_new([over_segment.as_str()]).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::ArtifactPathSegmentBytes,
            OutputLimitObservation::Exact(256),
        )
    );

    let mut exact_path = vec!["a".repeat(255); 16];
    exact_path.push("a".repeat(16));
    assert!(ExportArtifactPath::try_new(exact_path.iter().map(String::as_str)).is_ok());
    exact_path[16].push('a');
    let error = ExportArtifactPath::try_new(exact_path.iter().map(String::as_str)).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::ArtifactPathBytesPerPath,
            OutputLimitObservation::Exact(4_097),
        )
    );

    let consumed = std::cell::Cell::new(0_u8);
    let streaming = std::iter::from_fn(|| {
        consumed.set(consumed.get() + 1);
        Some("a")
    });
    let error = ExportArtifactPath::try_new(streaming).unwrap_err();
    assert_eq!(consumed.get(), 65);
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::ArtifactPathSegmentsPerPath,
            OutputLimitObservation::Exact(65),
        )
    );
}

#[test]
fn kind_and_media_types_are_open_but_lexically_canonical() {
    let exact_kind = format!("com.example/{}", "a".repeat(243));
    assert_eq!(exact_kind.len(), 255);
    assert!(ExportArtifactKind::try_new(exact_kind).is_ok());
    let over_kind = format!("com.example/{}", "a".repeat(244));
    let error = ExportArtifactKind::try_new(over_kind).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::ArtifactKindBytes,
            OutputLimitObservation::Exact(256),
        )
    );
    assert_eq!(
        invalid(&ExportArtifactKind::try_new("Com.Example/custom").unwrap_err()),
        InvalidOutputViolation::ArtifactKind
    );

    let exact_media = format!("{}/{}", "a".repeat(127), "b".repeat(127));
    assert_eq!(exact_media.len(), 255);
    assert!(ExportMediaType::try_new(exact_media).is_ok());
    let component_over = format!("a/{}", "b".repeat(128));
    let error = ExportMediaType::try_new(component_over).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::MediaTypeComponentBytes,
            OutputLimitObservation::Exact(128),
        )
    );
    assert_eq!(
        invalid(&ExportMediaType::try_new("Text/JavaScript; charset=utf-8").unwrap_err()),
        InvalidOutputViolation::MediaType
    );
}

#[test]
fn relationship_failures_follow_target_duplicate_conflict_self_then_closure_order() {
    assert_eq!(
        invalid(
            &ExportArtifactSet::try_new(vec![artifact(
                "a",
                vec![relationship(
                    ExportArtifactRelationshipKind::EagerLoad,
                    "missing",
                )],
            )])
            .unwrap_err(),
        ),
        InvalidOutputViolation::RelationshipTarget
    );

    assert_eq!(
        invalid(
            &ExportArtifactSet::try_new(vec![
                artifact(
                    "a",
                    vec![
                        relationship(ExportArtifactRelationshipKind::EagerLoad, "b"),
                        relationship(ExportArtifactRelationshipKind::EagerLoad, "b"),
                    ],
                ),
                artifact("b", Vec::new()),
            ])
            .unwrap_err(),
        ),
        InvalidOutputViolation::DuplicateRelationship
    );

    assert_eq!(
        invalid(
            &ExportArtifactSet::try_new(vec![
                artifact(
                    "a",
                    vec![
                        relationship(ExportArtifactRelationshipKind::LazyLoad, "b"),
                        relationship(ExportArtifactRelationshipKind::EagerLoad, "b"),
                    ],
                ),
                artifact("b", Vec::new()),
            ])
            .unwrap_err(),
        ),
        InvalidOutputViolation::ConflictingRelationshipClassification
    );

    assert_eq!(
        invalid(
            &ExportArtifactSet::try_new(vec![artifact(
                "a",
                vec![relationship(ExportArtifactRelationshipKind::LazyLoad, "a",)],
            )])
            .unwrap_err(),
        ),
        InvalidOutputViolation::RelationshipSelfEdge
    );

    assert_eq!(
        invalid(
            &ExportArtifactSet::try_new(vec![
                artifact(
                    "a",
                    vec![
                        relationship(ExportArtifactRelationshipKind::EagerLoad, "b"),
                        relationship(ExportArtifactRelationshipKind::LazyLoad, "c"),
                    ],
                ),
                artifact(
                    "b",
                    vec![relationship(ExportArtifactRelationshipKind::EagerLoad, "c",)],
                ),
                artifact("c", Vec::new()),
            ])
            .unwrap_err(),
        ),
        InvalidOutputViolation::LazyTargetInEagerClosure
    );
}

#[test]
fn eager_and_lazy_cycles_are_valid_when_each_lazy_edge_stays_outside_eager_closure() {
    assert!(ExportArtifactSet::try_new(vec![
        artifact(
            "a",
            vec![relationship(ExportArtifactRelationshipKind::EagerLoad, "b",)],
        ),
        artifact(
            "b",
            vec![relationship(ExportArtifactRelationshipKind::EagerLoad, "a",)],
        ),
    ])
    .is_ok());

    assert!(ExportArtifactSet::try_new(vec![
        artifact(
            "a",
            vec![relationship(ExportArtifactRelationshipKind::LazyLoad, "b",)],
        ),
        artifact(
            "b",
            vec![relationship(ExportArtifactRelationshipKind::LazyLoad, "a",)],
        ),
    ])
    .is_ok());

    assert!(ExportArtifactSet::try_new(vec![
        artifact(
            "a",
            vec![relationship(ExportArtifactRelationshipKind::EagerLoad, "b",)],
        ),
        artifact(
            "b",
            vec![relationship(ExportArtifactRelationshipKind::LazyLoad, "a",)],
        ),
    ])
    .is_ok());
}

#[test]
fn eager_scc_membership_makes_a_lazy_edge_contradictory() {
    let error = ExportArtifactSet::try_new(vec![
        artifact(
            "a",
            vec![
                relationship(ExportArtifactRelationshipKind::EagerLoad, "b"),
                relationship(ExportArtifactRelationshipKind::LazyLoad, "c"),
            ],
        ),
        artifact(
            "b",
            vec![relationship(ExportArtifactRelationshipKind::EagerLoad, "c")],
        ),
        artifact(
            "c",
            vec![relationship(ExportArtifactRelationshipKind::EagerLoad, "a")],
        ),
    ])
    .unwrap_err();
    assert_eq!(
        invalid(&error),
        InvalidOutputViolation::LazyTargetInEagerClosure
    );
}

#[test]
fn relationship_count_charges_submitted_occurrences_before_semantics() {
    let relationships: Vec<_> = (0..4_096)
        .map(|index| {
            ExportArtifactRelationship::new(
                ExportArtifactRelationshipKind::EagerLoad,
                ExportArtifactPath::try_new([format!("target-{index}")]).unwrap(),
            )
        })
        .collect();
    assert!(ExportArtifactMetadata::try_new(None, relationships.clone()).is_ok());

    let mut over = relationships;
    over.push(over[0].clone());
    let error = ExportArtifactMetadata::try_new(None, over).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::RelationshipRecordsPerArtifact,
            OutputLimitObservation::Exact(4_097),
        )
    );
}

#[test]
fn artifact_record_count_accepts_the_exact_ceiling_and_rejects_first_over() {
    let ceiling = OutputLimitCounter::ArtifactRecordsPerSet.ceiling() as usize;
    let kind = ExportArtifactKind::try_new("com.example/count-fixture").unwrap();
    let exact: Vec<_> = (0..ceiling)
        .map(|index| lightweight_artifact(format!("artifact-{index:05}"), &kind, Vec::new()))
        .collect();
    assert_eq!(
        ExportArtifactSet::try_new(exact).unwrap().artifacts().len(),
        ceiling
    );

    let over = vec![lightweight_artifact("over", &kind, Vec::new()); ceiling + 1];
    let error = ExportArtifactSet::try_new(over).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::ArtifactRecordsPerSet,
            OutputLimitObservation::Exact(ceiling as u64 + 1),
        )
    );
}

#[test]
fn set_relationship_count_accepts_the_exact_ceiling_and_rejects_first_over() {
    let kind = ExportArtifactKind::try_new("com.example/relationship-fixture").unwrap();
    let targets: Vec<_> = (0..4_096)
        .map(|index| ExportArtifactPath::try_new([format!("target-{index:04}")]).unwrap())
        .collect();
    let fanout = || {
        targets
            .iter()
            .cloned()
            .map(|target| {
                ExportArtifactRelationship::new(ExportArtifactRelationshipKind::EagerLoad, target)
            })
            .collect()
    };

    let mut exact: Vec<_> = (0..4_096)
        .map(|index| lightweight_artifact(format!("target-{index:04}"), &kind, Vec::new()))
        .collect();
    exact.extend(
        (0..16).map(|index| lightweight_artifact(format!("source-{index:02}"), &kind, fanout())),
    );
    assert_eq!(
        ExportArtifactSet::try_new(exact)
            .unwrap()
            .artifacts()
            .iter()
            .map(|artifact| artifact.metadata().relationships().len())
            .sum::<usize>(),
        65_536
    );

    let repeated_relationship = ExportArtifactRelationship::new(
        ExportArtifactRelationshipKind::EagerLoad,
        targets[0].clone(),
    );
    let full_source =
        lightweight_artifact("source", &kind, vec![repeated_relationship.clone(); 4_096]);
    let mut over = vec![full_source; 16];
    over.push(lightweight_artifact(
        "source-extra",
        &kind,
        vec![repeated_relationship],
    ));
    let error = ExportArtifactSet::try_new(over).unwrap_err();
    assert_eq!(
        limit(&error),
        (
            OutputLimitCounter::RelationshipRecordsPerSet,
            OutputLimitObservation::Exact(65_537),
        )
    );
}
