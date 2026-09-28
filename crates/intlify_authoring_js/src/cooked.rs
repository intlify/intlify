// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Decoding a host literal into the text a message is read from.
//!
//! 016 reads a message from the host's cooked value: JavaScript escapes are
//! decoded before MF2 sees the text, so `intent('a\nb')` and a `mf2` template
//! spelling the same escape both hand MF2 a line feed. The parser computes
//! that value, but not where each decoded byte came from, and a diagnostic
//! inside a message has to point back into host source.
//!
//! This module decodes the literal again from its source bytes, recording an
//! [`InputSegment`] per run as it goes, and requires its text to equal the
//! parser's cooked value byte for byte. The two decoders are independent, so
//! agreement is evidence that neither misread the literal, and a disagreement
//! stops the invocation rather than choosing one of them.
//!
//! What the profile does not accept is decided here, from the source bytes:
//!
//! - a surrogate that does not pair as two adjacent `\uXXXX` escapes;
//! - an escape a tagged template keeps without a cooked value;
//! - a legacy octal escape, or `\8` or `\9`, which only sloppy code admits.
//!
//! JavaScript also pairs surrogates spelled other ways, such as
//! `\uD83D\u{DE00}`, or two escapes on either side of a line continuation. The
//! pinned parser does not, and the profile does not accept a spelling its
//! parser reads differently from the language.
//!
//! Runs split wherever a source byte stops answering for exactly one decoded
//! byte. Verbatim text is one positional run, and so is a lone carriage return
//! a template reads as a line feed, because one byte still answers for one
//! byte. An escape, a CRLF a template reads as one line feed, and a line
//! continuation are each a run of their own: several source bytes produced
//! fewer decoded ones, and no byte inside them answers for a byte of output.

use intlify_authoring::{ByteRange, InputSegment};
use oxc_ast::ast::{StringLiteral, TemplateElement};

use crate::limits::{JsAuthoringLimits, JsLimitKind};

/// One decoded literal and where each run of its text came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cooked {
    /// The decoded text, equal to the parser's cooked value.
    pub(crate) text: String,
    /// Runs from decoded text to host source, covering the text exactly.
    pub(crate) input_map: Vec<InputSegment>,
}

/// A literal form the profile does not accept as message source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unsupported {
    /// A surrogate that does not pair as two adjacent `\uXXXX` escapes.
    SurrogateEscape,
    /// An escape a tagged template keeps without a cooked value.
    TemplateEscapeInvalid,
    /// A legacy octal escape, or `\8` or `\9`.
    LegacyEscape,
}

impl Unsupported {
    /// Return the exact spelling fixtures name this form by.
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::SurrogateEscape => "surrogate-escape",
            Self::TemplateEscapeInvalid => "template-escape-invalid",
            Self::LegacyEscape => "legacy-escape",
        }
    }
}

/// Why a literal could not be decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CookFailure {
    /// The literal uses a form the profile does not accept, at this escape.
    ///
    /// The author can fix this, so it becomes a diagnostic at the escape.
    Unsupported(Unsupported, ByteRange),
    /// Decoding needs more input segments than the invocation allows.
    Limit(JsLimitKind),
    /// This decoder and the parser read the literal differently.
    ///
    /// Neither reading can be trusted over the other, so this is operational.
    Disagreement,
}

/// Which literal syntax is being decoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    String,
    Template,
}

/// Decode one string literal and check the result against the parser's.
pub(crate) fn cook_string(
    source: &str,
    literal: &StringLiteral<'_>,
    limits: &JsAuthoringLimits,
) -> Result<Cooked, CookFailure> {
    let start = literal.span.start as usize;
    let end = literal.span.end as usize;
    let token = source
        .get(start..end)
        .ok_or(CookFailure::Disagreement)?
        .as_bytes();
    // The span has to be the whole quoted token for its interior to be the
    // literal's content.
    let quoted =
        token.len() >= 2 && matches!(token[0], b'\'' | b'"') && token[token.len() - 1] == token[0];
    if !quoted {
        return Err(CookFailure::Disagreement);
    }
    let cooked = decode(source, start + 1, end - 1, Kind::String, limits)?;
    if literal.lone_surrogates || literal.value.as_str() != cooked.text {
        return Err(CookFailure::Disagreement);
    }
    Ok(cooked)
}

/// Decode one template element and check the result against the parser's.
///
/// An element's span is its content alone, without the backtick or the
/// substitution delimiters around it.
pub(crate) fn cook_template(
    source: &str,
    element: &TemplateElement<'_>,
    limits: &JsAuthoringLimits,
) -> Result<Cooked, CookFailure> {
    let start = element.span.start as usize;
    let end = element.span.end as usize;
    source.get(start..end).ok_or(CookFailure::Disagreement)?;
    let parsed = element.value.cooked.as_deref();
    match decode(source, start, end, Kind::Template, limits) {
        Ok(cooked) => {
            if element.lone_surrogates || parsed != Some(cooked.text.as_str()) {
                return Err(CookFailure::Disagreement);
            }
            Ok(cooked)
        }
        // Only a tagged template keeps an escape like this, and the parser
        // then gives it no cooked value. An untagged one never parses.
        Err(failure @ CookFailure::Unsupported(Unsupported::TemplateEscapeInvalid, _)) => {
            if parsed.is_some() {
                Err(CookFailure::Disagreement)
            } else {
                Err(failure)
            }
        }
        Err(failure) => Err(failure),
    }
}

/// Decode the content between `start` and `end`.
fn decode(
    source: &str,
    start: usize,
    end: usize,
    kind: Kind,
    limits: &JsAuthoringLimits,
) -> Result<Cooked, CookFailure> {
    let bytes = source.as_bytes();
    let mut output = Output {
        text: String::with_capacity(end - start),
        input_map: Vec::new(),
        limit: limits.input_segments,
        run: None,
    };
    let mut at = start;
    while at < end {
        // Everything before the next byte that can change meaning is copied as
        // it is. Both such bytes are ASCII, so the cut is a scalar boundary.
        let special = |byte: &u8| *byte == b'\\' || (kind == Kind::Template && *byte == b'\r');
        let stop = bytes[at..end]
            .iter()
            .position(special)
            .map_or(end, |offset| at + offset);
        if stop > at {
            output.verbatim(&source[at..stop], at);
            at = stop;
            continue;
        }
        at = match bytes[at] {
            b'\\' => escape(source, at, end, kind, &mut output)?,
            // Only a template stops at a carriage return, and it reads both a
            // CRLF and a lone CR as one line feed.
            _ if at + 1 < end && bytes[at + 1] == b'\n' => replace(&mut output, "\n", at, at + 2)?,
            _ => {
                // A lone carriage return still answers for exactly one byte,
                // so the positional run continues through it.
                output.verbatim("\n", at);
                at + 1
            }
        };
    }
    output.finish(start)
}

/// Decode the escape whose backslash is at `at`, returning where it ends.
fn escape(
    source: &str,
    at: usize,
    end: usize,
    kind: Kind,
    output: &mut Output,
) -> Result<usize, CookFailure> {
    let bytes = source.as_bytes();
    let next = at + 1;
    // A backslash cannot end a literal's content: it would have escaped the
    // closing delimiter instead.
    if next >= end {
        return Err(CookFailure::Disagreement);
    }
    match bytes[next] {
        b'\n' => replace(output, "", at, next + 1),
        b'\r' => {
            let stop = if next + 1 < end && bytes[next + 1] == b'\n' {
                next + 2
            } else {
                next + 1
            };
            replace(output, "", at, stop)
        }
        b'b' => replace(output, "\u{8}", at, next + 1),
        b'f' => replace(output, "\u{c}", at, next + 1),
        b'n' => replace(output, "\n", at, next + 1),
        b'r' => replace(output, "\r", at, next + 1),
        b't' => replace(output, "\t", at, next + 1),
        b'v' => replace(output, "\u{b}", at, next + 1),
        b'0' if !(next + 1 < end && bytes[next + 1].is_ascii_digit()) => {
            replace(output, "\0", at, next + 1)
        }
        digit @ b'0'..=b'9' => Err(match kind {
            Kind::String => CookFailure::Unsupported(
                Unsupported::LegacyEscape,
                range(at, legacy_end(bytes, next, end))?,
            ),
            // A template has no legacy escapes; a digit escape there is one
            // it keeps without a cooked value.
            Kind::Template => CookFailure::Unsupported(
                Unsupported::TemplateEscapeInvalid,
                range(at, if digit == b'0' { next + 2 } else { next + 1 })?,
            ),
        }),
        b'x' => match (hex(bytes, next + 1, end), hex(bytes, next + 2, end)) {
            (Some(high), Some(low)) => {
                let character = char::from_u32(high * 16 + low).ok_or(CookFailure::Disagreement)?;
                replace(output, character.encode_utf8(&mut [0; 4]), at, next + 3)
            }
            (Some(_), None) => invalid(kind, at, next + 2),
            (None, _) => invalid(kind, at, next + 1),
        },
        b'u' => unicode(bytes, at, end, kind, output),
        _ => {
            let character = source[next..]
                .chars()
                .next()
                .ok_or(CookFailure::Disagreement)?;
            let stop = next + character.len_utf8();
            if stop > end {
                return Err(CookFailure::Disagreement);
            }
            if matches!(character, '\u{2028}' | '\u{2029}') {
                // A line continuation: the terminator is part of the escape.
                replace(output, "", at, stop)
            } else {
                replace(output, character.encode_utf8(&mut [0; 4]), at, stop)
            }
        }
    }
}

/// Decode a `\u` escape whose backslash is at `at`.
fn unicode(
    bytes: &[u8],
    at: usize,
    end: usize,
    kind: Kind,
    output: &mut Output,
) -> Result<usize, CookFailure> {
    let marker = at + 1;
    if marker + 1 < end && bytes[marker + 1] == b'{' {
        let first = marker + 2;
        let mut stop = first;
        let mut value = 0_u32;
        while let Some(digit) = hex(bytes, stop, end) {
            value = value.saturating_mul(16).saturating_add(digit);
            stop += 1;
        }
        if stop == first {
            return invalid(kind, at, first);
        }
        if value > 0x0010_FFFF || stop >= end || bytes[stop] != b'}' {
            return invalid(kind, at, stop);
        }
        let close = stop + 1;
        if (0xD800..=0xDFFF).contains(&value) {
            return Err(CookFailure::Unsupported(
                Unsupported::SurrogateEscape,
                range(at, close)?,
            ));
        }
        let character = char::from_u32(value).ok_or(CookFailure::Disagreement)?;
        return replace(output, character.encode_utf8(&mut [0; 4]), at, close);
    }

    let Some(unit) = four_hex(bytes, marker + 1, end) else {
        let digits = (marker + 1..end)
            .take(4)
            .take_while(|&index| hex(bytes, index, end).is_some())
            .count();
        return invalid(kind, at, marker + 1 + digits);
    };
    let stop = marker + 5;
    match unit {
        0xD800..=0xDBFF => {
            // A high surrogate pairs only with a low one spelled as the very
            // next escape, in the same four-digit form.
            let spelled_next = stop + 1 < end && bytes[stop] == b'\\' && bytes[stop + 1] == b'u';
            let low = spelled_next
                .then(|| four_hex(bytes, stop + 2, end))
                .flatten()
                .filter(|low| (0xDC00..=0xDFFF).contains(low));
            let Some(low) = low else {
                return Err(CookFailure::Unsupported(
                    Unsupported::SurrogateEscape,
                    range(at, stop)?,
                ));
            };
            let scalar = 0x1_0000 + ((unit - 0xD800) << 10) + (low - 0xDC00);
            let character = char::from_u32(scalar).ok_or(CookFailure::Disagreement)?;
            replace(output, character.encode_utf8(&mut [0; 4]), at, stop + 6)
        }
        0xDC00..=0xDFFF => Err(CookFailure::Unsupported(
            Unsupported::SurrogateEscape,
            range(at, stop)?,
        )),
        _ => {
            let character = char::from_u32(unit).ok_or(CookFailure::Disagreement)?;
            replace(output, character.encode_utf8(&mut [0; 4]), at, stop)
        }
    }
}

/// Report an escape the language does not define.
///
/// A string literal holding one never parses, so meeting one here means this
/// decoder and the parser disagree. A tagged template keeps it, and the
/// parser gives the template no cooked value.
fn invalid(kind: Kind, at: usize, stop: usize) -> Result<usize, CookFailure> {
    match kind {
        Kind::String => Err(CookFailure::Disagreement),
        Kind::Template => Err(CookFailure::Unsupported(
            Unsupported::TemplateEscapeInvalid,
            range(at, stop)?,
        )),
    }
}

/// Return where a legacy octal escape ends, following Annex B's grammar.
///
/// A digit from 0 to 3 takes up to two more octal digits, one from 4 to 7
/// takes one more, and `\8`, `\9` and a `\0` before 8 or 9 are one digit.
fn legacy_end(bytes: &[u8], first: usize, end: usize) -> usize {
    let octal = |index: usize| index < end && matches!(bytes[index], b'0'..=b'7');
    let longest = match bytes[first] {
        b'0'..=b'3' => 3,
        b'4'..=b'7' => 2,
        _ => 1,
    };
    let mut stop = first + 1;
    while stop - first < longest && octal(stop) {
        stop += 1;
    }
    stop
}

fn four_hex(bytes: &[u8], first: usize, end: usize) -> Option<u32> {
    (first..first + 4).try_fold(0_u32, |value, index| {
        Some(value * 16 + hex(bytes, index, end)?)
    })
}

fn hex(bytes: &[u8], index: usize, end: usize) -> Option<u32> {
    if index >= end {
        return None;
    }
    char::from(bytes[index]).to_digit(16)
}

fn range(start: usize, end: usize) -> Result<ByteRange, CookFailure> {
    ByteRange::new(start as u64, end as u64).map_err(|_| CookFailure::Disagreement)
}

fn replace(
    output: &mut Output,
    piece: &str,
    start: usize,
    end: usize,
) -> Result<usize, CookFailure> {
    output.replaced(piece, start, end)?;
    Ok(end)
}

/// Decoded text under construction, with its map.
struct Output {
    text: String,
    input_map: Vec<InputSegment>,
    limit: u64,
    /// Where the open positional run began, in decoded text and in source.
    run: Option<(usize, usize)>,
}

impl Output {
    /// Extend the open positional run, opening one if none is open.
    fn verbatim(&mut self, piece: &str, source_at: usize) {
        let (decoded, origin) = *self.run.get_or_insert((self.text.len(), source_at));
        debug_assert_eq!(
            source_at - origin,
            self.text.len() - decoded,
            "a positional run is contiguous on both sides"
        );
        self.text.push_str(piece);
    }

    /// Close the open run and record `piece` as a run of its own.
    fn replaced(&mut self, piece: &str, start: usize, end: usize) -> Result<(), CookFailure> {
        self.close()?;
        // Equal lengths would be read as a positional claim, which a
        // replacement never makes. No escape can produce one: the backslash
        // alone makes the source side longer.
        debug_assert_ne!(
            piece.len(),
            end - start,
            "a replacement never looks positional"
        );
        let decoded = self.text.len();
        self.text.push_str(piece);
        self.segment(decoded, self.text.len(), start, end)
    }

    fn close(&mut self) -> Result<(), CookFailure> {
        if let Some((decoded, origin)) = self.run.take() {
            let length = self.text.len() - decoded;
            self.segment(decoded, self.text.len(), origin, origin + length)?;
        }
        Ok(())
    }

    fn segment(
        &mut self,
        decoded_start: usize,
        decoded_end: usize,
        source_start: usize,
        source_end: usize,
    ) -> Result<(), CookFailure> {
        if self.input_map.len() as u64 >= self.limit {
            return Err(CookFailure::Limit(JsLimitKind::InputSegments));
        }
        self.input_map.push(InputSegment::new(
            range(decoded_start, decoded_end)?,
            range(source_start, source_end)?,
        ));
        Ok(())
    }

    fn finish(mut self, start: usize) -> Result<Cooked, CookFailure> {
        self.close()?;
        // An empty literal still has to say where its text came from, and a
        // present map has to describe the text even when the text is empty.
        if self.input_map.is_empty() {
            self.segment(0, 0, start, start)?;
        }
        Ok(Cooked {
            text: self.text,
            input_map: self.input_map,
        })
    }
}

#[cfg(test)]
mod tests;
