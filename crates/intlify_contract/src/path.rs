// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Portable project-relative paths and source-document identities.
//!
//! This module owns checked Unicode path segments, non-empty relative segment
//! sequences, their aggregate limits, and the namespace-qualified identity of a
//! source document. It preserves segment boundaries and exact spelling for
//! deterministic comparison and fingerprinting across hosts.
//!
//! It does not join native path strings, access the file system, canonicalize
//! platform separators, or resolve symbolic links. Host integrations map these
//! portable identities to local files under an already selected project root.

use crate::error::{ValueConstructionError, ValueGrammar};
use crate::fingerprint::{write_sequence, write_tagged_field, FingerprintPayload};
use crate::{ArtifactLimitEvidence, ArtifactNamespace, LinkLimitCounter, LinkLimits};

/// One exact Unicode segment in a portable project-relative path.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortablePathSegment(Box<str>);

impl PortablePathSegment {
    /// Validate and retain one portable path segment.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        LinkLimitCounter::PathSegmentBytes.check_construction_limit(value.len() as u64)?;
        if value.is_empty() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
        }
        if matches!(value.as_ref(), "." | "..") || value.contains('\0') || value.contains('/') {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
        }
        Ok(Self(value))
    }

    /// Return the exact segment spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    fn revalidate_limit(&self, limits: &LinkLimits) -> Result<(), ArtifactLimitEvidence> {
        LinkLimitCounter::PathSegmentBytes.check_artifact_limit(self.0.len() as u64, limits)
    }
}

impl FingerprintPayload for PortablePathSegment {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

/// Non-empty path relative to a host-bound project root.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortableRelativePath(Box<[PortablePathSegment]>);

impl PortableRelativePath {
    /// Validate and retain one exact ordered segment array.
    pub fn try_new(segments: Vec<PortablePathSegment>) -> Result<Self, ValueConstructionError> {
        LinkLimitCounter::PathSegments.check_construction_limit(segments.len() as u64)?;
        if segments.is_empty() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
        }

        let mut total = 0_u64;
        for segment in &segments {
            total = total
                .checked_add(segment.as_str().len() as u64)
                .unwrap_or(LinkLimitCounter::PathBytes.protocol_ceiling() + 1);
            LinkLimitCounter::PathBytes.check_construction_limit(total)?;
        }

        Ok(Self(segments.into_boxed_slice()))
    }

    /// Return the exact ordered path segments.
    #[must_use]
    pub fn segments(&self) -> &[PortablePathSegment] {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn decoded_bytes(&self) -> u64 {
        self.0
            .iter()
            .map(|segment| segment.as_str().len() as u64)
            .sum()
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limits(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        LinkLimitCounter::PathSegments.check_artifact_limit(self.0.len() as u64, limits)?;

        for segment in &self.0 {
            segment.revalidate_limit(limits)?;
        }

        let mut total = 0_u64;
        for segment in &self.0 {
            total = total
                .checked_add(segment.as_str().len() as u64)
                .expect("checked path components cannot overflow u64");
            LinkLimitCounter::PathBytes.check_artifact_limit(total, limits)?;
        }
        Ok(())
    }
}

impl FingerprintPayload for PortableRelativePath {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        write_sequence(
            self.0.iter().map(|segment| segment.as_str().as_bytes()),
            output,
        );
    }
}

/// Portable identity for one source document in the consuming project.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceDocumentIdentity {
    namespace: ArtifactNamespace,
    path: PortableRelativePath,
}

impl SourceDocumentIdentity {
    /// Construct a source identity from independently checked components.
    #[must_use]
    pub const fn new(namespace: ArtifactNamespace, path: PortableRelativePath) -> Self {
        Self { namespace, path }
    }

    /// Return the explicit artifact namespace.
    #[must_use]
    pub const fn namespace(&self) -> ArtifactNamespace {
        self.namespace
    }

    /// Return the exact portable path.
    #[must_use]
    pub const fn path(&self) -> &PortableRelativePath {
        &self.path
    }
}

impl FingerprintPayload for SourceDocumentIdentity {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        let namespace = [self.namespace.fingerprint_discriminant()];
        write_tagged_field(0x01, &namespace, output);
        let path = self.path.fingerprint_payload();
        write_tagged_field(0x02, &path, output);
    }
}

#[cfg(test)]
mod tests {
    use super::{PortablePathSegment, PortableRelativePath, SourceDocumentIdentity};
    use crate::fingerprint::FingerprintPayload;
    use crate::{
        ArtifactNamespace, LinkLimitCounter, LinkLimitObservation, LinkLimits,
        ValueConstructionError, ValueGrammar,
    };

    #[test]
    fn path_segments_retain_exact_bytes_without_separator_rewriting() {
        let segment = PortablePathSegment::try_new("checkout\\messages.json").unwrap();
        assert_eq!(segment.as_str(), "checkout\\messages.json");
        for invalid in ["", ".", "..", "a/b", "a\0b"] {
            assert!(matches!(
                PortablePathSegment::try_new(invalid),
                Err(ValueConstructionError::Grammar(
                    ValueGrammar::Empty | ValueGrammar::Invalid
                ))
            ));
        }
    }

    #[test]
    fn portable_path_boundaries_are_inclusive() {
        assert!(PortablePathSegment::try_new("a".repeat(4_096)).is_ok());
        assert!(matches!(
            PortablePathSegment::try_new("a".repeat(4_097)),
            Err(ValueConstructionError::FieldLimit {
                counter: LinkLimitCounter::PathSegmentBytes,
                limit: 4_096,
                attempted: 4_097,
            })
        ));

        let segments = (0..64)
            .map(|_| PortablePathSegment::try_new("a".repeat(4_096)).unwrap())
            .collect();
        assert!(PortableRelativePath::try_new(segments).is_ok());

        let segments = (0..64)
            .map(|_| PortablePathSegment::try_new("a".repeat(4_096)).unwrap())
            .chain(std::iter::once(PortablePathSegment::try_new("a").unwrap()))
            .collect();
        assert!(matches!(
            PortableRelativePath::try_new(segments),
            Err(ValueConstructionError::FieldLimit {
                counter: LinkLimitCounter::PathBytes,
                limit: 262_144,
                attempted: 262_145,
            })
        ));
    }

    #[test]
    fn path_order_compares_exact_segment_bytes_and_boundaries() {
        let short = PortableRelativePath::try_new(vec![PortablePathSegment::try_new("a").unwrap()])
            .unwrap();
        let long = PortableRelativePath::try_new(vec![
            PortablePathSegment::try_new("a").unwrap(),
            PortablePathSegment::try_new("b").unwrap(),
        ])
        .unwrap();
        let other = PortableRelativePath::try_new(vec![PortablePathSegment::try_new("b").unwrap()])
            .unwrap();
        assert!(short < long);
        assert!(long < other);
    }

    #[test]
    fn source_identity_fingerprint_is_namespace_and_path_record() {
        let source = SourceDocumentIdentity::new(
            ArtifactNamespace::Project,
            PortableRelativePath::try_new(vec![
                PortablePathSegment::try_new("src").unwrap(),
                PortablePathSegment::try_new("messages.ts").unwrap(),
            ])
            .unwrap(),
        );
        let payload = source.fingerprint_payload();
        assert_eq!(payload[0], 0x01);
        assert_eq!(payload[9], 0x00);
        assert_eq!(payload[10], 0x02);
    }

    #[test]
    fn lower_path_limits_follow_count_segment_and_running_total_order() {
        let path = PortableRelativePath::try_new(vec![
            PortablePathSegment::try_new("abc").unwrap(),
            PortablePathSegment::try_new("def").unwrap(),
        ])
        .unwrap();

        let count_zero = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathSegments, 0)
            .unwrap();
        let evidence = path.revalidate_limits(&count_zero).unwrap_err();
        assert_eq!(evidence.counter(), LinkLimitCounter::PathSegments);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(1));

        let segment_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathSegmentBytes, 2)
            .unwrap();
        let evidence = path.revalidate_limits(&segment_first_over).unwrap_err();
        assert_eq!(evidence.counter(), LinkLimitCounter::PathSegmentBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(3));

        let path_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathBytes, 6)
            .unwrap();
        assert!(path.revalidate_limits(&path_exact).is_ok());

        let path_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::PathBytes, 5)
            .unwrap();
        let evidence = path.revalidate_limits(&path_first_over).unwrap_err();
        assert_eq!(evidence.counter(), LinkLimitCounter::PathBytes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(6));
    }
}
