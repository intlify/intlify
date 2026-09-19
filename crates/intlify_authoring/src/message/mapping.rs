// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Composing the host's own decoding map with this crate's extraction map.
//!
//! Two decodings sit between host source and the MF2 a parser sees. The host
//! decodes its own escapes to produce message text, and this crate encodes
//! displayed text as MF2. Each step records where its output bytes came from,
//! and a reader that wants to point at source needs the composition of both.
//!
//! Neither map claims more than it knows. A run whose two sides have equal
//! length asserts a positional correspondence, so it can be sliced at any
//! offset; a run whose sides differ in length asserts only that the whole
//! emitted run came from the whole source run, so it is never subdivided. That
//! distinction is what keeps composition from manufacturing byte-for-byte
//! correspondence out of two coincidental lengths.
//!
//! A host that cannot assert a positional correspondence for an equal-length
//! run must split that run into separate segments. This is the host's
//! declaration about its own decoding, not an inference made here.

use super::literal::ExtractionSegment;
use crate::limits::{AuthoringLimits, LimitKind};
use crate::primitives::{ByteRange, Occurrence};

/// One contiguous correspondence between decoded text and host source.
///
/// The input side is a range in the text the host handed to this crate; the
/// source side is a range in the host's own source unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputSegment {
    input: ByteRange,
    source: ByteRange,
}

impl InputSegment {
    /// Retain one correspondence the host establishes.
    #[must_use]
    pub const fn new(input: ByteRange, source: ByteRange) -> Self {
        Self { input, source }
    }

    /// Return the covered range of decoded text.
    #[must_use]
    pub const fn input(self) -> ByteRange {
        self.input
    }

    /// Return the host source bytes this run came from.
    #[must_use]
    pub const fn source(self) -> ByteRange {
        self.source
    }

    /// Return whether this run asserts a positional correspondence.
    ///
    /// Equal lengths are the host's way of saying that decoded byte `k` of
    /// this run is source byte `k` of it. A host that means something weaker
    /// splits the run instead.
    const fn positional(self) -> bool {
        self.input.len() == self.source.len()
    }

    /// Map one offset inside this run onto host source.
    const fn at(self, offset: u64) -> u64 {
        if self.positional() {
            self.source.start() + (offset - self.input.start())
        } else {
            // A replacement has no interior positions to address, so any
            // offset inside it resolves to where the replacement begins.
            self.source.start()
        }
    }

    /// Map one decoded sub-range of this run onto host source.
    ///
    /// A positional run answers for the exact bytes asked about. A replacement
    /// answers for the whole of itself, because it has no interior: asking
    /// which source byte produced the second byte of a decoded escape has no
    /// answer finer than the escape.
    const fn span(self, low: u64, high: u64) -> ByteRange {
        if self.positional() {
            ByteRange::assume(self.at(low), self.at(high))
        } else {
            self.source
        }
    }
}

/// Why an input map cannot be composed.
///
/// These are host-integration failures rather than authoring mistakes: no edit
/// to the analyzed source could fix a map that does not describe it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MappingError {
    /// The map has no segments.
    ///
    /// A present map must describe the text, including an empty one. Having no
    /// map at all is expressed by omitting it, which keeps the extraction map
    /// in decoded coordinates instead of claiming an unknown source position.
    Empty,
    /// A segment does not begin where the previous one ended.
    Discontinuous,
    /// The segments do not cover the supplied text exactly.
    Coverage,
    /// A source range falls outside the occurrence it claims to come from.
    SourceOutsideOccurrence,
    /// An extraction segment addresses text the input map does not describe.
    Uncovered,
    /// The composed map needs more segments than the invocation allows.
    Limit(LimitKind),
}

/// Compose an extraction map with the host's input map.
///
/// The result addresses the same emitted MF2 bytes as `extraction`, so its
/// ordering, non-overlap, and coverage follow from that map. Only the source
/// side changes: it moves from decoded-text coordinates into the host's own
/// source unit.
///
/// A composed segment may be finer than its extraction segment. Where both
/// sides are positional, the extraction segment is split at the host's own run
/// boundaries so that a diagnostic resolves to the bytes it means rather than
/// to the whole literal.
pub fn compose_extraction_map(
    extraction: &[ExtractionSegment],
    input_map: &[InputSegment],
    text_len: u64,
    occurrence: &Occurrence,
    limits: &AuthoringLimits,
) -> Result<Vec<ExtractionSegment>, MappingError> {
    validate(input_map, text_len, occurrence)?;

    let mut composed: Vec<ExtractionSegment> = Vec::new();
    for segment in extraction {
        let decoded = segment.source();
        if decoded.is_empty() {
            let position = zero_width(input_map, decoded.start(), text_len)?;
            push(
                &mut composed,
                limits,
                segment.extracted(),
                range(position, position)?,
            )?;
            continue;
        }

        let overlapped = overlapping(input_map, decoded);
        if overlapped.is_empty() {
            return Err(MappingError::Uncovered);
        }

        // An extraction segment whose two sides have equal length carries the
        // same positional assertion as a host run, so it can be cut where the
        // host's own runs change. An escape cannot: its emitted bytes have no
        // individual sources, so it stays one segment over the whole span.
        let spans = overlapped.iter().map(|run| {
            let low = run.input.start().max(decoded.start());
            let high = run.input.end().min(decoded.end());
            (low, high, run.span(low, high))
        });

        if segment.extracted().len() == decoded.len() {
            for (low, high, source) in spans {
                let extracted = range(
                    segment.extracted().start() + (low - decoded.start()),
                    segment.extracted().start() + (high - decoded.start()),
                )?;
                push(&mut composed, limits, extracted, source)?;
            }
        } else {
            // The emitted bytes have no individual sources, so the whole run
            // answers for the whole of what the decoded bytes came from. The
            // hull is taken over the mapped spans rather than over the host
            // runs themselves, so an escape inside a long verbatim run
            // resolves to the character it escaped and not to the run.
            let mut start = u64::MAX;
            let mut end = 0_u64;
            for (_, _, source) in spans {
                start = start.min(source.start());
                end = end.max(source.end());
            }
            push(
                &mut composed,
                limits,
                segment.extracted(),
                range(start, end)?,
            )?;
        }
    }
    Ok(composed)
}

/// Check that an input map describes exactly the supplied text.
fn validate(
    input_map: &[InputSegment],
    text_len: u64,
    occurrence: &Occurrence,
) -> Result<(), MappingError> {
    let Some(first) = input_map.first() else {
        return Err(MappingError::Empty);
    };
    if first.input.start() != 0 {
        return Err(MappingError::Coverage);
    }
    let mut covered = 0_u64;
    for segment in input_map {
        if segment.input.start() != covered {
            return Err(MappingError::Discontinuous);
        }
        covered = segment.input.end();
        // The decoded text comes from inside the declaration's own syntax, so
        // a source range outside it is describing something else.
        if segment.source.start() < occurrence.range().start()
            || segment.source.end() > occurrence.range().end()
        {
            return Err(MappingError::SourceOutsideOccurrence);
        }
    }
    if covered != text_len {
        return Err(MappingError::Coverage);
    }
    Ok(())
}

/// Return the host position for a zero-width decoded position.
///
/// A generated delimiter marks an insertion point, so it resolves to an
/// insertion point in source rather than to a span of bytes.
fn zero_width(
    input_map: &[InputSegment],
    position: u64,
    text_len: u64,
) -> Result<u64, MappingError> {
    if let Some(run) = input_map
        .iter()
        .find(|run| run.input.start() < position && position < run.input.end())
    {
        return Ok(run.at(position));
    }
    // A run that begins here is the more precise answer than one that merely
    // ends here: text the host dropped, such as a line continuation, sits
    // between them and is not part of the decoded content.
    if let Some(run) = input_map.iter().find(|run| run.input.start() == position) {
        return Ok(run.source.start());
    }
    if position == text_len {
        return input_map
            .last()
            .map(|run| run.source.end())
            .ok_or(MappingError::Empty);
    }
    Err(MappingError::Uncovered)
}

/// Return the runs that actually produced the bytes of `decoded`.
///
/// Zero-width runs are excluded: they produced no decoded byte, so they are
/// not the origin of any byte in a nonempty range.
fn overlapping(input_map: &[InputSegment], decoded: ByteRange) -> &[InputSegment] {
    // Ends are non-decreasing across a contiguous map, so the first run that
    // can contribute is the first whose end passes the start of the range.
    let first = input_map.partition_point(|run| run.input.end() <= decoded.start());
    let rest = &input_map[first..];
    let count = rest.partition_point(|run| run.input.start() < decoded.end());
    let window = &rest[..count];
    // Zero-width runs can sit at either end of the window, so the empties are
    // trimmed from both sides rather than searched for a single boundary.
    let Some(lead) = window.iter().position(|run| !run.input.is_empty()) else {
        return &[];
    };
    let trail = window
        .iter()
        .rposition(|run| !run.input.is_empty())
        .unwrap_or(lead);
    &window[lead..=trail]
}

fn push(
    composed: &mut Vec<ExtractionSegment>,
    limits: &AuthoringLimits,
    extracted: ByteRange,
    source: ByteRange,
) -> Result<(), MappingError> {
    if composed.len() as u64 >= limits.extraction_segments {
        return Err(MappingError::Limit(LimitKind::ExtractionSegments));
    }
    composed.push(ExtractionSegment::new(extracted, source));
    Ok(())
}

fn range(start: u64, end: u64) -> Result<ByteRange, MappingError> {
    ByteRange::new(start, end).map_err(|_| MappingError::Uncovered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::tests::generous;
    use crate::message::literal;
    use crate::primitives::{OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot};
    use intlify_shared_json::token::VersionedIdentity;

    /// A declaration occupying `range` inside a 64-byte unit.
    fn occurrence(start: u64, end: u64) -> Occurrence {
        let source = SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            "checkout",
            "1",
            VersionedIdentity::literal("intlify-js-grammar", "0"),
            64,
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap();
        Occurrence::new(
            source,
            ByteRange::new(start, end).unwrap(),
            OccurrenceRole::UiLiteral,
        )
        .unwrap()
    }

    fn segment(input: (u64, u64), source: (u64, u64)) -> InputSegment {
        InputSegment::new(
            ByteRange::new(input.0, input.1).unwrap(),
            ByteRange::new(source.0, source.1).unwrap(),
        )
    }

    /// Encode `text` as a literal and compose the result with `input_map`.
    fn compose(
        text: &str,
        input_map: &[InputSegment],
        at: (u64, u64),
    ) -> Vec<(u64, u64, u64, u64)> {
        let mut extraction = Vec::new();
        literal::encode(text, &generous(), &mut extraction).unwrap();
        let composed = compose_extraction_map(
            &extraction,
            input_map,
            text.len() as u64,
            &occurrence(at.0, at.1),
            &generous(),
        )
        .unwrap();
        // Composition must not disturb the emitted side: ordered, adjacent,
        // and covering every emitted byte.
        let mut covered = 0;
        for piece in &composed {
            assert_eq!(
                piece.extracted().start(),
                covered,
                "emitted side stays whole"
            );
            covered = piece.extracted().end();
        }
        composed
            .iter()
            .map(|piece| {
                (
                    piece.extracted().start(),
                    piece.extracted().end(),
                    piece.source().start(),
                    piece.source().end(),
                )
            })
            .collect()
    }

    #[test]
    fn a_verbatim_run_slices_and_the_generated_delimiters_become_insertion_points() {
        // `'Pay now'` occupying [10, 19), its content at [11, 18).
        let composed = compose("Pay now", &[segment((0, 7), (11, 18))], (10, 19));
        assert_eq!(
            composed,
            [(0, 2, 11, 11), (2, 9, 11, 18), (9, 11, 18, 18),],
            "the delimiters mark where content begins and ends, not bytes of it"
        );
    }

    #[test]
    fn a_host_escape_keeps_its_own_span_and_splits_the_run_around_it() {
        // `'a\nb'` with content at [11, 15): `a`, `\`, `n`, `b`. The escape is
        // two source bytes producing one decoded byte, so it is not sliceable.
        let map = [
            segment((0, 1), (11, 12)),
            segment((1, 2), (12, 14)),
            segment((2, 3), (14, 15)),
        ];
        let composed = compose("a\nb", &map, (10, 16));
        assert_eq!(
            composed,
            [
                (0, 2, 11, 11),
                (2, 3, 11, 12),
                (3, 4, 12, 14),
                (4, 5, 14, 15),
                (5, 7, 15, 15),
            ],
            "one emitted run splits at the host's own boundaries"
        );
    }

    #[test]
    fn an_mf2_escape_inside_a_verbatim_run_resolves_to_the_character_it_escaped() {
        // A brace in displayed text is escaped on the way out, so two emitted
        // bytes come from one source byte. Answering with the whole literal
        // would make every diagnostic about a brace point at the whole string.
        let composed = compose("a{b", &[segment((0, 3), (11, 14))], (10, 15));
        assert_eq!(
            composed,
            [
                (0, 2, 11, 11),
                (2, 3, 11, 12),
                (3, 5, 12, 13),
                (5, 6, 13, 14),
                (6, 8, 14, 14),
            ]
        );
    }

    #[test]
    fn multibyte_text_composes_on_scalar_boundaries() {
        // Two three-byte scalars around an escaped brace.
        let text = "日{本";
        assert_eq!(text.len(), 7);
        let composed = compose(text, &[segment((0, 7), (11, 18))], (10, 19));
        assert_eq!(
            composed,
            [
                (0, 2, 11, 11),
                (2, 5, 11, 14),
                (5, 7, 14, 15),
                (7, 10, 15, 18),
                (10, 12, 18, 18),
            ]
        );
        for piece in &composed {
            assert!(text.is_char_boundary((piece.2 - 11) as usize));
            assert!(text.is_char_boundary((piece.3 - 11) as usize));
        }
    }

    #[test]
    fn a_crlf_the_host_normalized_answers_for_both_of_its_bytes() {
        // A template literal carrying CR LF hands over one line feed. The pair
        // is what produced it, and saying so is not the same as claiming the
        // line feed is the CR.
        let map = [
            segment((0, 1), (11, 12)),
            segment((1, 2), (12, 14)),
            segment((2, 3), (14, 15)),
        ];
        let composed = compose("a\nb", &map, (10, 16));
        assert_eq!(composed[2], (3, 4, 12, 14));
    }

    #[test]
    fn text_the_host_dropped_is_not_part_of_the_content_it_surrounds() {
        // A line continuation produces no decoded byte. The closing delimiter
        // marks where content ended, which is before the continuation.
        let map = [segment((0, 3), (11, 14)), segment((3, 3), (14, 16))];
        let composed = compose("abc", &map, (10, 17));
        assert_eq!(
            composed,
            [(0, 2, 11, 11), (2, 5, 11, 14), (5, 7, 14, 14)],
            "the dropped bytes are neither content nor the end of it"
        );
    }

    #[test]
    fn empty_text_still_has_a_position_in_source() {
        // Both generated delimiters mark the same insertion point, which is
        // the only honest answer for a string with no bytes.
        let composed = compose("", &[segment((0, 0), (11, 11))], (10, 12));
        assert_eq!(composed, [(0, 2, 11, 11), (2, 4, 11, 11)]);
    }

    #[test]
    fn a_declaration_ending_at_the_unit_boundary_composes() {
        // The occurrence runs to the last byte of the unit, so the closing
        // delimiter resolves to the end of the source itself.
        let composed = compose("ok", &[segment((0, 2), (62, 64))], (61, 64));
        assert_eq!(composed, [(0, 2, 62, 62), (2, 4, 62, 64), (4, 6, 64, 64)]);
    }

    #[test]
    fn authored_mf2_composes_through_its_identity_segment() {
        let source = "Hello {$name}!";
        let mut extraction = vec![literal::identity_segment(source.len() as u64).unwrap()];
        let map = [
            segment((0, 6), (11, 17)),
            segment((6, 7), (17, 19)),
            segment((7, 14), (19, 26)),
        ];
        let composed = compose_extraction_map(
            &extraction,
            &map,
            source.len() as u64,
            &occurrence(10, 27),
            &generous(),
        )
        .unwrap();
        extraction.clear();
        assert_eq!(
            composed.len(),
            3,
            "one identity segment splits where the host decoded"
        );
        assert_eq!(composed[0].extracted(), ByteRange::new(0, 6).unwrap());
        assert_eq!(composed[1].source(), ByteRange::new(17, 19).unwrap());
        assert_eq!(composed[2].source(), ByteRange::new(19, 26).unwrap());
    }

    #[test]
    fn a_map_that_does_not_describe_the_text_is_refused_for_the_reason_it_fails() {
        let mut extraction = Vec::new();
        literal::encode("abc", &generous(), &mut extraction).unwrap();
        let check = |map: &[InputSegment]| {
            compose_extraction_map(&extraction, map, 3, &occurrence(10, 16), &generous())
                .unwrap_err()
        };

        assert_eq!(
            check(&[]),
            MappingError::Empty,
            "absence is expressed by omitting the map"
        );
        assert_eq!(
            check(&[segment((0, 1), (11, 12)), segment((2, 3), (13, 14))]),
            MappingError::Discontinuous
        );
        assert_eq!(check(&[segment((0, 2), (11, 13))]), MappingError::Coverage);
        assert_eq!(check(&[segment((0, 4), (11, 15))]), MappingError::Coverage);
        assert_eq!(
            check(&[segment((1, 3), (12, 14))]),
            MappingError::Coverage,
            "a map that does not start at the first byte describes something else"
        );
        assert_eq!(
            check(&[segment((0, 3), (2, 5))]),
            MappingError::SourceOutsideOccurrence,
            "the decoded text comes from inside the declaration's own syntax"
        );
    }

    #[test]
    fn the_composed_map_is_bounded_by_the_invocation() {
        let mut extraction = Vec::new();
        literal::encode("abc", &generous(), &mut extraction).unwrap();
        let map = [
            segment((0, 1), (11, 12)),
            segment((1, 2), (12, 13)),
            segment((2, 3), (13, 14)),
        ];
        // Splitting produces more segments than either input map has, so the
        // bound has to be checked against the composed result rather than
        // against the two maps that went into it.
        assert_eq!(extraction.len(), 3);
        assert_eq!(map.len(), 3);

        let mut limits = generous();
        limits.extraction_segments = 5;
        let exact = compose_extraction_map(&extraction, &map, 3, &occurrence(10, 15), &limits);
        assert_eq!(
            exact.map(|composed| composed.len()),
            Ok(5),
            "the exact bound is admitted"
        );

        limits.extraction_segments = 4;
        assert_eq!(
            compose_extraction_map(&extraction, &map, 3, &occurrence(10, 15), &limits),
            Err(MappingError::Limit(LimitKind::ExtractionSegments)),
            "one segment short of what this map needs is a refusal, not a shorter map"
        );
    }
}
