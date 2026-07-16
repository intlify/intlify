// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::{Utf8ByteSpan, MAX_NESTING_DEPTH};

const NO_PARENT: u32 = u32::MAX;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JsonNodeKind {
    Object,
    Array,
    String,
    NonString,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JsonPathStep {
    Root,
    Member(Utf8ByteSpan),
    Index(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JsonTapeNode {
    kind: JsonNodeKind,
    span: Utf8ByteSpan,
    parent: u32,
    step: JsonPathStep,
}

impl JsonTapeNode {
    #[cfg(test)]
    pub(super) const fn kind(self) -> JsonNodeKind {
        self.kind
    }

    pub(super) const fn span(self) -> Utf8ByteSpan {
        self.span
    }

    pub(super) const fn parent(self) -> Option<u32> {
        if self.parent == NO_PARENT {
            None
        } else {
            Some(self.parent)
        }
    }

    pub(super) const fn step(self) -> JsonPathStep {
        self.step
    }
}

/// Construction-local, raw-order JSON syntax representation.
#[derive(Debug)]
pub(super) struct JsonSyntaxTape {
    nodes: Vec<JsonTapeNode>,
}

impl JsonSyntaxTape {
    pub(super) fn node(&self, index: u32) -> JsonTapeNode {
        self.nodes[usize::try_from(index).expect("u32 tape index fits usize")]
    }

    pub(super) fn string_nodes(&self) -> impl Iterator<Item = (u32, JsonTapeNode)> + '_ {
        self.nodes
            .iter()
            .copied()
            .enumerate()
            .filter(|(_, node)| node.kind == JsonNodeKind::String)
            .map(|(index, node)| {
                (
                    u32::try_from(index).expect("host byte limit bounds tape indices"),
                    node,
                )
            })
    }

    #[cfg(test)]
    fn nodes(&self) -> &[JsonTapeNode] {
        &self.nodes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JsonSyntaxError {
    pub(super) span: Utf8ByteSpan,
    pub(super) line: u32,
    pub(super) byte_column: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JsonFrontendError {
    Syntax(JsonSyntaxError),
    NestingDepth { span: Utf8ByteSpan, actual: u128 },
}

pub(super) fn parse_json(source: &str) -> Result<JsonSyntaxTape, JsonFrontendError> {
    let parser_start = usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8();
    Parser::new(source, parser_start).parse()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenKind {
    LeftBrace,
    RightBrace,
    LeftBracket,
    RightBracket,
    Colon,
    Comma,
    String,
    Number,
    True,
    False,
    Null,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Token {
    kind: TokenKind,
    span: Utf8ByteSpan,
}

struct Lexer<'source> {
    source: &'source str,
    cursor: usize,
}

impl<'source> Lexer<'source> {
    const fn new(source: &'source str, cursor: usize) -> Self {
        Self { source, cursor }
    }

    fn next_token(&mut self) -> Result<Option<Token>, JsonSyntaxError> {
        self.skip_whitespace();
        if self.cursor == self.source.len() {
            return Ok(None);
        }

        let start = self.cursor;
        let byte = self.source.as_bytes()[start];
        let kind = match byte {
            b'{' => {
                self.cursor += 1;
                TokenKind::LeftBrace
            }
            b'}' => {
                self.cursor += 1;
                TokenKind::RightBrace
            }
            b'[' => {
                self.cursor += 1;
                TokenKind::LeftBracket
            }
            b']' => {
                self.cursor += 1;
                TokenKind::RightBracket
            }
            b':' => {
                self.cursor += 1;
                TokenKind::Colon
            }
            b',' => {
                self.cursor += 1;
                TokenKind::Comma
            }
            b'"' => {
                self.scan_string()?;
                TokenKind::String
            }
            b'-' | b'0'..=b'9' => {
                self.scan_number()?;
                TokenKind::Number
            }
            b't' => {
                self.scan_keyword(b"true")?;
                TokenKind::True
            }
            b'f' => {
                self.scan_keyword(b"false")?;
                TokenKind::False
            }
            b'n' => {
                self.scan_keyword(b"null")?;
                TokenKind::Null
            }
            _ => return Err(self.error_at_scalar(start)),
        };

        Ok(Some(Token {
            kind,
            span: Self::span(start, self.cursor),
        }))
    }

    fn required_token(&mut self) -> Result<Token, JsonSyntaxError> {
        self.next_token()?
            .ok_or_else(|| self.error(self.source.len(), self.source.len()))
    }

    fn skip_whitespace(&mut self) {
        while self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.cursor += 1;
        }
    }

    fn scan_string(&mut self) -> Result<(), JsonSyntaxError> {
        self.cursor += 1;
        loop {
            let Some(&byte) = self.source.as_bytes().get(self.cursor) else {
                return Err(self.error(self.source.len(), self.source.len()));
            };
            match byte {
                b'"' => {
                    self.cursor += 1;
                    return Ok(());
                }
                b'\\' => {
                    let escape_start = self.cursor;
                    self.cursor += 1;
                    let Some(&escaped) = self.source.as_bytes().get(self.cursor) else {
                        return Err(self.error(self.source.len(), self.source.len()));
                    };
                    match escaped {
                        b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't' => {
                            self.cursor += 1;
                        }
                        b'u' => {
                            self.cursor += 1;
                            for _ in 0..4 {
                                let Some(&hex) = self.source.as_bytes().get(self.cursor) else {
                                    return Err(self.error(self.source.len(), self.source.len()));
                                };
                                if !hex.is_ascii_hexdigit() {
                                    return Err(self.error_at_scalar(self.cursor));
                                }
                                self.cursor += 1;
                            }
                        }
                        _ => {
                            return Err(self.error(escape_start, self.scalar_end(self.cursor)));
                        }
                    }
                }
                0x00..=0x1f => return Err(self.error(self.cursor, self.cursor + 1)),
                _ => {
                    self.cursor = self.scalar_end(self.cursor);
                }
            }
        }
    }

    fn scan_keyword(&mut self, expected: &[u8]) -> Result<(), JsonSyntaxError> {
        let start = self.cursor;
        for (relative, expected_byte) in expected.iter().enumerate() {
            let position = start + relative;
            let Some(&actual) = self.source.as_bytes().get(position) else {
                return Err(self.error(self.source.len(), self.source.len()));
            };
            if actual != *expected_byte {
                return Err(self.error_at_scalar(position));
            }
        }
        self.cursor += expected.len();
        Ok(())
    }

    fn scan_number(&mut self) -> Result<(), JsonSyntaxError> {
        if self.source.as_bytes()[self.cursor] == b'-' {
            self.cursor += 1;
        }

        let Some(&integer_start) = self.source.as_bytes().get(self.cursor) else {
            return Err(self.error(self.source.len(), self.source.len()));
        };
        match integer_start {
            b'0' => {
                self.cursor += 1;
                if self
                    .source
                    .as_bytes()
                    .get(self.cursor)
                    .is_some_and(u8::is_ascii_digit)
                {
                    return Err(self.error(self.cursor, self.cursor + 1));
                }
            }
            b'1'..=b'9' => {
                self.cursor += 1;
                while self
                    .source
                    .as_bytes()
                    .get(self.cursor)
                    .is_some_and(u8::is_ascii_digit)
                {
                    self.cursor += 1;
                }
            }
            _ => return Err(self.error_at_scalar(self.cursor)),
        }

        if self.source.as_bytes().get(self.cursor) == Some(&b'.') {
            self.cursor += 1;
            self.scan_required_digits()?;
        }
        if self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(|byte| matches!(byte, b'e' | b'E'))
        {
            self.cursor += 1;
            if self
                .source
                .as_bytes()
                .get(self.cursor)
                .is_some_and(|byte| matches!(byte, b'+' | b'-'))
            {
                self.cursor += 1;
            }
            self.scan_required_digits()?;
        }
        Ok(())
    }

    fn scan_required_digits(&mut self) -> Result<(), JsonSyntaxError> {
        if !self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(u8::is_ascii_digit)
        {
            return if self.cursor == self.source.len() {
                Err(self.error(self.cursor, self.cursor))
            } else {
                Err(self.error_at_scalar(self.cursor))
            };
        }
        while self
            .source
            .as_bytes()
            .get(self.cursor)
            .is_some_and(u8::is_ascii_digit)
        {
            self.cursor += 1;
        }
        Ok(())
    }

    fn error_at_scalar(&self, start: usize) -> JsonSyntaxError {
        self.error(start, self.scalar_end(start))
    }

    fn scalar_end(&self, start: usize) -> usize {
        self.source[start..]
            .chars()
            .next()
            .map_or(start, |character| start + character.len_utf8())
    }

    fn error(&self, start: usize, end: usize) -> JsonSyntaxError {
        let (line, byte_column) = source_location(self.source, start);
        JsonSyntaxError {
            span: Self::span(start, end),
            line,
            byte_column,
        }
    }

    fn span(start: usize, end: usize) -> Utf8ByteSpan {
        Utf8ByteSpan::new(
            u32::try_from(start).expect("host byte limit fits u32"),
            u32::try_from(end).expect("host byte limit fits u32"),
        )
    }
}

#[derive(Debug, Clone, Copy)]
enum ObjectState {
    KeyOrEnd,
    Key,
    Colon(Utf8ByteSpan),
    Value(Utf8ByteSpan),
    CommaOrEnd,
}

#[derive(Debug, Clone, Copy)]
enum ArrayState {
    ValueOrEnd,
    Value,
    CommaOrEnd,
}

#[derive(Debug, Clone, Copy)]
enum FrameKind {
    Object(ObjectState),
    Array { state: ArrayState, next_index: u32 },
}

#[derive(Debug, Clone, Copy)]
struct Frame {
    node: u32,
    kind: FrameKind,
}

#[derive(Debug, Clone, Copy)]
struct ValueContext {
    parent: u32,
    step: JsonPathStep,
}

impl ValueContext {
    const fn root() -> Self {
        Self {
            parent: NO_PARENT,
            step: JsonPathStep::Root,
        }
    }
}

struct Parser<'source> {
    lexer: Lexer<'source>,
    nodes: Vec<JsonTapeNode>,
    stack: Vec<Frame>,
    root_started: bool,
}

impl<'source> Parser<'source> {
    const fn new(source: &'source str, parser_start: usize) -> Self {
        Self {
            lexer: Lexer::new(source, parser_start),
            nodes: Vec::new(),
            stack: Vec::new(),
            root_started: false,
        }
    }

    fn parse(mut self) -> Result<JsonSyntaxTape, JsonFrontendError> {
        loop {
            if self.stack.is_empty() {
                if !self.root_started {
                    let token = self.lexer.required_token()?;
                    self.root_started = true;
                    self.consume_value(token, ValueContext::root())?;
                    continue;
                }

                if let Some(extra) = self.lexer.next_token()? {
                    return Err(JsonFrontendError::Syntax(self.lexer.error(
                        usize::try_from(extra.span.start()).expect("u32 fits usize"),
                        usize::try_from(extra.span.end()).expect("u32 fits usize"),
                    )));
                }
                return Ok(JsonSyntaxTape { nodes: self.nodes });
            }

            self.advance_frame()?;
        }
    }

    fn advance_frame(&mut self) -> Result<(), JsonFrontendError> {
        let frame_index = self.stack.len() - 1;
        let frame = self.stack[frame_index];
        match frame.kind {
            FrameKind::Object(ObjectState::KeyOrEnd) => {
                let token = self.lexer.required_token()?;
                match token.kind {
                    TokenKind::RightBrace => self.close_container(token),
                    TokenKind::String => {
                        self.stack[frame_index].kind =
                            FrameKind::Object(ObjectState::Colon(token.span));
                    }
                    _ => return Err(self.syntax_token(token)),
                }
            }
            FrameKind::Object(ObjectState::Key) => {
                let token = self.lexer.required_token()?;
                if token.kind != TokenKind::String {
                    return Err(self.syntax_token(token));
                }
                self.stack[frame_index].kind = FrameKind::Object(ObjectState::Colon(token.span));
            }
            FrameKind::Object(ObjectState::Colon(key)) => {
                let token = self.lexer.required_token()?;
                if token.kind != TokenKind::Colon {
                    return Err(self.syntax_token(token));
                }
                self.stack[frame_index].kind = FrameKind::Object(ObjectState::Value(key));
            }
            FrameKind::Object(ObjectState::Value(key)) => {
                let token = self.lexer.required_token()?;
                self.stack[frame_index].kind = FrameKind::Object(ObjectState::CommaOrEnd);
                self.consume_value(
                    token,
                    ValueContext {
                        parent: frame.node,
                        step: JsonPathStep::Member(key),
                    },
                )?;
            }
            FrameKind::Object(ObjectState::CommaOrEnd) => {
                let token = self.lexer.required_token()?;
                match token.kind {
                    TokenKind::Comma => {
                        self.stack[frame_index].kind = FrameKind::Object(ObjectState::Key);
                    }
                    TokenKind::RightBrace => self.close_container(token),
                    _ => return Err(self.syntax_token(token)),
                }
            }
            FrameKind::Array {
                state: ArrayState::ValueOrEnd,
                next_index,
            } => {
                let token = self.lexer.required_token()?;
                if token.kind == TokenKind::RightBracket {
                    self.close_container(token);
                } else {
                    self.stack[frame_index].kind = FrameKind::Array {
                        state: ArrayState::CommaOrEnd,
                        next_index: next_index + 1,
                    };
                    self.consume_value(
                        token,
                        ValueContext {
                            parent: frame.node,
                            step: JsonPathStep::Index(next_index),
                        },
                    )?;
                }
            }
            FrameKind::Array {
                state: ArrayState::Value,
                next_index,
            } => {
                let token = self.lexer.required_token()?;
                self.stack[frame_index].kind = FrameKind::Array {
                    state: ArrayState::CommaOrEnd,
                    next_index: next_index + 1,
                };
                self.consume_value(
                    token,
                    ValueContext {
                        parent: frame.node,
                        step: JsonPathStep::Index(next_index),
                    },
                )?;
            }
            FrameKind::Array {
                state: ArrayState::CommaOrEnd,
                next_index,
            } => {
                let token = self.lexer.required_token()?;
                match token.kind {
                    TokenKind::Comma => {
                        self.stack[frame_index].kind = FrameKind::Array {
                            state: ArrayState::Value,
                            next_index,
                        };
                    }
                    TokenKind::RightBracket => self.close_container(token),
                    _ => return Err(self.syntax_token(token)),
                }
            }
        }
        Ok(())
    }

    fn consume_value(
        &mut self,
        token: Token,
        context: ValueContext,
    ) -> Result<(), JsonFrontendError> {
        let kind = match token.kind {
            TokenKind::String => JsonNodeKind::String,
            TokenKind::Number | TokenKind::True | TokenKind::False | TokenKind::Null => {
                JsonNodeKind::NonString
            }
            TokenKind::LeftBrace => JsonNodeKind::Object,
            TokenKind::LeftBracket => JsonNodeKind::Array,
            _ => return Err(self.syntax_token(token)),
        };

        if matches!(kind, JsonNodeKind::Object | JsonNodeKind::Array) {
            let actual = self.stack.len() as u128 + 1;
            if actual > u128::from(MAX_NESTING_DEPTH) {
                return Err(JsonFrontendError::NestingDepth {
                    span: token.span,
                    actual,
                });
            }
        }

        let node = u32::try_from(self.nodes.len()).expect("host byte limit bounds tape indices");
        self.nodes.push(JsonTapeNode {
            kind,
            span: token.span,
            parent: context.parent,
            step: context.step,
        });

        match kind {
            JsonNodeKind::Object => self.stack.push(Frame {
                node,
                kind: FrameKind::Object(ObjectState::KeyOrEnd),
            }),
            JsonNodeKind::Array => self.stack.push(Frame {
                node,
                kind: FrameKind::Array {
                    state: ArrayState::ValueOrEnd,
                    next_index: 0,
                },
            }),
            JsonNodeKind::String | JsonNodeKind::NonString => {}
        }
        Ok(())
    }

    fn close_container(&mut self, token: Token) {
        let frame = self.stack.pop().expect("a closing token has an open frame");
        let node = &mut self.nodes[usize::try_from(frame.node).expect("u32 fits usize")];
        node.span = Utf8ByteSpan::new(node.span.start(), token.span.end());
    }

    fn syntax_token(&self, token: Token) -> JsonFrontendError {
        JsonFrontendError::Syntax(self.lexer.error(
            usize::try_from(token.span.start()).expect("u32 fits usize"),
            usize::try_from(token.span.end()).expect("u32 fits usize"),
        ))
    }
}

fn source_location(source: &str, offset: usize) -> (u32, u32) {
    let mut line = 1_u32;
    let mut column = 0_u32;
    let mut cursor = 0;
    let bytes = source.as_bytes();
    while cursor < offset {
        match bytes[cursor] {
            b'\r' => {
                line += 1;
                column = 0;
                cursor += 1;
                if cursor < offset && bytes.get(cursor) == Some(&b'\n') {
                    cursor += 1;
                }
            }
            b'\n' => {
                line += 1;
                column = 0;
                cursor += 1;
            }
            _ => {
                column += 1;
                cursor += 1;
            }
        }
    }
    (line, column)
}

impl From<JsonSyntaxError> for JsonFrontendError {
    fn from(error: JsonSyntaxError) -> Self {
        Self::Syntax(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_json, JsonFrontendError, JsonNodeKind, JsonPathStep};
    use crate::MAX_NESTING_DEPTH;

    #[test]
    fn accepts_complete_rfc_8259_value_grammar() {
        let valid = [
            "null",
            "true",
            "false",
            "0",
            "-0",
            "123",
            "-12.34e+5",
            r#""text\\\"\/\b\f\n\r\t\u00e9""#,
            "[]",
            "{}",
            r#"{"a":1,"a":[true,false,null,{"b":"value"}]}"#,
            " \t\r\n[0,1.0,2e-3] \r\n",
        ];

        for source in valid {
            assert!(parse_json(source).is_ok(), "valid JSON rejected: {source}");
        }
    }

    #[test]
    fn rejects_incomplete_or_extended_json_syntax() {
        let invalid = [
            "",
            " ",
            "+1",
            "01",
            "1.",
            "1e",
            "tru",
            "undefined",
            "'text'",
            "[1,]",
            "[,1]",
            r#"{"a":1,}"#,
            r"{a:1}",
            r#"{"a" 1}"#,
            r#""bad\x""#,
            "\"line\nfeed\"",
            "{}{}",
        ];

        for source in invalid {
            assert!(
                matches!(parse_json(source), Err(JsonFrontendError::Syntax(_))),
                "invalid JSON accepted: {source}"
            );
        }
    }

    #[test]
    fn tape_retains_duplicate_members_and_mixed_value_roles_in_raw_order() {
        let tape = parse_json(r#"{"a":"first","a":[0,{"b":"second"}]}"#).unwrap();
        let kinds = tape
            .nodes()
            .iter()
            .map(|node| node.kind())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            [
                JsonNodeKind::Object,
                JsonNodeKind::String,
                JsonNodeKind::Array,
                JsonNodeKind::NonString,
                JsonNodeKind::Object,
                JsonNodeKind::String,
            ]
        );

        let member_steps = tape
            .nodes()
            .iter()
            .filter_map(|node| match node.step() {
                JsonPathStep::Member(span) => Some(span),
                JsonPathStep::Root | JsonPathStep::Index(_) => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(member_steps.len(), 3);
        assert_eq!(
            &r#"{"a":"first","a":[0,{"b":"second"}]}"#[usize::try_from(member_steps[0].start())
                .unwrap()
                ..usize::try_from(member_steps[0].end()).unwrap()],
            "\"a\""
        );
        assert_eq!(
            &r#"{"a":"first","a":[0,{"b":"second"}]}"#[usize::try_from(member_steps[1].start())
                .unwrap()
                ..usize::try_from(member_steps[1].end()).unwrap()],
            "\"a\""
        );
    }

    #[test]
    fn accepts_exact_depth_limit_and_rejects_the_next_opening_token() {
        let depth = usize::try_from(MAX_NESTING_DEPTH).unwrap();
        let accepted = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        assert!(parse_json(&accepted).is_ok());

        let rejected = format!("{}0{}", "[".repeat(depth + 1), "]".repeat(depth + 1));
        let error = parse_json(&rejected).unwrap_err();
        assert_eq!(
            error,
            JsonFrontendError::NestingDepth {
                span: crate::Utf8ByteSpan::new(MAX_NESTING_DEPTH, MAX_NESTING_DEPTH + 1),
                actual: u128::from(MAX_NESTING_DEPTH) + 1,
            }
        );
    }

    #[test]
    fn syntax_and_depth_failures_follow_source_order_without_recursion() {
        let depth = usize::try_from(MAX_NESTING_DEPTH).unwrap();
        assert!(matches!(
            parse_json(&format!("?{}", "[".repeat(depth + 1))),
            Err(JsonFrontendError::Syntax(_))
        ));
        assert!(matches!(
            parse_json(&format!("{}?", "[".repeat(depth + 1))),
            Err(JsonFrontendError::NestingDepth { .. })
        ));

        let complete = format!("{}\"ok\"{}", "{\"a\":".repeat(depth), "}".repeat(depth));
        assert!(parse_json(&complete).is_ok());
    }

    #[test]
    fn accepts_one_leading_bom_and_rejects_repeated_or_out_of_place_bom() {
        let tape = parse_json("\u{feff}{\"a\":\"value\"}").unwrap();
        let (_, string) = tape.string_nodes().next().unwrap();
        assert_eq!(string.span().start(), 8);

        for source in ["\u{feff}\u{feff}{}", "{}\u{feff}", "[\u{feff}]"] {
            assert!(matches!(
                parse_json(source),
                Err(JsonFrontendError::Syntax(_))
            ));
        }
        assert!(parse_json("\"\u{feff}\"").is_ok());
    }

    #[test]
    fn syntax_locations_use_absolute_utf8_byte_columns_and_eof_offsets() {
        let error = parse_json("\"é\" ?").unwrap_err();
        let JsonFrontendError::Syntax(error) = error else {
            panic!("expected syntax failure");
        };
        assert_eq!(error.span.start(), 5);
        assert_eq!((error.line, error.byte_column), (1, 5));

        let error = parse_json("[\r\n  1,").unwrap_err();
        let JsonFrontendError::Syntax(error) = error else {
            panic!("expected EOF syntax failure");
        };
        assert_eq!(
            usize::try_from(error.span.start()).unwrap(),
            "[\r\n  1,".len()
        );
        assert_eq!((error.line, error.byte_column), (2, 4));
    }

    #[test]
    fn tape_node_representation_stays_compact_and_linear() {
        assert!(std::mem::size_of::<super::JsonTapeNode>() <= 32);
        let source = format!("[{}]", vec!["0"; 1_000].join(","));
        let tape = parse_json(&source).unwrap();
        assert_eq!(tape.nodes().len(), 1_001);
    }
}
