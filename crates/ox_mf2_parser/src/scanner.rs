//! Scanner cursor and lexical helpers.
//!
//! The scanner is parser-internal. It reads source bytes through [`Cursor`]
//! and exposes small inline helpers (`bump_byte`, `try_eat`, `peek_char`)
//! plus the MF2 character classifiers from the spec
//! (`refers/message-format-wg/spec/message.abnf`). It deliberately does not
//! emit tokens itself: the parser consumes spans returned by the scan
//! routines and commits them to [`crate::CstTables`].
//!
//! Design choices:
//!
//! - Cursor operates on `&[u8]`. ASCII delimiters drive an inline fast path;
//!   the Unicode slow path only kicks in for bytes `>= 0x80`.
//! - State is `Copy` so [`Cursor::checkpoint`] / [`Cursor::restore`] cost
//!   nothing beyond an integer move; recovery points snapshot it directly.
//! - Predicates (`is_ws`, `is_bidi`, `is_text_char`, `is_name_start`, etc.)
//!   match the WG spec ABNF byte-for-byte; tests pin the exclusion edges.

#![allow(clippy::while_let_loop)] // explicit loop {} + Some(b) else-break is
                                   // clearer for the byte/codepoint dual paths.

use crate::span::Span;

/// Snapshotable scanner state. Embedded inside `parser::Checkpoint`.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ScannerState {
    pub offset: u32,
}

impl ScannerState {
    #[inline]
    pub const fn new(offset: u32) -> Self {
        Self { offset }
    }
}

/// Cursor over UTF-8 source bytes.
///
/// `Cursor` borrows the source for the duration of one parse and tracks a
/// single byte offset. Methods come in two layers: byte-level helpers
/// (`peek_byte`, `bump_byte`, `try_eat`) and `char`-level helpers
/// (`peek_char`, `bump_char`) that decode the next Unicode scalar value.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Cursor<'src> {
    bytes: &'src [u8],
    offset: u32,
}

impl<'src> Cursor<'src> {
    #[inline]
    pub fn new(source: &'src str) -> Self {
        Self {
            bytes: source.as_bytes(),
            offset: 0,
        }
    }

    #[inline]
    pub fn offset(&self) -> u32 {
        self.offset
    }

    #[allow(dead_code)]
    #[inline]
    pub fn len(&self) -> u32 {
        self.bytes.len() as u32
    }

    #[inline]
    pub fn is_eof(&self) -> bool {
        self.offset as usize >= self.bytes.len()
    }

    #[inline]
    pub fn state(&self) -> ScannerState {
        ScannerState::new(self.offset)
    }

    #[inline]
    pub fn checkpoint(&self) -> ScannerState {
        self.state()
    }

    #[inline]
    pub fn restore(&mut self, state: ScannerState) {
        self.offset = state.offset;
    }

    #[inline]
    pub fn set_offset(&mut self, offset: u32) {
        debug_assert!(offset as usize <= self.bytes.len());
        self.offset = offset;
    }

    #[inline]
    pub fn peek_byte(&self) -> Option<u8> {
        self.bytes.get(self.offset as usize).copied()
    }

    #[inline]
    pub fn peek_byte_at(&self, ahead: u32) -> Option<u8> {
        self.bytes.get((self.offset + ahead) as usize).copied()
    }

    /// Consume one byte. Returns `None` at EOF. Use only when the next byte
    /// is known ASCII (`< 0x80`); for general Unicode use [`Self::bump_char`].
    #[inline]
    pub fn bump_byte(&mut self) -> Option<u8> {
        let b = self.peek_byte()?;
        self.offset += 1;
        Some(b)
    }

    /// Try to consume an exact ASCII byte sequence. Returns `true` and
    /// advances on success; leaves the cursor untouched on failure.
    #[allow(dead_code)]
    pub fn try_eat(&mut self, prefix: &[u8]) -> bool {
        let end = self.offset as usize + prefix.len();
        if end > self.bytes.len() {
            return false;
        }
        if &self.bytes[self.offset as usize..end] == prefix {
            self.offset = end as u32;
            true
        } else {
            false
        }
    }

    /// Decode the next Unicode scalar value without advancing.
    pub fn peek_char(&self) -> Option<(char, u32)> {
        decode_utf8(self.bytes, self.offset as usize)
    }

    /// Decode and consume the next Unicode scalar value.
    #[allow(dead_code)]
    pub fn bump_char(&mut self) -> Option<(char, u32)> {
        let (c, len) = self.peek_char()?;
        self.offset += len;
        Some((c, len))
    }

    /// Source slice from `from` to the current offset.
    #[inline]
    pub fn span_from(&self, from: u32) -> Span {
        Span::new(from, self.offset)
    }
}

// ───────────────────────── Character classifiers ─────────────────────────

/// `ws = SP / HTAB / CR / LF / %x3000`
#[inline]
pub(crate) fn is_ws_byte(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\r' | b'\n')
}

#[inline]
pub(crate) fn is_ws_char(c: char) -> bool {
    is_ws_byte(c as u32 as u8) && (c as u32) < 0x80 || c as u32 == 0x3000
}

/// `bidi = %x061C / %x200E / %x200F / %x2066-2069`
#[inline]
pub(crate) fn is_bidi_char(c: char) -> bool {
    matches!(c as u32, 0x061C | 0x200E | 0x200F | 0x2066..=0x2069)
}

/// `text-char = %x01-5B / %x5D-7A / %x7C / %x7E-10FFFF` — excludes NULL,
/// `\` (0x5C), `{` (0x7B), `}` (0x7D).
#[inline]
pub(crate) fn is_text_char(c: char) -> bool {
    let cp = c as u32;
    cp != 0 && cp != 0x5C && cp != 0x7B && cp != 0x7D
}

/// `quoted-char = %x01-5B / %x5D-7B / %x7D-10FFFF` — excludes NULL, `\`, `|`.
#[inline]
pub(crate) fn is_quoted_char(c: char) -> bool {
    let cp = c as u32;
    cp != 0 && cp != 0x5C && cp != 0x7C
}

/// `simple-start-char` — excludes NULL, HTAB, LF, CR, SP, `.`, `\`, `{`,
/// `}`, IDEOGRAPHIC SPACE. Consumed when stricter mode-detection lands.
#[allow(dead_code)]
#[inline]
pub(crate) fn is_simple_start_char(c: char) -> bool {
    let cp = c as u32;
    !matches!(
        cp,
        0x00 | 0x09 | 0x0A | 0x0D | 0x20 | 0x2E | 0x5C | 0x7B | 0x7D | 0x3000
    )
}

/// `escaped-char` second byte — `\\`, `{`, `|`, `}`. Used by the M7
/// `InvalidEscape` recovery diagnostic.
#[allow(dead_code)]
#[inline]
pub(crate) fn is_escape_target(b: u8) -> bool {
    matches!(b, b'\\' | b'{' | b'|' | b'}')
}

/// ASCII-only fast check for `name-start`. Returns `Some(true)` / `Some(false)`
/// for ASCII bytes, `None` for bytes that need the Unicode slow path.
#[inline]
pub fn ascii_is_name_start(b: u8) -> Option<bool> {
    if b >= 0x80 {
        return None;
    }
    Some(matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'+' | b'_'))
}

#[inline]
pub fn ascii_is_name_char(b: u8) -> Option<bool> {
    if b >= 0x80 {
        return None;
    }
    Some(
        matches!(b, b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'_' | b'-' | b'.'),
    )
}

/// `name-start` — full spec including Unicode.
pub(crate) fn is_name_start(c: char) -> bool {
    let cp = c as u32;
    if cp < 0xA1 {
        return matches!(cp, 0x2B | 0x5F | 0x41..=0x5A | 0x61..=0x7A);
    }
    is_unicode_name_codepoint(cp)
}

/// `name-char = name-start / DIGIT / "-" / "."`
pub(crate) fn is_name_char(c: char) -> bool {
    let cp = c as u32;
    if cp < 0xA1 {
        return matches!(
            cp,
            0x2B | 0x5F | 0x2D | 0x2E | 0x41..=0x5A | 0x61..=0x7A | 0x30..=0x39
        );
    }
    is_unicode_name_codepoint(cp)
}

/// Shared body of [`is_name_start`] / [`is_name_char`] for code points
/// `>= 0xA1`. Encodes the ranges from `message.abnf` directly.
fn is_unicode_name_codepoint(cp: u32) -> bool {
    if cp > 0x0010_FFFD {
        return false;
    }
    // BidiControl: 0x061C, 0x200E-0x200F, 0x202A-0x202E, 0x2066-0x2069
    if matches!(cp, 0x061C | 0x200E..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069) {
        return false;
    }
    // Whitespace: 0x1680, 0x2000-0x200A, 0x2028-0x2029, 0x202F, 0x205F, 0x3000
    if matches!(
        cp,
        0x1680 | 0x2000..=0x200A | 0x2028..=0x2029 | 0x202F | 0x205F | 0x3000
    ) {
        return false;
    }
    // Cs: 0xD800-0xDFFF (already excluded by `char`, but defensive)
    if (0xD800..=0xDFFF).contains(&cp) {
        return false;
    }
    // NChar: 0xFDD0-0xFDEF and *FFFE/*FFFF per plane.
    if (0xFDD0..=0xFDEF).contains(&cp) {
        return false;
    }
    if cp & 0xFFFE == 0xFFFE {
        return false;
    }
    true
}

/// Returns true if `prefix` starts with a `.` followed by an ASCII keyword.
#[inline]
pub(crate) fn detect_keyword(cursor: &Cursor<'_>) -> Option<KeywordHit> {
    if cursor.peek_byte() != Some(b'.') {
        return None;
    }
    if cursor.try_eat_at_offset(b".input") {
        return Some(KeywordHit::Input);
    }
    if cursor.try_eat_at_offset(b".local") {
        return Some(KeywordHit::Local);
    }
    if cursor.try_eat_at_offset(b".match") {
        return Some(KeywordHit::Match);
    }
    None
}

impl Cursor<'_> {
    /// Read-only `try_eat` that does not advance the cursor.
    #[inline]
    pub(crate) fn try_eat_at_offset(&self, prefix: &[u8]) -> bool {
        let end = self.offset as usize + prefix.len();
        end <= self.bytes.len() && &self.bytes[self.offset as usize..end] == prefix
    }
}

/// Result of a keyword lookahead. Carries which keyword matched; callers
/// advance the cursor explicitly to keep `Cursor::detect_keyword` purely
/// non-destructive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordHit {
    Input,
    Local,
    Match,
}

impl KeywordHit {
    pub const fn length(self) -> u32 {
        // All three keywords are `.<five-letter-name>` → 6 bytes.
        match self {
            Self::Input | Self::Local | Self::Match => 6,
        }
    }
}

// ─────────────────────────── Scan routines ───────────────────────────────

/// Trivia scan flavour. The parser layer scans `ws` / `bidi` directly through
/// [`Parser::eat_trivia`] and uses this enum only to document intent — the
/// classifier is the same in both cases. `Required` callers must also verify
/// that at least one `ws` was consumed to satisfy the spec `s = *bidi ws o`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriviaMode {
    /// Spec `o = *(ws / bidi)`.
    Optional,
    /// Spec `s = *bidi ws o`.
    Required,
}

/// Scan a `text-char` / `escaped-char` run inside a `pattern`. Stops at the
/// next `{`, `}`, `\`, or NULL — those are handled by the parser.
pub(crate) fn scan_text_run(cursor: &mut Cursor<'_>) -> Span {
    let start = cursor.offset();
    loop {
        let Some(b) = cursor.peek_byte() else { break };
        if b < 0x80 {
            // ASCII fast path: cannot be `{`, `}`, `\`, or NULL.
            if b == 0x00 || b == b'\\' || b == b'{' || b == b'}' {
                break;
            }
            cursor.offset += 1;
        } else {
            let Some((c, len)) = cursor.peek_char() else { break };
            if !is_text_char(c) {
                break;
            }
            cursor.offset += len;
        }
    }
    cursor.span_from(start)
}

/// Scan a `quoted-char` / `escaped-char` run inside `|...|`. Stops at the
/// next `|`, `\`, or NULL.
pub(crate) fn scan_quoted_text_run(cursor: &mut Cursor<'_>) -> Span {
    let start = cursor.offset();
    loop {
        let Some(b) = cursor.peek_byte() else { break };
        if b < 0x80 {
            if b == 0x00 || b == b'\\' || b == b'|' {
                break;
            }
            cursor.offset += 1;
        } else {
            let Some((c, len)) = cursor.peek_char() else { break };
            if !is_quoted_char(c) {
                break;
            }
            cursor.offset += len;
        }
    }
    cursor.span_from(start)
}

/// Scan a `name` lexeme. Per spec `name = [bidi] name-start *name-char [bidi]`.
/// Returns `None` if no valid `name-start` is at the cursor.
pub(crate) fn scan_name(cursor: &mut Cursor<'_>) -> Option<Span> {
    let start = cursor.offset();

    // Optional leading bidi marks.
    skip_bidi(cursor);

    // name-start
    let saved = cursor.checkpoint();
    let Some((c, len)) = cursor.peek_char() else {
        cursor.restore(ScannerState::new(start));
        return None;
    };
    if !is_name_start(c) {
        cursor.restore(saved);
        cursor.set_offset(start);
        return None;
    }
    cursor.offset += len;

    // *name-char
    loop {
        let Some(b) = cursor.peek_byte() else { break };
        if b < 0x80 {
            // ASCII fast path
            match ascii_is_name_char(b) {
                Some(true) => {
                    cursor.offset += 1;
                }
                Some(false) | None => break,
            }
        } else {
            let Some((c, len)) = cursor.peek_char() else { break };
            if !is_name_char(c) {
                break;
            }
            cursor.offset += len;
        }
    }

    // Optional trailing bidi marks.
    skip_bidi(cursor);

    Some(Span::new(start, cursor.offset()))
}

/// Scan an `unquoted-literal = 1*name-char` lexeme. Distinct from `scan_name`
/// because the spec allows an unquoted literal to start with any `name-char`
/// (including DIGIT), not just `name-start`.
pub(crate) fn scan_unquoted_literal(cursor: &mut Cursor<'_>) -> Option<Span> {
    let start = cursor.offset();
    skip_bidi(cursor);
    let mut produced = false;
    loop {
        let Some(b) = cursor.peek_byte() else { break };
        if b < 0x80 {
            match ascii_is_name_char(b) {
                Some(true) => {
                    cursor.offset += 1;
                    produced = true;
                }
                _ => break,
            }
        } else {
            let Some((c, len)) = cursor.peek_char() else { break };
            if !is_name_char(c) {
                break;
            }
            cursor.offset += len;
            produced = true;
        }
    }
    skip_bidi(cursor);
    if produced {
        Some(Span::new(start, cursor.offset()))
    } else {
        cursor.set_offset(start);
        None
    }
}

fn skip_bidi(cursor: &mut Cursor<'_>) {
    loop {
        let Some((c, len)) = cursor.peek_char() else { break };
        if !is_bidi_char(c) {
            break;
        }
        cursor.offset += len;
    }
}

// ─────────────────────────── UTF-8 decoding ──────────────────────────────

/// Manual UTF-8 decode. Returns the next scalar value and its byte length.
/// Returns `None` at EOF. The input is assumed to be valid UTF-8 because
/// the cursor was created from `&str`.
#[inline]
fn decode_utf8(bytes: &[u8], offset: usize) -> Option<(char, u32)> {
    let b0 = *bytes.get(offset)?;
    if b0 < 0x80 {
        return Some((b0 as char, 1));
    }
    // Length from the leading byte's UTF-8 prefix.
    let (cp, len) = if b0 < 0xC0 {
        // Continuation byte at the start: malformed; treat as REPLACEMENT.
        (0xFFFDu32, 1u32)
    } else if b0 < 0xE0 {
        let b1 = (*bytes.get(offset + 1)?) as u32;
        ((u32::from(b0) & 0x1F) << 6 | (b1 & 0x3F), 2)
    } else if b0 < 0xF0 {
        let b1 = (*bytes.get(offset + 1)?) as u32;
        let b2 = (*bytes.get(offset + 2)?) as u32;
        (
            (u32::from(b0) & 0x0F) << 12 | (b1 & 0x3F) << 6 | (b2 & 0x3F),
            3,
        )
    } else {
        let b1 = (*bytes.get(offset + 1)?) as u32;
        let b2 = (*bytes.get(offset + 2)?) as u32;
        let b3 = (*bytes.get(offset + 3)?) as u32;
        (
            (u32::from(b0) & 0x07) << 18 | (b1 & 0x3F) << 12 | (b2 & 0x3F) << 6 | (b3 & 0x3F),
            4,
        )
    };
    let c = char::from_u32(cp).unwrap_or('\u{FFFD}');
    Some((c, len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_byte_helpers() {
        let mut c = Cursor::new("abc");
        assert_eq!(c.peek_byte(), Some(b'a'));
        assert_eq!(c.peek_byte_at(2), Some(b'c'));
        assert_eq!(c.bump_byte(), Some(b'a'));
        assert_eq!(c.offset(), 1);
        assert!(c.try_eat(b"bc"));
        assert!(c.is_eof());
    }

    #[test]
    fn cursor_checkpoint_round_trip() {
        let mut c = Cursor::new("xyz");
        c.bump_byte();
        let saved = c.checkpoint();
        c.bump_byte();
        c.bump_byte();
        assert!(c.is_eof());
        c.restore(saved);
        assert_eq!(c.offset(), 1);
        assert_eq!(c.peek_byte(), Some(b'y'));
    }

    #[test]
    fn peek_char_decodes_utf8() {
        let mut c = Cursor::new("aあい"); // a=1 byte, あ=3 bytes, い=3 bytes
        assert_eq!(c.peek_char(), Some(('a', 1)));
        c.bump_char();
        assert_eq!(c.peek_char(), Some(('あ', 3)));
        c.bump_char();
        assert_eq!(c.peek_char(), Some(('い', 3)));
        c.bump_char();
        assert!(c.is_eof());
    }

    #[test]
    fn ws_and_bidi_predicates_match_spec() {
        for b in [b' ', b'\t', b'\r', b'\n'] {
            assert!(is_ws_byte(b));
        }
        assert!(!is_ws_byte(b'.'));
        assert!(is_ws_char('\u{3000}'));
        for c in ['\u{061C}', '\u{200E}', '\u{200F}', '\u{2066}', '\u{2069}'] {
            assert!(is_bidi_char(c));
        }
        assert!(!is_bidi_char('a'));
    }

    #[test]
    fn text_quoted_simple_start_predicates_match_spec() {
        assert!(is_text_char('a'));
        assert!(!is_text_char('{'));
        assert!(!is_text_char('}'));
        assert!(!is_text_char('\\'));

        assert!(!is_quoted_char('|'));
        assert!(is_quoted_char('a'));

        assert!(!is_simple_start_char('.'));
        assert!(!is_simple_start_char(' '));
        assert!(!is_simple_start_char('\u{3000}'));
        assert!(is_simple_start_char('a'));
    }

    #[test]
    fn name_predicates_cover_ascii_fast_path() {
        for b in [b'A', b'a', b'Z', b'+', b'_'] {
            assert_eq!(ascii_is_name_start(b), Some(true));
        }
        for b in [b'0', b'-', b'.'] {
            assert_eq!(ascii_is_name_start(b), Some(false));
            assert_eq!(ascii_is_name_char(b), Some(true));
        }
        assert_eq!(ascii_is_name_start(b'$'), Some(false));
        assert_eq!(ascii_is_name_start(0xFF), None);

        assert!(is_name_start('a'));
        assert!(is_name_start('あ'));
        assert!(!is_name_start('\u{3000}'));
        assert!(!is_name_start('\u{061C}')); // ALM is bidi
    }

    #[test]
    fn detect_keyword_returns_match() {
        for (input, expected) in [
            (".input ", KeywordHit::Input),
            (".local foo", KeywordHit::Local),
            (".match $x", KeywordHit::Match),
        ] {
            let c = Cursor::new(input);
            assert_eq!(detect_keyword(&c), Some(expected));
        }
        assert!(detect_keyword(&Cursor::new(".other")).is_none());
        assert!(detect_keyword(&Cursor::new("input")).is_none());
    }

    #[test]
    fn scan_text_run_stops_at_delimiter() {
        let mut c = Cursor::new("Hello{world");
        let span = scan_text_run(&mut c);
        assert_eq!(span, Span::new(0, 5));
        assert_eq!(c.peek_byte(), Some(b'{'));
    }

    #[test]
    fn scan_quoted_text_run_stops_at_pipe() {
        let mut c = Cursor::new("abc|tail");
        let span = scan_quoted_text_run(&mut c);
        assert_eq!(span, Span::new(0, 3));
        assert_eq!(c.peek_byte(), Some(b'|'));
    }

    #[test]
    fn scan_name_handles_unicode_and_bidi_wrappers() {
        let mut c = Cursor::new("alpha rest");
        let span = scan_name(&mut c).unwrap();
        assert_eq!(span, Span::new(0, 5));
        assert_eq!(c.peek_byte(), Some(b' '));

        let mut c = Cursor::new("\u{061C}foo\u{200E} tail");
        let span = scan_name(&mut c).unwrap();
        // ALM(U+061C → 2 B) + "foo"(3 B) + LRM(U+200E → 3 B) = 8 bytes
        assert_eq!(span, Span::new(0, 8));
    }

    #[test]
    fn scan_name_rejects_non_name_start() {
        let mut c = Cursor::new("0not-a-name");
        assert!(scan_name(&mut c).is_none());
        assert_eq!(c.offset(), 0);
    }
}
