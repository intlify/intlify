// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Iterative syntax preflight, fail-complete collection of duplicate members
//! and values outside the JSON-compatible domain, and complete logical-domain
//! counting. Parsing stops only at the token bound or at the first byte that
//! is not UTF-8 or not strict JSON. Past a duplicate member, a lone surrogate,
//! or a number outside the Portable JSON Number domain it continues, so the
//! complete file's facts are known (D15:802, Decision 015-150). The raw-token
//! index and active member-name indexes are bounded by the raw capability.
//! Materialized value allocation starts only after this pass and admission of
//! the complete value both succeed.

use std::collections::BTreeMap;

use super::{
    byte_count, lex::Lexer, ByteSpan, DuplicateMember, Incompatibility, IncompatibleValue,
    InputCounts, InputFailure, Token, TokenKind,
};
use crate::input_limits::{Bound, RawInputLimits, ValueCounts};

/// The members an object has so far: each name, as the UTF-16 code units of
/// its value, with the spans of its keys in source order. Code units compare
/// a name with a lone surrogate as JSON spells it.
type SeenMembers = BTreeMap<Vec<u16>, Vec<ByteSpan>>;

enum Action {
    Value {
        depth: u64,
    },
    ArrayNext {
        depth: u64,
        allow_end: bool,
    },
    ArrayDelimiter {
        depth: u64,
    },
    ObjectNext {
        seen: SeenMembers,
        depth: u64,
        allow_end: bool,
    },
    ObjectDelimiter {
        seen: SeenMembers,
        depth: u64,
    },
}

/// What a complete parse found, each fact in source order.
pub(super) struct Parsed {
    pub(super) tokens: Vec<Token>,
    pub(super) counts: InputCounts,
    pub(super) duplicates: Vec<DuplicateMember>,
    pub(super) incompatible: Vec<IncompatibleValue>,
    /// Each string value and member name longer than the single-string bound.
    pub(super) long_strings: Vec<ByteSpan>,
}

/// Why parsing stopped, and how many tokens it visited.
pub(super) struct Stopped {
    pub(super) failure: InputFailure,
    pub(super) tokens: u64,
}

pub(super) fn parse(
    source: &[u8],
    limits: RawInputLimits,
    max_single_string_bytes: Bound,
) -> Result<Parsed, Stopped> {
    let mut parser = Parser {
        source,
        lexer: Lexer::new(source, limits),
        tokens: Vec::new(),
        actions: Vec::new(),
        counts: ValueCounts::default(),
        max_single_string_bytes,
        duplicates: Vec::new(),
        incompatible: Vec::new(),
        long_strings: Vec::new(),
    };
    match parser.parse() {
        Ok(()) => Ok(parser.finish()),
        Err(failure) => Err(Stopped {
            failure,
            tokens: parser.lexer.visited_tokens(),
        }),
    }
}

struct Parser<'source> {
    source: &'source [u8],
    lexer: Lexer<'source>,
    tokens: Vec<Token>,
    actions: Vec<Action>,
    counts: ValueCounts,
    max_single_string_bytes: Bound,
    duplicates: Vec<DuplicateMember>,
    incompatible: Vec<IncompatibleValue>,
    long_strings: Vec<ByteSpan>,
}

impl Parser<'_> {
    fn parse(&mut self) -> Result<(), InputFailure> {
        self.actions.push(Action::Value { depth: 1 });
        while let Some(action) = self.actions.pop() {
            match action {
                Action::Value { depth } => {
                    let token = self.token()?;
                    self.value(depth, token)?;
                }
                Action::ArrayNext { depth, allow_end } => self.array_next(depth, allow_end)?,
                Action::ArrayDelimiter { depth } => self.array_delimiter(depth)?,
                Action::ObjectNext {
                    seen,
                    depth,
                    allow_end,
                } => self.object_next(seen, depth, allow_end)?,
                Action::ObjectDelimiter { seen, depth } => self.object_delimiter(seen, depth)?,
            }
        }
        self.lexer.finish()
    }

    /// The facts of the complete parse. Objects report their duplicates when
    /// they close, inner ones first, so the duplicates are put back in source
    /// order.
    fn finish(mut self) -> Parsed {
        self.duplicates
            .sort_by_key(|duplicate| duplicate.key.start_byte());
        Parsed {
            counts: InputCounts {
                file_bytes: byte_count(self.source.len()),
                parser_tokens: self.lexer.visited_tokens(),
                value: self.counts,
            },
            tokens: self.tokens,
            duplicates: self.duplicates,
            incompatible: self.incompatible,
            long_strings: self.long_strings,
        }
    }

    fn token(&mut self) -> Result<Token, InputFailure> {
        let token = self.lexer.token()?;
        self.tokens.push(token);
        Ok(token)
    }

    /// Counts a string value or member name, and records whether it is
    /// longer than the single-string bound or outside the Unicode scalar
    /// domain.
    fn string(&mut self, decoded: u64, scalar: bool, span: ByteSpan) -> Result<(), InputFailure> {
        self.counts.total_string_bytes = self.lexer.add(self.counts.total_string_bytes, decoded)?;
        self.counts.single_string_bytes = self.counts.single_string_bytes.max(decoded);
        if decoded > self.max_single_string_bytes.get() {
            self.long_strings.push(span);
        }
        if !scalar {
            self.incompatible.push(IncompatibleValue {
                reason: Incompatibility::InvalidUnicode,
                span,
            });
        }
        Ok(())
    }

    fn value(&mut self, depth: u64, token: Token) -> Result<(), InputFailure> {
        match token.kind {
            TokenKind::Null | TokenKind::Boolean(_) | TokenKind::Number(Some(_)) => {}
            TokenKind::Number(None) => self.incompatible.push(IncompatibleValue {
                reason: Incompatibility::NonPortableNumber,
                span: token.span,
            }),
            TokenKind::String { decoded, scalar } => self.string(decoded, scalar, token.span)?,
            TokenKind::OpenArray => self.actions.push(Action::ArrayNext {
                depth,
                allow_end: true,
            }),
            TokenKind::OpenObject => self.actions.push(Action::ObjectNext {
                seen: SeenMembers::new(),
                depth,
                allow_end: true,
            }),
            _ => return Err(InputFailure::Syntax { span: token.span }),
        }
        self.counts.nodes = self.lexer.add(self.counts.nodes, 1)?;
        self.counts.depth = self.counts.depth.max(depth);
        Ok(())
    }

    fn array_next(&mut self, depth: u64, allow_end: bool) -> Result<(), InputFailure> {
        let token = self.token()?;
        if allow_end && matches!(token.kind, TokenKind::CloseArray) {
            return Ok(());
        }
        self.actions.push(Action::ArrayDelimiter { depth });
        self.counts.collection_entries = self.lexer.add(self.counts.collection_entries, 1)?;
        self.value(self.lexer.add(depth, 1)?, token)
    }

    fn array_delimiter(&mut self, depth: u64) -> Result<(), InputFailure> {
        let token = self.token()?;
        match token.kind {
            TokenKind::CloseArray => {}
            TokenKind::Comma => self.actions.push(Action::ArrayNext {
                depth,
                allow_end: false,
            }),
            _ => return Err(InputFailure::Syntax { span: token.span }),
        }
        Ok(())
    }

    fn object_next(
        &mut self,
        mut seen: SeenMembers,
        depth: u64,
        allow_end: bool,
    ) -> Result<(), InputFailure> {
        let token = self.token()?;
        if allow_end && matches!(token.kind, TokenKind::CloseObject) {
            self.close(seen);
            return Ok(());
        }
        let TokenKind::String { decoded, scalar } = token.kind else {
            return Err(InputFailure::Syntax { span: token.span });
        };
        self.string(decoded, scalar, token.span)?;
        seen.entry(name_units(self.source, token.span))
            .or_default()
            .push(token.span);
        let colon = self.token()?;
        if !matches!(colon.kind, TokenKind::Colon) {
            return Err(InputFailure::Syntax { span: colon.span });
        }
        self.counts.collection_entries = self.lexer.add(self.counts.collection_entries, 1)?;
        self.actions.push(Action::ObjectDelimiter { seen, depth });
        self.actions.push(Action::Value {
            depth: self.lexer.add(depth, 1)?,
        });
        Ok(())
    }

    fn object_delimiter(&mut self, seen: SeenMembers, depth: u64) -> Result<(), InputFailure> {
        let token = self.token()?;
        match token.kind {
            TokenKind::CloseObject => self.close(seen),
            TokenKind::Comma => self.actions.push(Action::ObjectNext {
                seen,
                depth,
                allow_end: false,
            }),
            _ => return Err(InputFailure::Syntax { span: token.span }),
        }
        Ok(())
    }

    /// Records the duplicates of a closed object: each member after the first
    /// with its name, with every other member of the name, the first
    /// included, as the others (Decision 015-194).
    fn close(&mut self, seen: SeenMembers) {
        for spans in seen.into_values().filter(|spans| spans.len() > 1) {
            for (index, &key) in spans.iter().enumerate().skip(1) {
                let others = spans
                    .iter()
                    .enumerate()
                    .filter(|&(other, _)| other != index)
                    .map(|(_, &span)| span)
                    .collect();
                self.duplicates.push(DuplicateMember { key, others });
            }
        }
    }
}

/// The UTF-16 code units of the value of the string token at `span`, which
/// strict tokenization already validated: escapes decode to their units, a
/// lone surrogate included, and other bytes are UTF-8 scalars.
fn name_units(source: &[u8], span: ByteSpan) -> Vec<u16> {
    let start = usize::try_from(span.start_byte()).expect("parser-proven source offset");
    let end = usize::try_from(span.end_byte()).expect("parser-proven source offset");
    let mut rest = &source[start + 1..end - 1];
    let mut units = Vec::new();
    while !rest.is_empty() {
        if let [b'\\', escape, tail @ ..] = rest {
            let unit = match escape {
                b'b' => 0x08,
                b'f' => 0x0c,
                b'n' => u16::from(b'\n'),
                b'r' => u16::from(b'\r'),
                b't' => u16::from(b'\t'),
                b'u' => {
                    let digits = std::str::from_utf8(&tail[..4]).expect("validated hex digits");
                    rest = &tail[4..];
                    units.push(u16::from_str_radix(digits, 16).expect("validated hex digits"));
                    continue;
                }
                other => u16::from(*other),
            };
            units.push(unit);
            rest = tail;
        } else {
            let run = rest
                .iter()
                .position(|&byte| byte == b'\\')
                .unwrap_or(rest.len());
            let text = std::str::from_utf8(&rest[..run]).expect("validated UTF-8 scalars");
            units.extend(text.encode_utf16());
            rest = &rest[run..];
        }
    }
    units
}
