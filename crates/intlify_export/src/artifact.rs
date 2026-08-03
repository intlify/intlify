// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked, portable in-memory output returned by platform exporters.
//!
//! This module owns logical paths, semantic kinds, per-kind format versions,
//! payload bytes, common metadata, relationship graph validation, canonical
//! ordering, and fixed output accounting. It performs no host-path mapping,
//! capability preflight, filesystem registration, or payload interpretation.

use std::collections::{BTreeMap, BTreeSet};

use intlify_contract::ProducerId;

use crate::export_error::{invalid_output, OutputLimitAccumulator};
use crate::{
    ExportError, InternalInvariantEvidence, InternalInvariantViolation, InvalidOutputViolation,
    OutputLimitCounter,
};

/// Complete checked result of one platform-exporter invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifactSet {
    artifacts: Vec<ExportArtifact>,
}

impl ExportArtifactSet {
    /// Validate, canonicalize, and retain one complete candidate artifact set.
    pub fn try_new(mut artifacts: Vec<ExportArtifact>) -> Result<Self, ExportError> {
        validate_output_limits(&artifacts)?;
        validate_paths(&artifacts)?;

        artifacts.sort_unstable_by(|left, right| left.logical_path.cmp(&right.logical_path));
        if artifacts
            .windows(2)
            .any(|pair| pair[0].logical_path == pair[1].logical_path)
        {
            return Err(invalid_output(
                InvalidOutputViolation::DuplicateArtifactPath,
            ));
        }
        validate_kind_and_media(&artifacts)?;

        for artifact in &mut artifacts {
            artifact
                .metadata
                .relationships
                .sort_unstable_by(|left, right| {
                    (left.kind, &left.target).cmp(&(right.kind, &right.target))
                });
        }
        validate_relationships(&artifacts)?;

        Ok(Self { artifacts })
    }

    /// Return artifacts in canonical exact logical-path order.
    #[must_use]
    pub fn artifacts(&self) -> &[ExportArtifact] {
        &self.artifacts
    }
}

/// One generic checked output envelope.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifact {
    logical_path: ExportArtifactPath,
    kind: ExportArtifactKind,
    format_version: ExportArtifactFormatVersion,
    payload: ExportArtifactPayload,
    metadata: ExportArtifactMetadata,
}

impl ExportArtifact {
    /// Assemble one candidate record from checked scalar values.
    #[must_use]
    pub const fn new(
        logical_path: ExportArtifactPath,
        kind: ExportArtifactKind,
        format_version: ExportArtifactFormatVersion,
        payload: ExportArtifactPayload,
        metadata: ExportArtifactMetadata,
    ) -> Self {
        Self {
            logical_path,
            kind,
            format_version,
            payload,
            metadata,
        }
    }

    /// Return the exact portable logical path.
    #[must_use]
    pub const fn logical_path(&self) -> &ExportArtifactPath {
        &self.logical_path
    }

    /// Return the open semantic representation kind.
    #[must_use]
    pub const fn kind(&self) -> &ExportArtifactKind {
        &self.kind
    }

    /// Return the per-kind representation contract version.
    #[must_use]
    pub const fn format_version(&self) -> ExportArtifactFormatVersion {
        self.format_version
    }

    /// Return the exact owned payload bytes.
    #[must_use]
    pub const fn payload(&self) -> &ExportArtifactPayload {
        &self.payload
    }

    /// Return the common portable metadata.
    #[must_use]
    pub const fn metadata(&self) -> &ExportArtifactMetadata {
        &self.metadata
    }
}

/// Portable non-empty relative logical path represented as exact segments.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExportArtifactPath {
    segments: Vec<ExportArtifactPathSegment>,
}

impl ExportArtifactPath {
    /// Validate and retain exact UTF-8 logical-path segments.
    pub fn try_new<I, S>(segments: I) -> Result<Self, ExportError>
    where
        I: IntoIterator<Item = S>,
        S: Into<Box<str>>,
    {
        let mut input = segments.into_iter();
        let mut limits = OutputLimitAccumulator::new();
        let (lower, upper) = input.size_hint();
        let mut segments = Vec::with_capacity(upper.unwrap_or(lower).min(64));
        let mut path_bytes = 0;
        for segment in &mut input {
            if segments.len() == 64 {
                limits.observe_exact(OutputLimitCounter::ArtifactPathSegmentsPerPath, 65);
                return Err(limits
                    .finish()
                    .expect("the sixty-fifth segment must exceed the fixed path ceiling"));
            }
            let segment = segment.into();
            limits.observe_usize(OutputLimitCounter::ArtifactPathSegmentBytes, segment.len());
            limits.add_usize(
                OutputLimitCounter::ArtifactPathBytesPerPath,
                &mut path_bytes,
                segment.len(),
            );
            segments.push(segment);
        }
        if let Some(error) = limits.finish() {
            return Err(error);
        }
        if !valid_path_segments(segments.iter().map(Box::as_ref)) {
            return Err(invalid_output(InvalidOutputViolation::ArtifactPath));
        }
        Ok(Self {
            segments: segments
                .into_iter()
                .map(ExportArtifactPathSegment)
                .collect(),
        })
    }

    /// Return the exact ordered segment array.
    #[must_use]
    pub fn segments(&self) -> &[ExportArtifactPathSegment] {
        &self.segments
    }
}

/// One exact validated logical-path segment.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExportArtifactPathSegment(Box<str>);

impl ExportArtifactPathSegment {
    /// Return the exact UTF-8 segment spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Open namespaced semantic identifier for one output representation and role.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExportArtifactKind(Box<str>);

impl ExportArtifactKind {
    /// Validate and retain an exact `<reverse-dns>/<kebab-slug>` identifier.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ExportError> {
        let value = value.into();
        if !valid_kind_shape(&value) {
            return Err(invalid_output(InvalidOutputViolation::ArtifactKind));
        }
        let mut limits = OutputLimitAccumulator::new();
        limits.observe_usize(OutputLimitCounter::ArtifactKindBytes, value.len());
        if let Some(error) = limits.finish() {
            return Err(error);
        }

        // Reuse the contract-owned validator for every admitted spelling. The
        // local shape pass above is also needed to distinguish an otherwise
        // valid overlength kind from a non-limit grammar violation.
        if ProducerId::try_new(value.clone()).is_err() {
            return Err(invalid_output(InvalidOutputViolation::ArtifactKind));
        }
        Ok(Self(value))
    }

    /// Return the exact validated identifier.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Mandatory per-kind output representation version.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExportArtifactFormatVersion {
    major: u16,
    minor: u16,
}

impl ExportArtifactFormatVersion {
    /// Current mutable draft version used by initial built-in representations.
    pub const DRAFT_V0_1: Self = Self::new(0, 1);

    /// Construct one typed version from its unsigned components.
    #[must_use]
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }

    /// Return the major component.
    #[must_use]
    pub const fn major(self) -> u16 {
        self.major
    }

    /// Return the minor component.
    #[must_use]
    pub const fn minor(self) -> u16 {
        self.minor
    }
}

/// Owned immutable bytes for one output artifact.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifactPayload(Box<[u8]>);

impl ExportArtifactPayload {
    /// Validate the exact stored length and take ownership of the final bytes.
    pub fn try_new(bytes: impl Into<Box<[u8]>>) -> Result<Self, ExportError> {
        let bytes = bytes.into();
        let mut limits = OutputLimitAccumulator::new();
        limits.observe_usize(OutputLimitCounter::PayloadBytesPerArtifact, bytes.len());
        if let Some(error) = limits.finish() {
            return Err(error);
        }
        Ok(Self(bytes))
    }

    pub(crate) const fn from_checked(bytes: Box<[u8]>) -> Self {
        Self(bytes)
    }

    /// Return the exact stored byte sequence.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

/// Closed common metadata shared by build integrations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifactMetadata {
    media_type: Option<ExportMediaType>,
    relationships: Vec<ExportArtifactRelationship>,
}

impl ExportArtifactMetadata {
    /// Validate per-artifact relationship count and retain common metadata.
    pub fn try_new(
        media_type: Option<ExportMediaType>,
        relationships: Vec<ExportArtifactRelationship>,
    ) -> Result<Self, ExportError> {
        let mut limits = OutputLimitAccumulator::new();
        limits.observe_usize(
            OutputLimitCounter::RelationshipRecordsPerArtifact,
            relationships.len(),
        );
        if let Some(error) = limits.finish() {
            return Err(error);
        }
        Ok(Self {
            media_type,
            relationships,
        })
    }

    /// Construct the canonical empty semantic metadata value.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            media_type: None,
            relationships: Vec::new(),
        }
    }

    /// Return the explicit parameter-free MIME claim, when present.
    #[must_use]
    pub const fn media_type(&self) -> Option<&ExportMediaType> {
        self.media_type.as_ref()
    }

    /// Return relationships in checked canonical order after set construction.
    #[must_use]
    pub fn relationships(&self) -> &[ExportArtifactRelationship] {
        &self.relationships
    }
}

/// Canonical lowercase parameter-free MIME essence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ExportMediaType(Box<str>);

impl ExportMediaType {
    /// Validate and retain one exact `type/subtype` spelling.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ExportError> {
        let value = value.into();
        let Some((type_name, subtype_name)) = media_components(&value) else {
            return Err(invalid_output(InvalidOutputViolation::MediaType));
        };
        let mut limits = OutputLimitAccumulator::new();
        limits.observe_usize(OutputLimitCounter::MediaTypeComponentBytes, type_name.len());
        limits.observe_usize(
            OutputLimitCounter::MediaTypeComponentBytes,
            subtype_name.len(),
        );
        limits.observe_usize(OutputLimitCounter::MediaTypeBytes, value.len());
        if let Some(error) = limits.finish() {
            return Err(error);
        }
        Ok(Self(value))
    }

    /// Return the exact validated MIME essence.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One directed portable relationship to another artifact in the same set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportArtifactRelationship {
    kind: ExportArtifactRelationshipKind,
    target: ExportArtifactPath,
}

impl ExportArtifactRelationship {
    /// Construct one typed relationship to an exact checked logical path.
    #[must_use]
    pub const fn new(kind: ExportArtifactRelationshipKind, target: ExportArtifactPath) -> Self {
        Self { kind, target }
    }

    /// Return the portable load classification.
    #[must_use]
    pub const fn kind(&self) -> ExportArtifactRelationshipKind {
        self.kind
    }

    /// Return the exact target logical path.
    #[must_use]
    pub const fn target(&self) -> &ExportArtifactPath {
        &self.target
    }
}

/// Closed portable load classifications in canonical tag order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ExportArtifactRelationshipKind {
    /// The target belongs to the source artifact's initial load closure.
    EagerLoad,
    /// The target remains outside that closure behind a deferred load path.
    LazyLoad,
}

impl ExportArtifactRelationshipKind {
    /// Return the exact common contract tag.
    #[must_use]
    pub const fn tag(self) -> u8 {
        match self {
            Self::EagerLoad => 0,
            Self::LazyLoad => 1,
        }
    }
}

fn valid_path_segments<'a>(mut segments: impl Iterator<Item = &'a str>) -> bool {
    let Some(first) = segments.next() else {
        return false;
    };
    valid_path_segment(first) && segments.all(valid_path_segment)
}

fn valid_path_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && !segment.chars().any(|character| {
            matches!(character, '/' | '\\' | '\0')
                || character.is_control()
                || is_bidirectional_control(character)
        })
}

fn is_bidirectional_control(character: char) -> bool {
    matches!(
        character,
        '\u{061C}'
            | '\u{200E}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn valid_kind_shape(value: &str) -> bool {
    if !value.is_ascii() {
        return false;
    }
    let Some((domain, slug)) = value.split_once('/') else {
        return false;
    };
    if slug.contains('/') || !valid_slug(slug) {
        return false;
    }
    let mut labels = domain.split('.');
    let Some(first) = labels.next() else {
        return false;
    };
    let Some(second) = labels.next() else {
        return false;
    };
    valid_domain_label(first) && valid_domain_label(second) && labels.all(valid_domain_label)
}

fn valid_domain_label(label: &str) -> bool {
    let bytes = label.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= 63
        && ascii_lowercase_or_digit(bytes[0])
        && ascii_lowercase_or_digit(bytes[bytes.len() - 1])
        && bytes
            .iter()
            .all(|byte| ascii_lowercase_or_digit(*byte) || *byte == b'-')
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug
            .split('-')
            .all(|part| !part.is_empty() && part.bytes().all(ascii_lowercase_or_digit))
}

fn ascii_lowercase_or_digit(byte: u8) -> bool {
    byte.is_ascii_lowercase() || byte.is_ascii_digit()
}

fn media_components(value: &str) -> Option<(&str, &str)> {
    if !value.is_ascii() {
        return None;
    }
    let (type_name, subtype_name) = value.split_once('/')?;
    if subtype_name.contains('/')
        || !valid_media_component(type_name)
        || !valid_media_component(subtype_name)
    {
        return None;
    }
    Some((type_name, subtype_name))
}

fn valid_media_component(component: &str) -> bool {
    let mut bytes = component.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    ascii_lowercase_or_digit(first)
        && bytes.all(|byte| {
            ascii_lowercase_or_digit(byte)
                || matches!(
                    byte,
                    b'!' | b'#' | b'$' | b'&' | b'-' | b'^' | b'_' | b'.' | b'+'
                )
        })
}

fn validate_output_limits(artifacts: &[ExportArtifact]) -> Result<(), ExportError> {
    let mut limits = OutputLimitAccumulator::new();
    limits.observe_usize(OutputLimitCounter::ArtifactRecordsPerSet, artifacts.len());
    if let Some(error) = limits.finish() {
        return Err(error);
    }

    // Logical paths, kinds, and media types precede relationship counters and
    // can be measured without visiting any relationship record.
    let mut limits = OutputLimitAccumulator::new();
    let mut artifact_path_bytes = 0;
    for artifact in artifacts {
        measure_path(
            &artifact.logical_path,
            &mut limits,
            Some(&mut artifact_path_bytes),
        );
        if valid_kind_shape(&artifact.kind.0) {
            limits.observe_usize(OutputLimitCounter::ArtifactKindBytes, artifact.kind.0.len());
        }
        if let Some(media_type) = &artifact.metadata.media_type {
            if let Some((type_name, subtype_name)) = media_components(&media_type.0) {
                limits.observe_usize(OutputLimitCounter::MediaTypeComponentBytes, type_name.len());
                limits.observe_usize(
                    OutputLimitCounter::MediaTypeComponentBytes,
                    subtype_name.len(),
                );
                limits.observe_usize(OutputLimitCounter::MediaTypeBytes, media_type.0.len());
            }
        }
    }
    if let Some(error) = limits.finish() {
        return Err(error);
    }

    // Every relationship target is already an independently checked path.
    // Preflight the submitted vector lengths before walking their records so
    // an oversized set cannot force unbounded target traversal or graph work.
    let mut limits = OutputLimitAccumulator::new();
    let mut relationship_records = 0;
    for artifact in artifacts {
        limits.observe_usize(
            OutputLimitCounter::RelationshipRecordsPerArtifact,
            artifact.metadata.relationships.len(),
        );
        limits.add_usize(
            OutputLimitCounter::RelationshipRecordsPerSet,
            &mut relationship_records,
            artifact.metadata.relationships.len(),
        );
    }
    if let Some(error) = limits.finish() {
        return Err(error);
    }

    let mut limits = OutputLimitAccumulator::new();
    let mut relationship_target_bytes = 0;
    let mut payload_bytes = 0;
    for artifact in artifacts {
        for relationship in &artifact.metadata.relationships {
            measure_path(&relationship.target, &mut limits, None);
            for segment in &relationship.target.segments {
                limits.add_usize(
                    OutputLimitCounter::RelationshipTargetBytesPerSet,
                    &mut relationship_target_bytes,
                    segment.0.len(),
                );
            }
        }
        limits.observe_usize(
            OutputLimitCounter::PayloadBytesPerArtifact,
            artifact.payload.0.len(),
        );
        limits.add_usize(
            OutputLimitCounter::PayloadBytesPerSet,
            &mut payload_bytes,
            artifact.payload.0.len(),
        );
    }

    limits.finish().map_or(Ok(()), Err)
}

fn measure_path(
    path: &ExportArtifactPath,
    limits: &mut OutputLimitAccumulator,
    mut set_bytes: Option<&mut u64>,
) {
    limits.observe_usize(
        OutputLimitCounter::ArtifactPathSegmentsPerPath,
        path.segments.len(),
    );
    let mut path_bytes = 0;
    for segment in &path.segments {
        limits.observe_usize(
            OutputLimitCounter::ArtifactPathSegmentBytes,
            segment.0.len(),
        );
        limits.add_usize(
            OutputLimitCounter::ArtifactPathBytesPerPath,
            &mut path_bytes,
            segment.0.len(),
        );
        if let Some(total) = &mut set_bytes {
            limits.add_usize(
                OutputLimitCounter::ArtifactPathBytesPerSet,
                total,
                segment.0.len(),
            );
        }
    }
}

fn validate_paths(artifacts: &[ExportArtifact]) -> Result<(), ExportError> {
    if artifacts.iter().any(|artifact| {
        !valid_path_segments(
            artifact
                .logical_path
                .segments
                .iter()
                .map(ExportArtifactPathSegment::as_str),
        ) || artifact.metadata.relationships.iter().any(|relationship| {
            !valid_path_segments(
                relationship
                    .target
                    .segments
                    .iter()
                    .map(ExportArtifactPathSegment::as_str),
            )
        })
    }) {
        return Err(invalid_output(InvalidOutputViolation::ArtifactPath));
    }
    Ok(())
}

fn validate_kind_and_media(artifacts: &[ExportArtifact]) -> Result<(), ExportError> {
    if artifacts
        .iter()
        .any(|artifact| !valid_kind_shape(&artifact.kind.0))
    {
        return Err(invalid_output(InvalidOutputViolation::ArtifactKind));
    }
    if artifacts.iter().any(|artifact| {
        artifact
            .metadata
            .media_type
            .as_ref()
            .is_some_and(|media_type| media_components(&media_type.0).is_none())
    }) {
        return Err(invalid_output(InvalidOutputViolation::MediaType));
    }
    Ok(())
}

/// Validate a resolved relationship graph whose records are in canonical order.
///
/// Duplicate detection relies on every artifact's relationships already being
/// sorted by `(kind, target)`.
fn validate_relationships(artifacts: &[ExportArtifact]) -> Result<(), ExportError> {
    debug_assert!(artifacts.iter().all(|artifact| artifact
        .metadata
        .relationships
        .windows(2)
        .all(|pair| (pair[0].kind, &pair[0].target) <= (pair[1].kind, &pair[1].target))));

    let paths: BTreeMap<&ExportArtifactPath, usize> = artifacts
        .iter()
        .enumerate()
        .map(|(index, artifact)| (&artifact.logical_path, index))
        .collect();
    let mut resolved = Vec::with_capacity(artifacts.len());

    for artifact in artifacts {
        let mut edges = Vec::with_capacity(artifact.metadata.relationships.len());
        for relationship in &artifact.metadata.relationships {
            let Some(target) = paths.get(&relationship.target).copied() else {
                return Err(invalid_output(InvalidOutputViolation::RelationshipTarget));
            };
            edges.push((relationship.kind, target));
        }
        resolved.push(edges);
    }

    if artifacts.iter().any(|artifact| {
        artifact
            .metadata
            .relationships
            .windows(2)
            .any(|pair| pair[0].kind == pair[1].kind && pair[0].target == pair[1].target)
    }) {
        return Err(invalid_output(
            InvalidOutputViolation::DuplicateRelationship,
        ));
    }

    for artifact in artifacts {
        let mut classifications = BTreeMap::new();
        for relationship in &artifact.metadata.relationships {
            if classifications
                .insert(&relationship.target, relationship.kind)
                .is_some_and(|previous| previous != relationship.kind)
            {
                return Err(invalid_output(
                    InvalidOutputViolation::ConflictingRelationshipClassification,
                ));
            }
        }
    }

    if resolved
        .iter()
        .enumerate()
        .any(|(source, edges)| edges.iter().any(|(_, target)| source == *target))
    {
        return Err(invalid_output(InvalidOutputViolation::RelationshipSelfEdge));
    }

    validate_lazy_relationships(&resolved)
}

fn validate_lazy_relationships(
    edges: &[Vec<(ExportArtifactRelationshipKind, usize)>],
) -> Result<(), ExportError> {
    let eager: Vec<Vec<usize>> = edges
        .iter()
        .map(|source| {
            source
                .iter()
                .filter_map(|(kind, target)| {
                    (*kind == ExportArtifactRelationshipKind::EagerLoad).then_some(*target)
                })
                .collect()
        })
        .collect();
    let (components, component_count) = eager_components(&eager);
    let mut component_edges = vec![Vec::new(); component_count];
    for (source, targets) in eager.iter().enumerate() {
        let source_component = components[source];
        for target in targets {
            let target_component = components[*target];
            if source_component != target_component {
                component_edges[source_component].push(target_component);
            }
        }
    }
    for targets in &mut component_edges {
        targets.sort_unstable();
        targets.dedup();
    }

    let mut queries_by_target = BTreeMap::<usize, BTreeSet<usize>>::new();
    for (source, source_edges) in edges.iter().enumerate() {
        for (kind, target) in source_edges {
            if *kind != ExportArtifactRelationshipKind::LazyLoad {
                continue;
            }
            let source_component = components[source];
            let target_component = components[*target];
            queries_by_target
                .entry(target_component)
                .or_default()
                .insert(source_component);
        }
    }

    if queries_by_target.is_empty() {
        return Ok(());
    }

    let topological_order = component_topological_order(&component_edges)?;
    let lazy_targets: Vec<_> = queries_by_target.keys().copied().collect();
    let mut reachable_targets = vec![0_u64; component_count];

    // Propagate 64 lazy targets at a time through the condensation DAG. This
    // shares graph traversal among all lazy sources without retaining an
    // unbounded all-pairs transitive closure.
    for target_block in lazy_targets.chunks(u64::BITS as usize) {
        reachable_targets.fill(0);
        for (bit, target) in target_block.iter().copied().enumerate() {
            reachable_targets[target] = 1_u64 << bit;
        }
        for source in topological_order.iter().copied().rev() {
            for target in &component_edges[source] {
                reachable_targets[source] |= reachable_targets[*target];
            }
        }
        for (bit, target) in target_block.iter().copied().enumerate() {
            let target_bit = 1_u64 << bit;
            if queries_by_target[&target]
                .iter()
                .any(|source| reachable_targets[*source] & target_bit != 0)
            {
                return Err(invalid_output(
                    InvalidOutputViolation::LazyTargetInEagerClosure,
                ));
            }
        }
    }
    Ok(())
}

fn component_topological_order(adjacency: &[Vec<usize>]) -> Result<Vec<usize>, ExportError> {
    let mut indegrees = vec![0_usize; adjacency.len()];
    for targets in adjacency {
        for target in targets {
            indegrees[*target] += 1;
        }
    }

    let mut ready: BTreeSet<_> = indegrees
        .iter()
        .enumerate()
        .filter_map(|(node, indegree)| (*indegree == 0).then_some(node))
        .collect();
    let mut order = Vec::with_capacity(adjacency.len());
    while let Some(node) = ready.pop_first() {
        order.push(node);
        for target in &adjacency[node] {
            indegrees[*target] -= 1;
            if indegrees[*target] == 0 {
                ready.insert(*target);
            }
        }
    }

    if order.len() != adjacency.len() {
        return Err(ExportError::internal_invariant(
            InternalInvariantEvidence::new(InternalInvariantViolation::SharedConstructorState),
        ));
    }
    Ok(order)
}

fn eager_components(adjacency: &[Vec<usize>]) -> (Vec<usize>, usize) {
    let mut visited = vec![false; adjacency.len()];
    let mut finish_order = Vec::with_capacity(adjacency.len());

    for start in 0..adjacency.len() {
        if visited[start] {
            continue;
        }
        visited[start] = true;
        let mut stack = vec![(start, 0usize)];
        while let Some((node, next_edge)) = stack.last_mut() {
            if *next_edge < adjacency[*node].len() {
                let target = adjacency[*node][*next_edge];
                *next_edge += 1;
                if !visited[target] {
                    visited[target] = true;
                    stack.push((target, 0));
                }
            } else {
                finish_order.push(*node);
                stack.pop();
            }
        }
    }

    let mut reverse = vec![Vec::new(); adjacency.len()];
    for (source, targets) in adjacency.iter().enumerate() {
        for target in targets {
            reverse[*target].push(source);
        }
    }

    let mut components = vec![usize::MAX; adjacency.len()];
    let mut component_count = 0;
    for start in finish_order.into_iter().rev() {
        if components[start] != usize::MAX {
            continue;
        }
        components[start] = component_count;
        let mut stack = vec![start];
        while let Some(node) = stack.pop() {
            for predecessor in &reverse[node] {
                if components[*predecessor] == usize::MAX {
                    components[*predecessor] = component_count;
                    stack.push(*predecessor);
                }
            }
        }
        component_count += 1;
    }
    (components, component_count)
}

#[cfg(test)]
mod tests {
    use super::{
        eager_components, valid_kind_shape, valid_media_component, valid_path_segment,
        validate_lazy_relationships, ExportArtifact, ExportArtifactFormatVersion,
        ExportArtifactKind, ExportArtifactMetadata, ExportArtifactPath, ExportArtifactPathSegment,
        ExportArtifactPayload, ExportArtifactRelationshipKind, ExportArtifactSet,
    };
    use crate::{ExportErrorEvidence, OutputLimitCounter, OutputLimitObservation};

    #[test]
    fn scalar_grammars_reject_ambiguous_or_host_specific_spellings() {
        assert!(valid_path_segment("messages.en.mjs"));
        assert!(valid_path_segment("メッセージ.mjs"));
        assert!(!valid_path_segment(".."));
        assert!(!valid_path_segment("a/b"));
        assert!(!valid_path_segment("a\\b"));
        assert!(!valid_path_segment("a\u{0007}b"));
        assert!(!valid_path_segment("a\u{061c}b"));
        assert!(!valid_path_segment("a\u{200e}b"));
        assert!(!valid_path_segment("a\u{202e}b"));
        assert!(!valid_path_segment("a\u{2066}b"));

        assert!(valid_kind_shape("com.example/custom-bundle"));
        assert!(!valid_kind_shape("example/custom-bundle"));
        assert!(!valid_kind_shape("com.example/custom_bundle"));

        assert!(valid_media_component("vnd.example+json"));
        assert!(!valid_media_component("Vnd.Example"));
        assert!(!valid_media_component("*"));
    }

    #[test]
    fn path_validation_counts_yielded_segments_instead_of_trusting_size_hint() {
        struct MisleadingSizeHint {
            yielded: bool,
        }

        impl Iterator for MisleadingSizeHint {
            type Item = &'static str;

            fn next(&mut self) -> Option<Self::Item> {
                if self.yielded {
                    None
                } else {
                    self.yielded = true;
                    Some("one")
                }
            }

            fn size_hint(&self) -> (usize, Option<usize>) {
                (65, Some(65))
            }
        }

        let path = ExportArtifactPath::try_new(MisleadingSizeHint { yielded: false }).unwrap();
        assert_eq!(path.segments().len(), 1);
        assert_eq!(path.segments()[0].as_str(), "one");
    }

    #[test]
    fn iterative_eager_components_handle_cycles_without_recursion() {
        let adjacency = vec![vec![1], vec![2], vec![0, 3], Vec::new()];
        let (components, count) = eager_components(&adjacency);
        assert_eq!(count, 2);
        assert_eq!(components[0], components[1]);
        assert_eq!(components[1], components[2]);
        assert_ne!(components[2], components[3]);
    }

    #[test]
    fn eager_closure_matches_every_three_artifact_graph() {
        const SLOTS: [(usize, usize); 6] = [(0, 1), (0, 2), (1, 0), (1, 2), (2, 0), (2, 1)];

        for mut encoding in 0..3_usize.pow(SLOTS.len() as u32) {
            let original_encoding = encoding;
            let mut edges = vec![Vec::new(); 3];
            for (source, target) in SLOTS {
                let kind = match encoding % 3 {
                    0 => None,
                    1 => Some(ExportArtifactRelationshipKind::EagerLoad),
                    2 => Some(ExportArtifactRelationshipKind::LazyLoad),
                    _ => unreachable!(),
                };
                encoding /= 3;
                if let Some(kind) = kind {
                    edges[source].push((kind, target));
                }
            }

            let expected_rejection = edges.iter().enumerate().any(|(source, outgoing)| {
                outgoing.iter().any(|(kind, target)| {
                    *kind == ExportArtifactRelationshipKind::LazyLoad
                        && eager_reaches(&edges, source, *target)
                })
            });
            assert_eq!(
                validate_lazy_relationships(&edges).is_err(),
                expected_rejection,
                "graph encoding {original_encoding} selected the wrong eager-closure result"
            );
        }
    }

    #[test]
    fn lazy_reachability_shares_dag_propagation_across_target_blocks() {
        let mut valid = vec![Vec::new(); 132];
        valid[0].extend((1..=130).map(|target| (ExportArtifactRelationshipKind::LazyLoad, target)));
        assert!(validate_lazy_relationships(&valid).is_ok());

        let mut invalid = valid;
        invalid[0].push((ExportArtifactRelationshipKind::EagerLoad, 131));
        invalid[131].push((ExportArtifactRelationshipKind::EagerLoad, 130));
        let error = validate_lazy_relationships(&invalid).unwrap_err();
        let ExportErrorEvidence::InvalidOutput(evidence) = error.evidence() else {
            panic!("expected invalid output evidence");
        };
        assert_eq!(
            evidence.violation(),
            crate::InvalidOutputViolation::LazyTargetInEagerClosure
        );
    }

    fn eager_reaches(
        edges: &[Vec<(ExportArtifactRelationshipKind, usize)>],
        source: usize,
        target: usize,
    ) -> bool {
        let mut visited = vec![false; edges.len()];
        let mut stack: Vec<_> = edges[source]
            .iter()
            .filter_map(|(kind, target)| {
                (*kind == ExportArtifactRelationshipKind::EagerLoad).then_some(*target)
            })
            .collect();
        while let Some(node) = stack.pop() {
            if node == target {
                return true;
            }
            if visited[node] {
                continue;
            }
            visited[node] = true;
            stack.extend(edges[node].iter().filter_map(|(kind, target)| {
                (*kind == ExportArtifactRelationshipKind::EagerLoad).then_some(*target)
            }));
        }
        false
    }

    #[test]
    fn exact_segment_arrays_are_the_only_path_identity() {
        let nested = ExportArtifactPath::try_new(["assets", "messages.mjs"]).unwrap();
        let joined = ExportArtifactPath::try_new(["assets/messages.mjs"]);
        assert_eq!(nested.segments().len(), 2);
        assert!(joined.is_err());
    }

    #[test]
    fn final_set_revalidation_selects_counter_precedence_for_internal_candidates() {
        let artifact = ExportArtifact {
            logical_path: ExportArtifactPath {
                segments: (0..65)
                    .map(|index| {
                        let value = if index == 0 {
                            "a".repeat(256)
                        } else {
                            "a".to_owned()
                        };
                        ExportArtifactPathSegment(value.into_boxed_str())
                    })
                    .collect(),
            },
            kind: ExportArtifactKind("com.example/custom".into()),
            format_version: ExportArtifactFormatVersion::DRAFT_V0_1,
            payload: ExportArtifactPayload(Box::default()),
            metadata: ExportArtifactMetadata::empty(),
        };

        let error = ExportArtifactSet::try_new(vec![artifact]).unwrap_err();
        let ExportErrorEvidence::OutputLimitExceeded(evidence) = error.evidence() else {
            panic!("expected output limit evidence");
        };
        assert_eq!(
            evidence.counter(),
            OutputLimitCounter::ArtifactPathSegmentsPerPath
        );
        assert_eq!(evidence.observation(), OutputLimitObservation::Exact(65));
    }

    #[test]
    fn duplicate_path_precedes_later_invalid_scalar_checks() {
        let path = ExportArtifactPath::try_new(["duplicate"]).unwrap();
        let artifact = |kind: &str| ExportArtifact {
            logical_path: path.clone(),
            kind: ExportArtifactKind(kind.into()),
            format_version: ExportArtifactFormatVersion::DRAFT_V0_1,
            payload: ExportArtifactPayload(Box::default()),
            metadata: ExportArtifactMetadata::empty(),
        };
        let error =
            ExportArtifactSet::try_new(vec![artifact("not-a-kind"), artifact("com.example/valid")])
                .unwrap_err();
        let ExportErrorEvidence::InvalidOutput(evidence) = error.evidence() else {
            panic!("expected invalid output evidence");
        };
        assert_eq!(
            evidence.violation(),
            crate::InvalidOutputViolation::DuplicateArtifactPath
        );
    }

    #[test]
    fn malformed_overlength_kind_remains_an_invalid_scalar() {
        let artifact = ExportArtifact {
            logical_path: ExportArtifactPath::try_new(["artifact"]).unwrap(),
            kind: ExportArtifactKind("X".repeat(256).into_boxed_str()),
            format_version: ExportArtifactFormatVersion::DRAFT_V0_1,
            payload: ExportArtifactPayload(Box::default()),
            metadata: ExportArtifactMetadata::empty(),
        };

        let error = ExportArtifactSet::try_new(vec![artifact]).unwrap_err();
        let ExportErrorEvidence::InvalidOutput(evidence) = error.evidence() else {
            panic!("expected invalid output evidence");
        };
        assert_eq!(
            evidence.violation(),
            crate::InvalidOutputViolation::ArtifactKind
        );
    }
}
