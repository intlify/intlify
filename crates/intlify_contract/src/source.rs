// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Portable source evidence and definition payload values.
//!
//! This module owns checked UTF-8 spans, source origins, host entry references,
//! opaque MF2 message payloads, and bounded human-readable selector reasons. It
//! preserves exact offsets and spellings needed for artifacts, fingerprints, and
//! later diagnostic attribution.
//!
//! It does not retain source buffers, decode host formats, parse MF2 messages, or
//! convert byte offsets into line and column locations. Producers and diagnostic
//! consumers own those operations.

use crate::error::{ValueConstructionError, ValueGrammar, ValueRangeError};
use crate::fingerprint::{write_tagged_field, FingerprintPayload};
use crate::{ArtifactLimitEvidence, LinkLimitCounter, LinkLimits, SourceDocumentIdentity};

/// Half-open UTF-8 byte range in an exact source document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceUtf8Span {
    start: u32,
    end: u32,
}

impl SourceUtf8Span {
    /// Validate and retain one half-open byte range.
    pub const fn try_new(start: u32, end: u32) -> Result<Self, ValueConstructionError> {
        if start > end {
            return Err(ValueConstructionError::Range(
                ValueRangeError::ReversedUtf8Span,
            ));
        }
        Ok(Self { start, end })
    }

    /// Return the inclusive start offset.
    #[must_use]
    pub const fn start(self) -> u32 {
        self.start
    }

    /// Return the exclusive end offset.
    #[must_use]
    pub const fn end(self) -> u32 {
        self.end
    }
}

impl FingerprintPayload for SourceUtf8Span {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        write_tagged_field(0x01, &self.start.to_be_bytes(), output);
        write_tagged_field(0x02, &self.end.to_be_bytes(), output);
    }
}

/// Optional diagnostic source evidence for one reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SourceOrigin {
    source: SourceDocumentIdentity,
    span: SourceUtf8Span,
}

impl SourceOrigin {
    /// Construct source evidence from independently checked components.
    #[must_use]
    pub const fn new(source: SourceDocumentIdentity, span: SourceUtf8Span) -> Self {
        Self { source, span }
    }

    /// Return the portable source identity.
    #[must_use]
    pub const fn source(&self) -> &SourceDocumentIdentity {
        &self.source
    }

    /// Return the half-open UTF-8 byte span.
    #[must_use]
    pub const fn span(&self) -> SourceUtf8Span {
        self.span
    }
}

/// Exact host-format structural path for one entry.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryStructuralPath(Box<str>);

impl EntryStructuralPath {
    /// Retain an exact structural path, including the valid empty root path.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        LinkLimitCounter::EntryStructuralPathBytes.check_construction_limit(value.len() as u64)?;
        Ok(Self(value))
    }

    /// Return the exact host-format serialization.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limit(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        LinkLimitCounter::EntryStructuralPathBytes.check_artifact_limit(self.0.len() as u64, limits)
    }
}

impl FingerprintPayload for EntryStructuralPath {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

/// Portable identity for one concrete entry occurrence within a source.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EntryReference {
    structural_path: EntryStructuralPath,
    occurrence: u32,
}

impl EntryReference {
    /// Construct an entry reference from independently checked fields.
    #[must_use]
    pub const fn new(structural_path: EntryStructuralPath, occurrence: u32) -> Self {
        Self {
            structural_path,
            occurrence,
        }
    }

    /// Return the exact structural path.
    #[must_use]
    pub const fn structural_path(&self) -> &EntryStructuralPath {
        &self.structural_path
    }

    /// Return the zero-based occurrence among equal structural paths.
    #[must_use]
    pub const fn occurrence(&self) -> u32 {
        self.occurrence
    }
}

impl FingerprintPayload for EntryReference {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        write_tagged_field(0x01, self.structural_path.as_str().as_bytes(), output);
        write_tagged_field(0x02, &self.occurrence.to_be_bytes(), output);
    }
}

/// Exact opaque MF2 source payload carried by one definition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MessagePayload(Box<str>);

impl MessagePayload {
    /// Retain an exact message payload, including an empty message.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        LinkLimitCounter::MessageBytes.check_construction_limit(value.len() as u64)?;
        Ok(Self(value))
    }

    /// Return the exact MF2 source payload.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limit(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        LinkLimitCounter::MessageBytes.check_artifact_limit(self.0.len() as u64, limits)
    }
}

impl FingerprintPayload for MessagePayload {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

/// Exact human-readable evidence explaining a widened selector.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ReasonText(Box<str>);

impl ReasonText {
    /// Validate and retain one non-empty reason.
    pub fn try_new(value: impl Into<Box<str>>) -> Result<Self, ValueConstructionError> {
        let value = value.into();
        if value.is_empty() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
        }
        LinkLimitCounter::ReasonBytes.check_construction_limit(value.len() as u64)?;
        if value.chars().any(is_forbidden_reason_control) {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
        }
        Ok(Self(value))
    }

    /// Return the exact reason text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_limit(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        LinkLimitCounter::ReasonBytes.check_artifact_limit(self.0.len() as u64, limits)
    }
}

impl FingerprintPayload for ReasonText {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.0.as_bytes());
    }
}

fn is_forbidden_reason_control(value: char) -> bool {
    let scalar = value as u32;
    matches!(scalar, 0x00..=0x08 | 0x0b..=0x0c | 0x0e..=0x1f | 0x7f..=0x9f)
}

#[cfg(test)]
mod tests {
    use super::{EntryReference, EntryStructuralPath, MessagePayload, ReasonText, SourceUtf8Span};
    use crate::fingerprint::FingerprintPayload;
    use crate::{LinkLimitCounter, ValueConstructionError, ValueGrammar, ValueRangeError};
    use crate::{LinkLimitObservation, LinkLimits};

    #[test]
    fn source_span_accepts_empty_and_rejects_reversed_ranges() {
        assert_eq!(SourceUtf8Span::try_new(0, 0).unwrap().start(), 0);
        assert_eq!(SourceUtf8Span::try_new(10, 10).unwrap().end(), 10);
        assert!(matches!(
            SourceUtf8Span::try_new(2, 1),
            Err(ValueConstructionError::Range(
                ValueRangeError::ReversedUtf8Span
            ))
        ));
    }

    #[test]
    fn entry_reference_fingerprint_is_exact_tagged_record() {
        let reference = EntryReference::new(EntryStructuralPath::try_new("/a").unwrap(), 7);
        assert_eq!(
            reference.fingerprint_payload().as_ref(),
            [0x01, 0, 0, 0, 0, 0, 0, 0, 2, b'/', b'a', 0x02, 0, 0, 0, 0, 0, 0, 0, 4, 0, 0, 0, 7]
        );
    }

    #[test]
    fn message_payload_is_opaque_and_empty_is_valid() {
        assert_eq!(MessagePayload::try_new("").unwrap().as_str(), "");
        assert_eq!(
            MessagePayload::try_new("{invalid MF2").unwrap().as_str(),
            "{invalid MF2"
        );
        assert!(matches!(
            MessagePayload::try_new("a".repeat(1_048_577)),
            Err(ValueConstructionError::FieldLimit {
                counter: LinkLimitCounter::MessageBytes,
                limit: 1_048_576,
                attempted: 1_048_577,
            })
        ));
    }

    #[test]
    fn reason_text_allows_only_the_documented_controls() {
        assert!(ReasonText::try_new("\tline one\r\nline two").is_ok());
        assert!(ReasonText::try_new("   ").is_ok());
        assert!(matches!(
            ReasonText::try_new(""),
            Err(ValueConstructionError::Grammar(ValueGrammar::Empty))
        ));
        for invalid in ['\0', '\u{000b}', '\u{007f}', '\u{0080}', '\u{009f}'] {
            assert!(matches!(
                ReasonText::try_new(format!("a{invalid}b")),
                Err(ValueConstructionError::Grammar(ValueGrammar::Invalid))
            ));
        }
    }

    #[test]
    fn source_value_lower_limits_accept_exact_and_reject_first_over() {
        let path = EntryStructuralPath::try_new("é").unwrap();
        let path_zero = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::EntryStructuralPathBytes, 0)
            .unwrap();
        assert_eq!(
            path.revalidate_limit(&path_zero).unwrap_err().observation(),
            LinkLimitObservation::Exact(1)
        );
        let path_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::EntryStructuralPathBytes, 2)
            .unwrap();
        assert!(path.revalidate_limit(&path_exact).is_ok());

        let message = MessagePayload::try_new("é").unwrap();
        let message_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::MessageBytes, 1)
            .unwrap();
        assert_eq!(
            message
                .revalidate_limit(&message_first_over)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(2)
        );
        let message_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::MessageBytes, 2)
            .unwrap();
        assert!(message.revalidate_limit(&message_exact).is_ok());

        let reason = ReasonText::try_new("why").unwrap();
        let reason_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReasonBytes, 2)
            .unwrap();
        assert_eq!(
            reason
                .revalidate_limit(&reason_first_over)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(3)
        );
        let reason_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::ReasonBytes, 3)
            .unwrap();
        assert!(reason.revalidate_limit(&reason_exact).is_ok());
    }
}
