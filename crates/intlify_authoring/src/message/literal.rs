// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Encoding displayed text as a literal MF2 message, with its source mapping.
//!
//! The encoder emits a quoted pattern, `{{` … `}}`, rather than a simple
//! message. The MF2 grammar starts a simple message with `simple-start-char`,
//! which excludes whitespace and `.`, so displayed text that begins with either
//! would need a special case in a simple-message encoder. A leading `.` starts a
//! declaration and the parser rejects it today; a leading space is accepted only
//! because stricter mode detection has not landed yet, and the encoder must not
//! depend on that leniency. The quoted pattern encodes every displayed string
//! uniformly and round-trips byte for byte.
//!
//! An empty simple message is valid, since the grammar makes the whole
//! production optional. It gains nothing here: the quoted form already covers
//! it, and one encoding for every string is what keeps this reversible.
//!
//! Only `\`, `{`, and `}` are escaped. `|` is an ordinary pattern text
//! character and is deliberately left alone, so escaping it would change the
//! literal content. U+0000 is the one Unicode scalar that MF2 pattern text
//! cannot carry in any form; it is reported rather than dropped or replaced.

use crate::limits::{AuthoringLimits, LimitKind};
use crate::primitives::ByteRange;

/// The opening and closing delimiters of a quoted pattern.
const OPEN: &str = "{{";
const CLOSE: &str = "}}";

/// One contiguous correspondence between emitted MF2 bytes and source bytes.
///
/// Segments are ordered by their extracted start, do not overlap, and together
/// cover every emitted byte. A source range may repeat, may be zero-width for a
/// generated delimiter, and may cover fewer bytes than the emitted run it maps
/// to. Equal lengths never by themselves assert a byte-for-byte correspondence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExtractionSegment {
    extracted: ByteRange,
    source: ByteRange,
}

impl ExtractionSegment {
    /// Return the covered range of emitted MF2 bytes.
    #[must_use]
    pub const fn extracted(self) -> ByteRange {
        self.extracted
    }

    /// Return the source bytes this run came from.
    #[must_use]
    pub const fn source(self) -> ByteRange {
        self.source
    }
}

/// Complete failure of a literal encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LiteralFailure {
    /// The text contains a scalar that MF2 pattern text cannot represent.
    UnrepresentableScalar { offset: u64 },
    /// A named bound was exhausted.
    Limit(LimitKind),
}

fn push_segment(
    segments: &mut Vec<ExtractionSegment>,
    limits: &AuthoringLimits,
    extracted: ByteRange,
    source: ByteRange,
) -> Result<(), LiteralFailure> {
    if segments.len() as u64 >= limits.extraction_segments {
        return Err(LiteralFailure::Limit(LimitKind::ExtractionSegments));
    }
    segments.push(ExtractionSegment { extracted, source });
    Ok(())
}

/// Encode displayed text as a quoted MF2 pattern and record its mapping.
///
/// `segments` is cleared first and receives the complete ordered mapping. The
/// returned string owns its bytes and borrows nothing from the workspace.
pub(crate) fn encode(
    text: &str,
    limits: &AuthoringLimits,
    segments: &mut Vec<ExtractionSegment>,
) -> Result<String, LiteralFailure> {
    segments.clear();
    if text.len() as u64 > limits.message_text_bytes {
        return Err(LiteralFailure::Limit(LimitKind::MessageTextBytes));
    }

    // Exact reserve for the common case with no escapes; escaping grows it.
    let mut output = String::new();
    output.reserve(text.len() + OPEN.len() + CLOSE.len());

    output.push_str(OPEN);
    push_segment(segments, limits, range(0, OPEN.len() as u64)?, range(0, 0)?)?;

    let mut run_start = 0_usize;
    for (offset, character) in text.char_indices() {
        let escape = match character {
            '\0' => {
                return Err(LiteralFailure::UnrepresentableScalar {
                    offset: offset as u64,
                })
            }
            '\\' | '{' | '}' => character,
            _ => continue,
        };
        flush_run(text, run_start, offset, &mut output, segments, limits)?;
        let emitted_start = output.len() as u64;
        output.push('\\');
        output.push(escape);
        push_segment(
            segments,
            limits,
            range(emitted_start, output.len() as u64)?,
            range(offset as u64, (offset + escape.len_utf8()) as u64)?,
        )?;
        run_start = offset + escape.len_utf8();
    }
    flush_run(text, run_start, text.len(), &mut output, segments, limits)?;

    let close_start = output.len() as u64;
    output.push_str(CLOSE);
    push_segment(
        segments,
        limits,
        range(close_start, output.len() as u64)?,
        range(text.len() as u64, text.len() as u64)?,
    )?;

    if output.len() as u64 > limits.emitted_mf2_bytes {
        return Err(LiteralFailure::Limit(LimitKind::EmittedMf2Bytes));
    }
    Ok(output)
}

fn flush_run(
    text: &str,
    start: usize,
    end: usize,
    output: &mut String,
    segments: &mut Vec<ExtractionSegment>,
    limits: &AuthoringLimits,
) -> Result<(), LiteralFailure> {
    if start == end {
        return Ok(());
    }
    let emitted_start = output.len() as u64;
    output.push_str(&text[start..end]);
    push_segment(
        segments,
        limits,
        range(emitted_start, output.len() as u64)?,
        range(start as u64, end as u64)?,
    )
}

/// Return the identity mapping for source that is already MF2.
///
/// Authored MF2 is its own extracted form, so one segment maps the whole
/// message onto itself. This is a real correspondence, not a placeholder.
pub(crate) fn identity_segment(byte_length: u64) -> Result<ExtractionSegment, LiteralFailure> {
    let whole = range(0, byte_length)?;
    Ok(ExtractionSegment {
        extracted: whole,
        source: whole,
    })
}

fn range(start: u64, end: u64) -> Result<ByteRange, LiteralFailure> {
    // Both endpoints are produced by forward scanning, so a reversed range
    // would be an encoder defect rather than an input failure.
    ByteRange::new(start, end).map_err(|_| LiteralFailure::Limit(LimitKind::ExtractionSegments))
}

/// Recover the displayed text from an encoded quoted pattern.
///
/// This is the encoder's inverse, used to prove the round trip rather than to
/// read arbitrary MF2. It returns `None` for anything this encoder would not
/// have produced.
pub(crate) fn decode_round_trip(encoded: &str) -> Option<String> {
    let body = encoded.strip_prefix(OPEN)?.strip_suffix(CLOSE)?;
    let mut text = String::with_capacity(body.len());
    let mut characters = body.chars();
    while let Some(character) = characters.next() {
        if character == '\\' {
            match characters.next()? {
                escaped @ ('\\' | '{' | '}') => text.push(escaped),
                _ => return None,
            }
        } else if matches!(character, '{' | '}') {
            return None;
        } else {
            text.push(character);
        }
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::tests::generous;

    fn encoded(text: &str) -> (String, Vec<ExtractionSegment>) {
        let mut segments = Vec::new();
        let output = encode(text, &generous(), &mut segments).unwrap();
        (output, segments)
    }

    fn check_segments(text: &str, output: &str, segments: &[ExtractionSegment]) {
        let mut covered = 0_u64;
        for segment in segments {
            assert_eq!(
                segment.extracted().start(),
                covered,
                "segments must be ordered and non-overlapping"
            );
            covered = segment.extracted().end();
            assert!(segment.source().end() <= text.len() as u64);
            assert!(output.is_char_boundary(segment.extracted().start() as usize));
            assert!(text.is_char_boundary(segment.source().start() as usize));
            assert!(text.is_char_boundary(segment.source().end() as usize));
        }
        assert_eq!(
            covered,
            output.len() as u64,
            "segments must cover the output"
        );
    }

    #[test]
    fn displayed_text_round_trips_byte_for_byte() {
        for text in [
            "",
            "Pay now",
            " leading and trailing ",
            ".starts with a period",
            "Total: {amount}",
            "back\\slash",
            "close } and open {",
            "pipe | stays literal",
            "日本語🙂",
            "line one\nline two",
            "crlf\r\nnext",
            "\u{3000}ideographic space",
        ] {
            let (output, segments) = encoded(text);
            assert_eq!(
                decode_round_trip(&output).as_deref(),
                Some(text),
                "round trip failed for {text:?} encoded as {output:?}"
            );
            check_segments(text, &output, &segments);
        }
    }

    #[test]
    fn only_backslash_and_braces_are_escaped() {
        assert_eq!(encoded("Pay now").0, "{{Pay now}}");
        assert_eq!(encoded("").0, "{{}}");
        assert_eq!(encoded("Total: {amount}").0, "{{Total: \\{amount\\}}}");
        assert_eq!(encoded("a\\b").0, "{{a\\\\b}}");
        // A pipe is ordinary pattern text; escaping it would change the content.
        assert_eq!(encoded("a|b").0, "{{a|b}}");
    }

    #[test]
    fn escaping_prevents_the_pattern_from_closing_early() {
        let (output, _) = encoded("}}");
        assert_eq!(output, "{{\\}\\}}}");
        assert!(!output[OPEN.len()..output.len() - CLOSE.len()].contains("}}"));
        assert_eq!(decode_round_trip(&output).as_deref(), Some("}}"));
    }

    #[test]
    fn escape_segments_map_several_emitted_bytes_to_one_source_scalar() {
        let (output, segments) = encoded("a{b");
        assert_eq!(output, "{{a\\{b}}");
        let pairs: Vec<(u64, u64, u64, u64)> = segments
            .iter()
            .map(|segment| {
                (
                    segment.extracted().start(),
                    segment.extracted().end(),
                    segment.source().start(),
                    segment.source().end(),
                )
            })
            .collect();
        assert_eq!(
            pairs,
            [
                (0, 2, 0, 0), // generated opening delimiter, zero-width source
                (2, 3, 0, 1), // "a"
                (3, 5, 1, 2), // "\\{" maps two emitted bytes to one source byte
                (5, 6, 2, 3), // "b"
                (6, 8, 3, 3), // generated closing delimiter, zero-width source
            ]
        );
    }

    #[test]
    fn multibyte_scalars_keep_byte_ranges_on_scalar_boundaries() {
        let text = "日{本";
        let (output, segments) = encoded(text);
        assert_eq!(output, "{{日\\{本}}");
        check_segments(text, &output, &segments);
        let escape = segments
            .iter()
            .find(|segment| segment.extracted().len() == 2 && !segment.source().is_empty())
            .unwrap();
        assert_eq!(escape.source().start(), 3);
        assert_eq!(escape.source().end(), 4);
    }

    #[test]
    fn the_one_unrepresentable_scalar_is_reported_with_its_offset() {
        let mut segments = Vec::new();
        assert_eq!(
            encode("ab\0cd", &generous(), &mut segments),
            Err(LiteralFailure::UnrepresentableScalar { offset: 2 })
        );
        assert_eq!(
            encode("日\0", &generous(), &mut segments),
            Err(LiteralFailure::UnrepresentableScalar { offset: 3 })
        );
    }

    #[test]
    fn exact_and_first_over_bounds_are_distinguished() {
        let mut limits = generous();
        limits.message_text_bytes = 4;
        let mut segments = Vec::new();
        assert!(encode("abcd", &limits, &mut segments).is_ok());
        assert_eq!(
            encode("abcde", &limits, &mut segments),
            Err(LiteralFailure::Limit(LimitKind::MessageTextBytes))
        );

        // "{{ab}}" is exactly six emitted bytes.
        let mut emitted = generous();
        emitted.emitted_mf2_bytes = 6;
        assert!(encode("ab", &emitted, &mut segments).is_ok());
        assert_eq!(
            encode("abc", &emitted, &mut segments),
            Err(LiteralFailure::Limit(LimitKind::EmittedMf2Bytes))
        );

        // "a{b" needs five segments: open, "a", escape, "b", close.
        let mut count = generous();
        count.extraction_segments = 5;
        assert!(encode("a{b", &count, &mut segments).is_ok());
        count.extraction_segments = 4;
        assert_eq!(
            encode("a{b", &count, &mut segments),
            Err(LiteralFailure::Limit(LimitKind::ExtractionSegments))
        );
    }

    #[test]
    fn the_inverse_rejects_inputs_this_encoder_never_produces() {
        for invalid in [
            "Pay now",
            "{{unclosed",
            "unopened}}",
            "{{\\a}}",
            "{{trailing\\}}",
        ] {
            assert_eq!(decode_round_trip(invalid), None, "{invalid}");
        }
        // A bare brace inside the body is not an encoder output either.
        assert_eq!(decode_round_trip("{{a{b}}"), None);
    }
}
