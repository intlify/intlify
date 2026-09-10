// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Iterative syntax/duplicate preflight and complete logical-domain counting.
//! The raw-token index and active member-name indexes are bounded by the raw
//! capability. Materialized value allocation starts only after this pass and
//! logical-limit admission both succeed.

use std::collections::BTreeMap;

use super::{
    byte_count, decode_string, lex::Lexer, ByteSpan, InputCounts, InputFailure,
    MaterializationError, ParsedIndex, Token, TokenKind,
};
use crate::input_limits::{RawInputLimits, ValueCounts};

type SeenMembers = BTreeMap<String, ByteSpan>;

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

pub(super) fn parse(
    source: &str,
    limits: RawInputLimits,
) -> Result<ParsedIndex, MaterializationError> {
    Parser {
        source,
        lexer: Lexer::new(source, limits),
        tokens: Vec::new(),
        actions: Vec::new(),
        counts: ValueCounts::default(),
    }
    .parse()
}

struct Parser<'source> {
    source: &'source str,
    lexer: Lexer<'source>,
    tokens: Vec<Token>,
    actions: Vec<Action>,
    counts: ValueCounts,
}

impl Parser<'_> {
    fn parse(mut self) -> Result<ParsedIndex, MaterializationError> {
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
        self.lexer.finish()?;
        Ok(ParsedIndex {
            tokens: self.tokens,
            counts: InputCounts {
                file_bytes: byte_count(self.source.len()),
                parser_tokens: self.lexer.visited_tokens(),
                value: self.counts,
            },
        })
    }

    fn token(&mut self) -> Result<Token, MaterializationError> {
        let token = self.lexer.token()?;
        self.tokens.push(token);
        Ok(token)
    }

    fn string_count(&mut self, length: u64) -> Result<(), MaterializationError> {
        self.counts.total_string_bytes = self.lexer.add(self.counts.total_string_bytes, length)?;
        self.counts.single_string_bytes = self.counts.single_string_bytes.max(length);
        Ok(())
    }

    fn value(&mut self, depth: u64, token: Token) -> Result<(), MaterializationError> {
        match token.kind {
            TokenKind::Null | TokenKind::Boolean(_) | TokenKind::Number(_) => {}
            TokenKind::String(length) => self.string_count(length)?,
            TokenKind::OpenArray => self.actions.push(Action::ArrayNext {
                depth,
                allow_end: true,
            }),
            TokenKind::OpenObject => self.actions.push(Action::ObjectNext {
                seen: SeenMembers::new(),
                depth,
                allow_end: true,
            }),
            _ => return Err(self.lexer.error(InputFailure::Syntax, token.span)),
        }
        self.counts.nodes = self.lexer.add(self.counts.nodes, 1)?;
        self.counts.depth = self.counts.depth.max(depth);
        Ok(())
    }

    fn array_next(&mut self, depth: u64, allow_end: bool) -> Result<(), MaterializationError> {
        let token = self.token()?;
        if allow_end && matches!(token.kind, TokenKind::CloseArray) {
            return Ok(());
        }
        self.actions.push(Action::ArrayDelimiter { depth });
        self.counts.collection_entries = self.lexer.add(self.counts.collection_entries, 1)?;
        self.value(self.lexer.add(depth, 1)?, token)
    }

    fn array_delimiter(&mut self, depth: u64) -> Result<(), MaterializationError> {
        let token = self.token()?;
        match token.kind {
            TokenKind::CloseArray => {}
            TokenKind::Comma => self.actions.push(Action::ArrayNext {
                depth,
                allow_end: false,
            }),
            _ => return Err(self.lexer.error(InputFailure::Syntax, token.span)),
        }
        Ok(())
    }

    fn object_next(
        &mut self,
        mut seen: SeenMembers,
        depth: u64,
        allow_end: bool,
    ) -> Result<(), MaterializationError> {
        let token = self.token()?;
        if allow_end && matches!(token.kind, TokenKind::CloseObject) {
            return Ok(());
        }
        let TokenKind::String(length) = token.kind else {
            return Err(self.lexer.error(InputFailure::Syntax, token.span));
        };
        self.string_count(length)?;
        let name = decode_string(self.source, token.span);
        if let Some(first) = seen.get(&name) {
            let mut error = self.lexer.error(InputFailure::DuplicateMember, token.span);
            error.related_span = Some(*first);
            return Err(error);
        }
        seen.insert(name, token.span);
        let colon = self.token()?;
        if !matches!(colon.kind, TokenKind::Colon) {
            return Err(self.lexer.error(InputFailure::Syntax, colon.span));
        }
        self.counts.collection_entries = self.lexer.add(self.counts.collection_entries, 1)?;
        self.actions.push(Action::ObjectDelimiter { seen, depth });
        self.actions.push(Action::Value {
            depth: self.lexer.add(depth, 1)?,
        });
        Ok(())
    }

    fn object_delimiter(
        &mut self,
        seen: SeenMembers,
        depth: u64,
    ) -> Result<(), MaterializationError> {
        let token = self.token()?;
        match token.kind {
            TokenKind::CloseObject => {}
            TokenKind::Comma => self.actions.push(Action::ObjectNext {
                seen,
                depth,
                allow_end: false,
            }),
            _ => return Err(self.lexer.error(InputFailure::Syntax, token.span)),
        }
        Ok(())
    }
}
