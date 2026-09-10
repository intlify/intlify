// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded strict-JSON tokenization. String tokens carry a decoded byte length,
//! not an allocated value. No arbitrary-precision serde number/map path is used.

use super::{
    byte_count, ByteSpan, EntryProgress, InputFailure, MaterializationError, PortableNumber, Token,
    TokenKind,
};
use crate::input_limits::{Bound, CountRelation, InputBound, LimitViolation, RawInputLimits};

pub(super) struct Lexer<'source> {
    source: &'source str,
    offset: usize,
    tokens: u64,
    max_tokens: Bound,
}

impl<'source> Lexer<'source> {
    pub(super) fn new(source: &'source str, limits: RawInputLimits) -> Self {
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

    pub(super) fn finish(&mut self) -> Result<(), MaterializationError> {
        self.whitespace();
        if self.offset != self.source.len() {
            return Err(self.error(InputFailure::Syntax, self.current_span()));
        }
        Ok(())
    }
    pub(super) fn error(&self, reason: InputFailure, span: ByteSpan) -> MaterializationError {
        MaterializationError {
            reason,
            span: Some(span),
            related_span: None,
            progress: EntryProgress {
                file_bytes: byte_count(self.source.len()),
                parser_tokens_visited: self.tokens,
                complete_value: None,
            },
        }
    }

    pub(super) fn add(&self, left: u64, right: u64) -> Result<u64, MaterializationError> {
        left.checked_add(right)
            .ok_or_else(|| self.error(InputFailure::AccountingOverflow, self.current_span()))
    }

    pub(super) fn current_span(&self) -> ByteSpan {
        let end = if self.offset < self.source.len() {
            self.offset + 1
        } else {
            self.offset
        };
        ByteSpan::from_offsets(self.offset, end)
    }

    fn whitespace(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(|b| matches!(b, b' ' | b'\n' | b'\r' | b'\t'))
        {
            self.offset += 1;
        }
    }

    pub(super) fn token(&mut self) -> Result<Token, MaterializationError> {
        self.whitespace();
        let start = self.offset;
        let Some(&byte) = self.source.as_bytes().get(start) else {
            return Err(self.error(InputFailure::Syntax, self.current_span()));
        };
        if self.tokens == self.max_tokens.get() {
            return Err(self.error(
                InputFailure::ResourceLimits(vec![LimitViolation {
                    bound: InputBound::ParserTokens,
                    limit: self.max_tokens,
                    actual: self.max_tokens.get() + 1,
                    relation: CountRelation::AtLeast,
                }]),
                self.current_span(),
            ));
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
            b'"' => TokenKind::String(self.string()?),
            b'-' | b'0'..=b'9' => TokenKind::Number(self.number()?),
            _ => return Err(self.error(InputFailure::Syntax, self.current_span())),
        };
        Ok(Token {
            kind,
            span: ByteSpan::from_offsets(start, self.offset),
        })
    }

    fn literal(&mut self, expected: &[u8]) -> Result<(), MaterializationError> {
        for &byte in expected {
            if self.source.as_bytes().get(self.offset) != Some(&byte) {
                return Err(self.error(InputFailure::Syntax, self.current_span()));
            }
            self.offset += 1;
        }
        Ok(())
    }

    fn number(&mut self) -> Result<PortableNumber, MaterializationError> {
        let start = self.offset;
        if self.source.as_bytes()[self.offset] == b'-' {
            self.offset += 1;
        }
        match self.source.as_bytes().get(self.offset) {
            Some(b'0') => self.offset += 1,
            Some(b'1'..=b'9') => self.digits(),
            _ => return Err(self.error(InputFailure::Syntax, self.current_span())),
        }
        if self.source.as_bytes().get(self.offset) == Some(&b'.') {
            self.offset += 1;
            self.required_digits()?;
        }
        if self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(|b| matches!(b, b'e' | b'E'))
        {
            self.offset += 1;
            if self
                .source
                .as_bytes()
                .get(self.offset)
                .is_some_and(|b| matches!(b, b'+' | b'-'))
            {
                self.offset += 1;
            }
            self.required_digits()?;
        }
        PortableNumber::from_token(&self.source[start..self.offset]).ok_or_else(|| {
            self.error(
                InputFailure::NonPortableNumber,
                ByteSpan::from_offsets(start, self.offset),
            )
        })
    }

    fn digits(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.offset)
            .is_some_and(u8::is_ascii_digit)
        {
            self.offset += 1;
        }
    }

    fn required_digits(&mut self) -> Result<(), MaterializationError> {
        let start = self.offset;
        self.digits();
        if self.offset == start {
            return Err(self.error(InputFailure::Syntax, self.current_span()));
        }
        Ok(())
    }

    fn string(&mut self) -> Result<u64, MaterializationError> {
        self.offset += 1;
        let mut decoded_bytes = 0_u64;
        loop {
            let bytes = match self.source.as_bytes().get(self.offset) {
                Some(b'"') => {
                    self.offset += 1;
                    return Ok(decoded_bytes);
                }
                Some(b'\\') => self.escape()?,
                Some(0..=0x1f) | None => {
                    return Err(self.error(InputFailure::Syntax, self.current_span()))
                }
                Some(_) => {
                    self.offset += 1;
                    1
                }
            };
            // Whole-input UTF-8 admission lets unescaped bytes contribute one
            // byte each, while escapes contribute the decoded scalar's length.
            decoded_bytes = self.add(decoded_bytes, bytes)?;
        }
    }

    fn escape(&mut self) -> Result<u64, MaterializationError> {
        let start = self.offset;
        self.offset += 1;
        match self.source.as_bytes().get(self.offset) {
            Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                self.offset += 1;
                Ok(1)
            }
            Some(b'u') => {
                self.offset += 1;
                let unit = self.hex_unit()?;
                if (0xd800..=0xdbff).contains(&unit) {
                    if !self.source.as_bytes()[self.offset..].starts_with(b"\\u") {
                        return Err(self.error(
                            InputFailure::NonScalarString,
                            ByteSpan::from_offsets(start, self.offset),
                        ));
                    }
                    self.offset += 2;
                    let low = self.hex_unit()?;
                    if !(0xdc00..=0xdfff).contains(&low) {
                        return Err(self.error(
                            InputFailure::NonScalarString,
                            ByteSpan::from_offsets(start, self.offset),
                        ));
                    }
                    Ok(4)
                } else if (0xdc00..=0xdfff).contains(&unit) {
                    Err(self.error(
                        InputFailure::NonScalarString,
                        ByteSpan::from_offsets(start, self.offset),
                    ))
                } else {
                    let scalar =
                        char::from_u32(u32::from(unit)).expect("non-surrogate UTF-16 unit");
                    Ok(byte_count(scalar.len_utf8()))
                }
            }
            _ => Err(self.error(InputFailure::Syntax, self.current_span())),
        }
    }
    fn hex_unit(&mut self) -> Result<u16, MaterializationError> {
        let mut unit = 0_u16;
        for _ in 0..4 {
            let digit = match self.source.as_bytes().get(self.offset) {
                Some(byte @ b'0'..=b'9') => byte - b'0',
                Some(byte @ b'a'..=b'f') => byte - b'a' + 10,
                Some(byte @ b'A'..=b'F') => byte - b'A' + 10,
                _ => return Err(self.error(InputFailure::Syntax, self.current_span())),
            };
            unit = (unit << 4) | u16::from(digit);
            self.offset += 1;
        }
        Ok(unit)
    }
}
