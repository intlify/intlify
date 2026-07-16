// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fmt;
use std::sync::Arc;

use crate::Utf8ByteSpan;

/// Caller error when mapping a message-local span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetMapError {
    /// The supplied start offset is greater than the end offset.
    Reversed {
        /// Supplied start offset.
        start: u32,
        /// Supplied end offset.
        end: u32,
    },
    /// The supplied end exceeds the complete message byte length.
    OutOfBounds {
        /// Supplied end offset.
        end: u32,
        /// Complete message byte length.
        message_len: u32,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SegmentKind {
    Identity,
    Unescape,
    RawOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct OffsetMapSegment {
    kind: SegmentKind,
    message_span: Utf8ByteSpan,
    raw_span: Utf8ByteSpan,
}

impl OffsetMapSegment {
    const fn identity(message_span: Utf8ByteSpan, raw_span: Utf8ByteSpan) -> Self {
        Self {
            kind: SegmentKind::Identity,
            message_span,
            raw_span,
        }
    }

    const fn unescape(message_span: Utf8ByteSpan, raw_span: Utf8ByteSpan) -> Self {
        Self {
            kind: SegmentKind::Unescape,
            message_span,
            raw_span,
        }
    }

    const fn raw_only(message_position: u32, raw_span: Utf8ByteSpan) -> Self {
        Self {
            kind: SegmentKind::RawOnly,
            message_span: Utf8ByteSpan::new(message_position, message_position),
            raw_span,
        }
    }
}

/// Validated monotonic mapping from message-local bytes to raw host bytes.
#[derive(Clone)]
pub struct MessageOffsetMap {
    segments: Arc<[OffsetMapSegment]>,
    raw_value_span: Utf8ByteSpan,
    message_len: u32,
    empty_message_anchor: Option<u32>,
}

impl MessageOffsetMap {
    /// Map a message-local half-open span into absolute host-document bytes.
    ///
    /// Ordered byte positions need not be Unicode scalar boundaries. Escape
    /// units are expanded to complete raw escape ranges and raw-only syntax is
    /// never consumed at the outer boundary of a mapped non-empty range.
    pub fn map_span(&self, span: Utf8ByteSpan) -> Result<Utf8ByteSpan, OffsetMapError> {
        if span.start() > span.end() {
            return Err(OffsetMapError::Reversed {
                start: span.start(),
                end: span.end(),
            });
        }
        if span.end() > self.message_len {
            return Err(OffsetMapError::OutOfBounds {
                end: span.end(),
                message_len: self.message_len,
            });
        }

        if self.message_len == 0 {
            let anchor = self
                .empty_message_anchor
                .expect("validated empty offset maps always contain an anchor");
            return Ok(Utf8ByteSpan::new(anchor, anchor));
        }

        if span.is_empty() {
            let position = self.map_empty_position(span.start());
            return Ok(Utf8ByteSpan::new(position, position));
        }

        Ok(Utf8ByteSpan::new(
            self.map_range_start(span.start()),
            self.map_range_end(span.end()),
        ))
    }

    /// Return the complete message byte length accepted by `map_span`.
    #[must_use]
    pub const fn message_len(&self) -> u32 {
        self.message_len
    }

    /// Return the complete raw host value span covered by this map.
    #[must_use]
    pub const fn raw_value_span(&self) -> Utf8ByteSpan {
        self.raw_value_span
    }

    fn map_range_start(&self, position: u32) -> u32 {
        if let Some(segment) = self.segments.iter().find(|segment| {
            segment.kind != SegmentKind::RawOnly
                && segment.message_span.start() <= position
                && position < segment.message_span.end()
        }) {
            return match segment.kind {
                SegmentKind::Identity => {
                    segment.raw_span.start() + position - segment.message_span.start()
                }
                SegmentKind::Unescape => segment.raw_span.start(),
                SegmentKind::RawOnly => unreachable!(),
            };
        }

        unreachable!("validated maps cover every non-terminal message byte")
    }

    fn map_range_end(&self, position: u32) -> u32 {
        if let Some(segment) = self.segments.iter().find(|segment| {
            segment.kind != SegmentKind::RawOnly
                && segment.message_span.start() < position
                && position <= segment.message_span.end()
        }) {
            return match segment.kind {
                SegmentKind::Identity => {
                    segment.raw_span.start() + position - segment.message_span.start()
                }
                SegmentKind::Unescape => segment.raw_span.end(),
                SegmentKind::RawOnly => unreachable!(),
            };
        }

        unreachable!("validated maps cover every nonzero message end")
    }

    fn map_empty_position(&self, position: u32) -> u32 {
        if let Some(segment) = self.segments.iter().find(|segment| {
            segment.kind != SegmentKind::RawOnly
                && segment.message_span.start() < position
                && position < segment.message_span.end()
        }) {
            return match segment.kind {
                SegmentKind::Identity => {
                    segment.raw_span.start() + position - segment.message_span.start()
                }
                SegmentKind::Unescape => segment.raw_span.start(),
                SegmentKind::RawOnly => unreachable!(),
            };
        }

        if let Some(next) = self.segments.iter().find(|segment| {
            segment.kind != SegmentKind::RawOnly && segment.message_span.start() == position
        }) {
            return next.raw_span.start();
        }

        self.segments
            .iter()
            .find(|segment| {
                segment.kind == SegmentKind::RawOnly
                    && segment.message_span.start() == self.message_len
            })
            .map_or(self.raw_value_span.end(), |segment| {
                segment.raw_span.start()
            })
    }
}

impl fmt::Debug for MessageOffsetMap {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MessageOffsetMap")
            .field("raw_value_span", &self.raw_value_span)
            .field("message_len", &self.message_len)
            .field("segment_count", &self.segments.len())
            .field("empty_message_anchor", &self.empty_message_anchor)
            .finish()
    }
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OffsetMapInvariantError {
    RawValueSpanReversed,
    RawValueSpanOutOfBounds,
    MessageLengthOutOfBounds,
    RawCoverage,
    MessageCoverage,
    SegmentKindLength,
    Utf8Boundary,
    NonCanonical,
    MissingEmptyMessageAnchor,
    UnexpectedEmptyMessageAnchor,
    EmptyMessageAnchorOutOfBounds,
}

/// Crate-private canonicalizing builder used by host adapters.
#[allow(dead_code)]
#[derive(Debug)]
pub(crate) struct MessageOffsetMapBuilder {
    raw_value_span: Utf8ByteSpan,
    segments: Vec<OffsetMapSegment>,
    empty_message_anchor: Option<u32>,
    duplicate_empty_message_anchor: bool,
}

#[allow(dead_code)]
impl MessageOffsetMapBuilder {
    pub(crate) const fn new(raw_value_span: Utf8ByteSpan) -> Self {
        Self {
            raw_value_span,
            segments: Vec::new(),
            empty_message_anchor: None,
            duplicate_empty_message_anchor: false,
        }
    }

    pub(crate) fn push_identity(&mut self, message_span: Utf8ByteSpan, raw_span: Utf8ByteSpan) {
        let next = OffsetMapSegment::identity(message_span, raw_span);
        if let Some(previous) = self.segments.last_mut() {
            if previous.kind == SegmentKind::Identity
                && previous.message_span.end() == next.message_span.start()
                && previous.raw_span.end() == next.raw_span.start()
            {
                previous.message_span =
                    Utf8ByteSpan::new(previous.message_span.start(), next.message_span.end());
                previous.raw_span =
                    Utf8ByteSpan::new(previous.raw_span.start(), next.raw_span.end());
                return;
            }
        }
        self.segments.push(next);
    }

    pub(crate) fn push_unescape(&mut self, message_span: Utf8ByteSpan, raw_span: Utf8ByteSpan) {
        self.segments
            .push(OffsetMapSegment::unescape(message_span, raw_span));
    }

    pub(crate) fn push_raw_only(&mut self, message_position: u32, raw_span: Utf8ByteSpan) {
        let next = OffsetMapSegment::raw_only(message_position, raw_span);
        if let Some(previous) = self.segments.last_mut() {
            if previous.kind == SegmentKind::RawOnly
                && previous.message_span.start() == message_position
                && previous.raw_span.end() == next.raw_span.start()
            {
                previous.raw_span =
                    Utf8ByteSpan::new(previous.raw_span.start(), next.raw_span.end());
                return;
            }
        }
        self.segments.push(next);
    }

    pub(crate) fn set_empty_message_anchor(
        &mut self,
        anchor: u32,
    ) -> Result<(), OffsetMapInvariantError> {
        if self.empty_message_anchor.is_some() {
            self.duplicate_empty_message_anchor = true;
            return Err(OffsetMapInvariantError::UnexpectedEmptyMessageAnchor);
        }
        self.empty_message_anchor = Some(anchor);
        Ok(())
    }

    pub(crate) fn segment_count(&self) -> usize {
        self.segments.len()
    }

    pub(crate) fn finish(
        self,
        raw_source: &str,
        message_text: &str,
    ) -> Result<MessageOffsetMap, OffsetMapInvariantError> {
        let message_len = u32::try_from(message_text.len())
            .map_err(|_| OffsetMapInvariantError::MessageLengthOutOfBounds)?;
        self.validate_outer_bounds(raw_source, message_text, message_len)?;
        self.validate_segments(raw_source, message_text, message_len)?;
        self.validate_anchor(raw_source, message_len)?;

        Ok(MessageOffsetMap {
            segments: self.segments.into(),
            raw_value_span: self.raw_value_span,
            message_len,
            empty_message_anchor: self.empty_message_anchor,
        })
    }

    fn validate_outer_bounds(
        &self,
        raw_source: &str,
        message_text: &str,
        message_len: u32,
    ) -> Result<(), OffsetMapInvariantError> {
        if self.raw_value_span.start() > self.raw_value_span.end() {
            return Err(OffsetMapInvariantError::RawValueSpanReversed);
        }

        let raw_end = usize::try_from(self.raw_value_span.end())
            .map_err(|_| OffsetMapInvariantError::RawValueSpanOutOfBounds)?;
        if raw_end > raw_source.len() {
            return Err(OffsetMapInvariantError::RawValueSpanOutOfBounds);
        }

        if !is_boundary(raw_source, self.raw_value_span.start())
            || !is_boundary(raw_source, self.raw_value_span.end())
            || !is_boundary(message_text, 0)
            || !is_boundary(message_text, message_len)
        {
            return Err(OffsetMapInvariantError::Utf8Boundary);
        }
        Ok(())
    }

    fn validate_segments(
        &self,
        raw_source: &str,
        message_text: &str,
        message_len: u32,
    ) -> Result<(), OffsetMapInvariantError> {
        let mut expected_raw = self.raw_value_span.start();
        let mut expected_message = 0;
        let mut previous: Option<OffsetMapSegment> = None;

        for segment in &self.segments {
            if segment.raw_span.start() != expected_raw
                || segment.raw_span.end() <= segment.raw_span.start()
                || segment.raw_span.end() > self.raw_value_span.end()
            {
                return Err(OffsetMapInvariantError::RawCoverage);
            }
            if !is_boundary(raw_source, segment.raw_span.start())
                || !is_boundary(raw_source, segment.raw_span.end())
                || !is_boundary(message_text, segment.message_span.start())
                || !is_boundary(message_text, segment.message_span.end())
            {
                return Err(OffsetMapInvariantError::Utf8Boundary);
            }

            match segment.kind {
                SegmentKind::Identity => {
                    validate_message_segment(segment, expected_message, message_len)?;
                    if segment.message_span.checked_len() != segment.raw_span.checked_len() {
                        return Err(OffsetMapInvariantError::SegmentKindLength);
                    }
                    if slice(raw_source, segment.raw_span)
                        != slice(message_text, segment.message_span)
                    {
                        return Err(OffsetMapInvariantError::SegmentKindLength);
                    }
                    expected_message = segment.message_span.end();
                }
                SegmentKind::Unescape => {
                    validate_message_segment(segment, expected_message, message_len)?;
                    expected_message = segment.message_span.end();
                }
                SegmentKind::RawOnly => {
                    if !segment.message_span.is_empty()
                        || segment.message_span.start() != expected_message
                    {
                        return Err(OffsetMapInvariantError::MessageCoverage);
                    }
                }
            }

            if let Some(previous) = previous {
                let coalescible_identity = previous.kind == SegmentKind::Identity
                    && segment.kind == SegmentKind::Identity
                    && previous.message_span.end() == segment.message_span.start()
                    && previous.raw_span.end() == segment.raw_span.start();
                let coalescible_raw_only = previous.kind == SegmentKind::RawOnly
                    && segment.kind == SegmentKind::RawOnly
                    && previous.message_span.start() == segment.message_span.start()
                    && previous.raw_span.end() == segment.raw_span.start();
                if coalescible_identity || coalescible_raw_only {
                    return Err(OffsetMapInvariantError::NonCanonical);
                }
            }

            expected_raw = segment.raw_span.end();
            previous = Some(*segment);
        }

        if expected_raw != self.raw_value_span.end() {
            return Err(OffsetMapInvariantError::RawCoverage);
        }
        if expected_message != message_len {
            return Err(OffsetMapInvariantError::MessageCoverage);
        }
        Ok(())
    }

    fn validate_anchor(
        &self,
        raw_source: &str,
        message_len: u32,
    ) -> Result<(), OffsetMapInvariantError> {
        if self.duplicate_empty_message_anchor {
            return Err(OffsetMapInvariantError::UnexpectedEmptyMessageAnchor);
        }

        match (message_len, self.empty_message_anchor) {
            (0, None) => Err(OffsetMapInvariantError::MissingEmptyMessageAnchor),
            (0, Some(anchor)) => {
                if anchor < self.raw_value_span.start()
                    || anchor > self.raw_value_span.end()
                    || !is_boundary(raw_source, anchor)
                {
                    Err(OffsetMapInvariantError::EmptyMessageAnchorOutOfBounds)
                } else {
                    Ok(())
                }
            }
            (_, Some(_)) => Err(OffsetMapInvariantError::UnexpectedEmptyMessageAnchor),
            (_, None) => Ok(()),
        }
    }
}

fn validate_message_segment(
    segment: &OffsetMapSegment,
    expected_message: u32,
    message_len: u32,
) -> Result<(), OffsetMapInvariantError> {
    if segment.message_span.start() != expected_message
        || segment.message_span.end() <= segment.message_span.start()
        || segment.message_span.end() > message_len
    {
        return Err(OffsetMapInvariantError::MessageCoverage);
    }
    Ok(())
}

fn is_boundary(value: &str, offset: u32) -> bool {
    usize::try_from(offset)
        .ok()
        .is_some_and(|offset| value.is_char_boundary(offset))
}

fn slice(value: &str, span: Utf8ByteSpan) -> &str {
    let start = usize::try_from(span.start()).expect("validated u32 offset fits usize");
    let end = usize::try_from(span.end()).expect("validated u32 offset fits usize");
    &value[start..end]
}

#[cfg(test)]
mod tests {
    use super::{MessageOffsetMapBuilder, OffsetMapError, OffsetMapInvariantError};
    use crate::Utf8ByteSpan;

    fn escaped_newline_map() -> super::MessageOffsetMap {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 6));
        builder.push_raw_only(0, Utf8ByteSpan::new(0, 1));
        builder.push_identity(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(1, 2));
        builder.push_unescape(Utf8ByteSpan::new(1, 2), Utf8ByteSpan::new(2, 4));
        builder.push_identity(Utf8ByteSpan::new(2, 3), Utf8ByteSpan::new(4, 5));
        builder.push_raw_only(3, Utf8ByteSpan::new(5, 6));
        builder.finish("\"a\\nb\"", "a\nb").unwrap()
    }

    #[test]
    fn maps_identity_escape_and_outer_raw_syntax() {
        let map = escaped_newline_map();

        assert_eq!(map.message_len(), 3);
        assert_eq!(map.raw_value_span(), Utf8ByteSpan::new(0, 6));
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(0, 3)),
            Ok(Utf8ByteSpan::new(1, 5))
        );
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(1, 2)),
            Ok(Utf8ByteSpan::new(2, 4))
        );
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(0, 1)),
            Ok(Utf8ByteSpan::new(1, 2))
        );
    }

    #[test]
    fn maps_empty_positions_before_next_message_byte_and_trailing_syntax() {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 5));
        builder.push_identity(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(0, 1));
        builder.push_raw_only(1, Utf8ByteSpan::new(1, 2));
        builder.push_identity(Utf8ByteSpan::new(1, 2), Utf8ByteSpan::new(2, 3));
        builder.push_raw_only(2, Utf8ByteSpan::new(3, 5));
        let map = builder.finish("a-b!!", "ab").unwrap();

        assert_eq!(
            map.map_span(Utf8ByteSpan::new(1, 1)),
            Ok(Utf8ByteSpan::new(2, 2))
        );
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(2, 2)),
            Ok(Utf8ByteSpan::new(3, 3))
        );
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(0, 2)),
            Ok(Utf8ByteSpan::new(0, 3))
        );
    }

    #[test]
    fn maps_empty_position_inside_unescape_to_escape_start() {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 8));
        builder.push_raw_only(0, Utf8ByteSpan::new(0, 1));
        builder.push_unescape(Utf8ByteSpan::new(0, 2), Utf8ByteSpan::new(1, 7));
        builder.push_raw_only(2, Utf8ByteSpan::new(7, 8));
        let map = builder.finish("\"\\u00e9\"", "é").unwrap();

        assert_eq!(
            map.map_span(Utf8ByteSpan::new(1, 1)),
            Ok(Utf8ByteSpan::new(1, 1))
        );
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(0, 2)),
            Ok(Utf8ByteSpan::new(1, 7))
        );
    }

    #[test]
    fn empty_message_uses_the_explicit_anchor() {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 2));
        builder.push_raw_only(0, Utf8ByteSpan::new(0, 2));
        builder.set_empty_message_anchor(1).unwrap();
        let map = builder.finish("\"\"", "").unwrap();

        assert_eq!(
            map.map_span(Utf8ByteSpan::new(0, 0)),
            Ok(Utf8ByteSpan::new(1, 1))
        );
    }

    #[test]
    fn validates_reversed_before_out_of_bounds() {
        let map = escaped_newline_map();

        assert_eq!(
            map.map_span(Utf8ByteSpan::new(5, 4)),
            Err(OffsetMapError::Reversed { start: 5, end: 4 })
        );
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(1, 4)),
            Err(OffsetMapError::OutOfBounds {
                end: 4,
                message_len: 3,
            })
        );
    }

    #[test]
    fn canonicalizes_adjacent_identity_and_raw_only_segments() {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 6));
        builder.push_raw_only(0, Utf8ByteSpan::new(0, 1));
        builder.push_raw_only(0, Utf8ByteSpan::new(1, 2));
        builder.push_identity(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(2, 3));
        builder.push_identity(Utf8ByteSpan::new(1, 2), Utf8ByteSpan::new(3, 4));
        builder.push_raw_only(2, Utf8ByteSpan::new(4, 6));

        assert_eq!(builder.segment_count(), 3);
        assert!(builder.finish("()ab!!", "ab").is_ok());
    }

    #[test]
    fn never_coalesces_unescape_units() {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 4));
        builder.push_unescape(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(0, 2));
        builder.push_unescape(Utf8ByteSpan::new(1, 2), Utf8ByteSpan::new(2, 4));

        assert_eq!(builder.segment_count(), 2);
        assert!(builder.finish("\\n\\t", "\n\t").is_ok());
    }

    #[test]
    fn rejects_raw_and_message_coverage_gaps() {
        let mut raw_gap = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 2));
        raw_gap.push_identity(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(1, 2));
        assert_eq!(
            raw_gap.finish("ab", "b").unwrap_err(),
            OffsetMapInvariantError::RawCoverage
        );

        let mut message_gap = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 1));
        message_gap.push_identity(Utf8ByteSpan::new(1, 2), Utf8ByteSpan::new(0, 1));
        assert_eq!(
            message_gap.finish("b", "ab").unwrap_err(),
            OffsetMapInvariantError::MessageCoverage
        );
    }

    #[test]
    fn rejects_utf8_boundary_violations() {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 2));
        builder.push_unescape(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(0, 1));

        assert_eq!(
            builder.finish("é", "a").unwrap_err(),
            OffsetMapInvariantError::Utf8Boundary
        );
    }

    #[test]
    fn rejects_identity_segments_whose_bytes_are_not_identical() {
        let mut builder = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 1));
        builder.push_identity(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(0, 1));

        assert_eq!(
            builder.finish("a", "b").unwrap_err(),
            OffsetMapInvariantError::SegmentKindLength
        );
    }

    #[test]
    fn rejects_missing_extra_duplicate_and_out_of_bounds_anchors() {
        let missing = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 0));
        assert_eq!(
            missing.finish("", "").unwrap_err(),
            OffsetMapInvariantError::MissingEmptyMessageAnchor
        );

        let mut extra = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 1));
        extra.push_identity(Utf8ByteSpan::new(0, 1), Utf8ByteSpan::new(0, 1));
        extra.set_empty_message_anchor(0).unwrap();
        assert_eq!(
            extra.finish("a", "a").unwrap_err(),
            OffsetMapInvariantError::UnexpectedEmptyMessageAnchor
        );

        let mut duplicate = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 0));
        duplicate.set_empty_message_anchor(0).unwrap();
        assert_eq!(
            duplicate.set_empty_message_anchor(0),
            Err(OffsetMapInvariantError::UnexpectedEmptyMessageAnchor)
        );
        assert_eq!(
            duplicate.finish("", "").unwrap_err(),
            OffsetMapInvariantError::UnexpectedEmptyMessageAnchor
        );

        let mut outside = MessageOffsetMapBuilder::new(Utf8ByteSpan::new(0, 0));
        outside.set_empty_message_anchor(1).unwrap();
        assert_eq!(
            outside.finish("", "").unwrap_err(),
            OffsetMapInvariantError::EmptyMessageAnchorOutOfBounds
        );
    }

    #[test]
    fn map_debug_does_not_expose_segment_representation() {
        let debug = format!("{:?}", escaped_newline_map());

        assert!(debug.contains("segment_count"));
        assert!(!debug.contains("Identity"));
        assert!(!debug.contains("Unescape"));
        assert!(!debug.contains("RawOnly"));
    }
}
