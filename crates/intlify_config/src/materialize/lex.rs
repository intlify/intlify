// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded strict-JSON tokenization over raw bytes. UTF-8 is validated as the
//! bytes are reached, so the first byte that is not UTF-8 or not strict JSON
//! stops tokenization where it is (D15:802). A string token carries its
//! decoded byte length and whether its value stays in the Unicode scalar
//! domain, and a number token whether it is a Portable JSON Number, not an
//! allocated value: a value outside the JSON-compatible domain is a fact the
//! parser records, not a reason to stop. No arbitrary-precision serde number
//! or map path is used.

use super::{byte_count, ByteSpan, InputFailure, PortableNumber, Token, TokenKind};
use crate::input_limits::{Bound, CountRelation, InputBound, LimitViolation, RawInputLimits};

pub(super) struct Lexer<'source> {
    source: &'source [u8],
    offset: usize,
    tokens: u64,
    max_tokens: Bound,
}

impl<'source> Lexer<'source> {
    pub(super) fn new(source: &'source [u8], limits: RawInputLimits) -> Self {
        Self {
            source,
            offset: 0,
            tokens: 0,
            max_tokens: limits.max_parser_tokens,
        }
    }

    pub(super) const fn visited_tokens(&self) -> u64 {
        self.tokens
    }

    pub(super) fn finish(&mut self) -> Result<(), InputFailure> {
        self.whitespace();
        if self.offset != self.source.len() {
            return Err(self.unexpected());
        }
        Ok(())
    }

    pub(super) fn add(&self, left: u64, right: u64) -> Result<u64, InputFailure> {
        left.checked_add(right)
            .ok_or_else(|| InputFailure::AccountingOverflow {
                span: self.current_span(),
            })
    }

    pub(super) fn current_span(&self) -> ByteSpan {
        let end = if self.offset < self.source.len() {
            self.offset + 1
        } else {
            self.offset
        };
        ByteSpan::from_offsets(self.offset, end)
    }

    /// The failure of the byte at the current offset, which strict JSON does
    /// not allow there: its bytes when they are not UTF-8, and otherwise the
    /// byte itself, or the end of the input.
    fn unexpected(&self) -> InputFailure {
        if self
            .source
            .get(self.offset)
            .is_some_and(|byte| !byte.is_ascii())
        {
            if let Err(span) = self.scalar_width() {
                return InputFailure::InvalidUtf8 { span };
            }
        }
        InputFailure::Syntax {
            span: self.current_span(),
        }
    }

    /// The byte length of the UTF-8 scalar at the current offset, or the span
    /// of the bytes that are not one, as `std` delimits them.
    fn scalar_width(&self) -> Result<usize, ByteSpan> {
        let start = self.offset;
        let window = &self.source[start..self.source.len().min(start + 4)];
        let valid = match std::str::from_utf8(window) {
            Ok(text) => text,
            Err(error) if error.valid_up_to() > 0 => {
                std::str::from_utf8(&window[..error.valid_up_to()]).unwrap_or_default()
            }
            Err(error) => {
                let end = error
                    .error_len()
                    .map_or(self.source.len(), |length| start + length);
                return Err(ByteSpan::from_offsets(start, end));
            }
        };
        Ok(valid.chars().next().map_or(1, char::len_utf8))
    }

    fn whitespace(&mut self) {
        while self
            .source
            .get(self.offset)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.offset += 1;
        }
    }

    pub(super) fn token(&mut self) -> Result<Token, InputFailure> {
        self.whitespace();
        let start = self.offset;
        let Some(&byte) = self.source.get(start) else {
            return Err(InputFailure::Syntax {
                span: self.current_span(),
            });
        };
        if self.tokens == self.max_tokens.get() {
            return Err(InputFailure::ParserTokens {
                violation: LimitViolation {
                    bound: InputBound::ParserTokens,
                    limit: self.max_tokens,
                    actual: self.max_tokens.get() + 1,
                    relation: CountRelation::AtLeast,
                },
                span: self.current_span(),
            });
        }
        self.tokens = self.add(self.tokens, 1)?;
        let kind = match byte {
            b'[' => {
                self.offset += 1;
                TokenKind::OpenArray
            }
            b']' => {
                self.offset += 1;
                TokenKind::CloseArray
            }
            b'{' => {
                self.offset += 1;
                TokenKind::OpenObject
            }
            b'}' => {
                self.offset += 1;
                TokenKind::CloseObject
            }
            b':' => {
                self.offset += 1;
                TokenKind::Colon
            }
            b',' => {
                self.offset += 1;
                TokenKind::Comma
            }
            b'n' => {
                self.literal(b"null")?;
                TokenKind::Null
            }
            b't' => {
                self.literal(b"true")?;
                TokenKind::Boolean(true)
            }
            b'f' => {
                self.literal(b"false")?;
                TokenKind::Boolean(false)
            }
            b'"' => self.string()?,
            b'-' | b'0'..=b'9' => TokenKind::Number(self.number()?),
            _ => return Err(self.unexpected()),
        };
        Ok(Token {
            kind,
            span: ByteSpan::from_offsets(start, self.offset),
        })
    }

    fn literal(&mut self, expected: &[u8]) -> Result<(), InputFailure> {
        for &byte in expected {
            if self.source.get(self.offset) != Some(&byte) {
                return Err(self.unexpected());
            }
            self.offset += 1;
        }
        Ok(())
    }

    /// A number token, which is `None` outside the Portable JSON Number
    /// domain.
    fn number(&mut self) -> Result<Option<PortableNumber>, InputFailure> {
        let start = self.offset;
        if self.source[self.offset] == b'-' {
            self.offset += 1;
        }
        match self.source.get(self.offset) {
            Some(b'0') => self.offset += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.unexpected()),
        }
        if self.source.get(self.offset) == Some(&b'.') {
            self.offset += 1;
            self.required_digits()?;
        }
        if self
            .source
            .get(self.offset)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.offset += 1;
            if self
                .source
                .get(self.offset)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.offset += 1;
            }
            self.required_digits()?;
        }
        // The token is ASCII digits and signs, so it is UTF-8.
        Ok(std::str::from_utf8(&self.source[start..self.offset])
            .ok()
            .and_then(PortableNumber::from_token))
    }

    fn digits(&mut self) {
        while self.source.get(self.offset).is_some_and(u8::is_ascii_digit) {
            self.offset += 1;
        }
    }

    fn required_digits(&mut self) -> Result<(), InputFailure> {
        let start = self.offset;
        self.digits();
        if self.offset == start {
            return Err(self.unexpected());
        }
        Ok(())
    }

    /// A string token: its decoded byte length, counting a lone surrogate as
    /// the three bytes of its code point, and whether its value stays in the
    /// Unicode scalar domain.
    fn string(&mut self) -> Result<TokenKind, InputFailure> {
        self.offset += 1;
        let mut decoded = 0_u64;
        let mut scalar = true;
        loop {
            let bytes = match self.source.get(self.offset) {
                Some(b'"') => {
                    self.offset += 1;
                    return Ok(TokenKind::String { decoded, scalar });
                }
                Some(b'\\') => {
                    let (bytes, in_domain) = self.escape()?;
                    scalar &= in_domain;
                    bytes
                }
                Some(0..=0x1f) | None => {
                    return Err(InputFailure::Syntax {
                        span: self.current_span(),
                    })
                }
                Some(0x80..) => {
                    let width = self
                        .scalar_width()
                        .map_err(|span| InputFailure::InvalidUtf8 { span })?;
                    self.offset += width;
                    byte_count(width)
                }
                Some(_) => {
                    self.offset += 1;
                    1
                }
            };
            decoded = self.add(decoded, bytes)?;
        }
    }

    /// One escape: the byte length of what it decodes to, and whether that is
    /// a Unicode scalar. A high surrogate pairs with an immediately following
    /// low surrogate escape; any other surrogate stands alone.
    fn escape(&mut self) -> Result<(u64, bool), InputFailure> {
        self.offset += 1;
        match self.source.get(self.offset) {
            Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                self.offset += 1;
                Ok((1, true))
            }
            Some(b'u') => {
                self.offset += 1;
                let unit = self.hex_unit()?;
                if (0xd800..=0xdbff).contains(&unit) {
                    if self.low_surrogate_follows() {
                        self.offset += 6;
                        return Ok((4, true));
                    }
                    Ok((3, false))
                } else if (0xdc00..=0xdfff).contains(&unit) {
                    Ok((3, false))
                } else {
                    let scalar =
                        char::from_u32(u32::from(unit)).expect("non-surrogate UTF-16 unit");
                    Ok((byte_count(scalar.len_utf8()), true))
                }
            }
            _ => Err(self.unexpected()),
        }
    }

    /// Whether a `\u` escape of a low surrogate is at the current offset.
    fn low_surrogate_follows(&self) -> bool {
        let Some([b'\\', b'u', digits @ ..]) = self.source.get(self.offset..self.offset + 6) else {
            return false;
        };
        digits
            .iter()
            .try_fold(0_u16, |unit, &byte| {
                Some((unit << 4) | u16::from(hex_digit(byte)?))
            })
            .is_some_and(|unit| (0xdc00..=0xdfff).contains(&unit))
    }

    fn hex_unit(&mut self) -> Result<u16, InputFailure> {
        let mut unit = 0_u16;
        for _ in 0..4 {
            let Some(digit) = self.source.get(self.offset).copied().and_then(hex_digit) else {
                return Err(self.unexpected());
            };
            unit = (unit << 4) | u16::from(digit);
            self.offset += 1;
        }
        Ok(unit)
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
