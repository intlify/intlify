// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Strict JSON syntax tree used by versioned artifact decoders.
//!
//! The parser preserves object-member occurrences and semantic array order,
//! retains numeric token spellings, decodes strings to Unicode scalar
//! sequences, and uses an explicit container stack so untrusted nesting cannot
//! consume the Rust call stack.
//!
//! It does not apply an artifact schema, normalize values, select semantic
//! errors, or expose byte offsets through the public artifact contract.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JsonParseError {
    Syntax,
    TrailingData,
}

pub(super) type NodeId = usize;

#[derive(Debug)]
pub(super) struct JsonDocument {
    nodes: Vec<JsonNode>,
    root: NodeId,
}

impl JsonDocument {
    pub(super) fn parse(source: &str) -> Result<Self, JsonParseError> {
        Parser::new(source).parse()
    }

    pub(super) fn root(&self) -> NodeId {
        self.root
    }

    pub(super) fn node(&self, id: NodeId) -> &JsonNode {
        &self.nodes[id]
    }
}

#[derive(Debug)]
pub(super) enum JsonNode {
    Null,
    Bool,
    Number(Box<str>),
    String(Box<str>),
    Array(Box<[NodeId]>),
    Object(Box<[JsonMember]>),
}

#[derive(Debug)]
pub(super) struct JsonMember {
    pub(super) name: Box<str>,
    pub(super) value: NodeId,
}

#[derive(Debug)]
enum Frame {
    Array {
        items: Vec<NodeId>,
        state: ArrayState,
    },
    Object {
        members: Vec<JsonMember>,
        pending_name: Option<Box<str>>,
        state: ObjectState,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ArrayState {
    FirstOrEnd,
    Value,
    CommaOrEnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ObjectState {
    FirstKeyOrEnd,
    Key,
    Colon,
    Value,
    CommaOrEnd,
}

struct Parser<'a> {
    source: &'a str,
    bytes: &'a [u8],
    offset: usize,
    nodes: Vec<JsonNode>,
    stack: Vec<Frame>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            bytes: source.as_bytes(),
            offset: 0,
            nodes: Vec::new(),
            stack: Vec::new(),
        }
    }

    fn parse(mut self) -> Result<JsonDocument, JsonParseError> {
        let mut root = None;
        let mut pending_value = None;

        loop {
            if let Some(value) = pending_value.take() {
                if let Some(parent) = self.stack.last_mut() {
                    match parent {
                        Frame::Array { items, state } if *state == ArrayState::Value => {
                            items.push(value);
                            *state = ArrayState::CommaOrEnd;
                        }
                        Frame::Object {
                            members,
                            pending_name,
                            state,
                        } if *state == ObjectState::Value => {
                            let name = pending_name
                                .take()
                                .expect("an object value always follows one parsed name");
                            members.push(JsonMember { name, value });
                            *state = ObjectState::CommaOrEnd;
                        }
                        _ => return Err(JsonParseError::Syntax),
                    }
                } else if root.replace(value).is_some() {
                    return Err(JsonParseError::Syntax);
                }
                continue;
            }

            if root.is_some() && self.stack.is_empty() {
                break;
            }

            if self.stack.is_empty() {
                pending_value = self.parse_value_start()?;
                continue;
            }

            self.skip_whitespace();
            let frame = self
                .stack
                .pop()
                .expect("the empty-stack case was handled above");
            match frame {
                Frame::Array { items, state } => match state {
                    ArrayState::FirstOrEnd => {
                        if self.consume(b']') {
                            pending_value =
                                Some(self.push_node(JsonNode::Array(items.into_boxed_slice())));
                        } else {
                            self.stack.push(Frame::Array {
                                items,
                                state: ArrayState::Value,
                            });
                        }
                    }
                    ArrayState::Value => {
                        self.stack.push(Frame::Array {
                            items,
                            state: ArrayState::Value,
                        });
                        pending_value = self.parse_value_start()?;
                    }
                    ArrayState::CommaOrEnd => {
                        if self.consume(b',') {
                            self.stack.push(Frame::Array {
                                items,
                                state: ArrayState::Value,
                            });
                        } else if self.consume(b']') {
                            pending_value =
                                Some(self.push_node(JsonNode::Array(items.into_boxed_slice())));
                        } else {
                            return Err(JsonParseError::Syntax);
                        }
                    }
                },
                Frame::Object {
                    members,
                    mut pending_name,
                    state,
                } => match state {
                    ObjectState::FirstKeyOrEnd => {
                        if self.consume(b'}') {
                            pending_value =
                                Some(self.push_node(JsonNode::Object(members.into_boxed_slice())));
                        } else {
                            pending_name = Some(self.parse_string()?.into_boxed_str());
                            self.stack.push(Frame::Object {
                                members,
                                pending_name,
                                state: ObjectState::Colon,
                            });
                        }
                    }
                    ObjectState::Key => {
                        pending_name = Some(self.parse_string()?.into_boxed_str());
                        self.stack.push(Frame::Object {
                            members,
                            pending_name,
                            state: ObjectState::Colon,
                        });
                    }
                    ObjectState::Colon => {
                        if !self.consume(b':') {
                            return Err(JsonParseError::Syntax);
                        }
                        self.stack.push(Frame::Object {
                            members,
                            pending_name,
                            state: ObjectState::Value,
                        });
                    }
                    ObjectState::Value => {
                        self.stack.push(Frame::Object {
                            members,
                            pending_name,
                            state: ObjectState::Value,
                        });
                        pending_value = self.parse_value_start()?;
                    }
                    ObjectState::CommaOrEnd => {
                        if self.consume(b',') {
                            self.stack.push(Frame::Object {
                                members,
                                pending_name,
                                state: ObjectState::Key,
                            });
                        } else if self.consume(b'}') {
                            pending_value =
                                Some(self.push_node(JsonNode::Object(members.into_boxed_slice())));
                        } else {
                            return Err(JsonParseError::Syntax);
                        }
                    }
                },
            }
        }

        self.skip_whitespace();
        if self.offset != self.bytes.len() {
            return Err(JsonParseError::TrailingData);
        }
        Ok(JsonDocument {
            nodes: self.nodes,
            root: root.ok_or(JsonParseError::Syntax)?,
        })
    }

    fn parse_value_start(&mut self) -> Result<Option<NodeId>, JsonParseError> {
        self.skip_whitespace();
        let Some(byte) = self.peek() else {
            return Err(JsonParseError::Syntax);
        };
        match byte {
            b'{' => {
                self.offset += 1;
                self.stack.push(Frame::Object {
                    members: Vec::new(),
                    pending_name: None,
                    state: ObjectState::FirstKeyOrEnd,
                });
                Ok(None)
            }
            b'[' => {
                self.offset += 1;
                self.stack.push(Frame::Array {
                    items: Vec::new(),
                    state: ArrayState::FirstOrEnd,
                });
                Ok(None)
            }
            b'"' => {
                let value = self.parse_string()?.into_boxed_str();
                Ok(Some(self.push_node(JsonNode::String(value))))
            }
            b'n' => {
                self.parse_literal(b"null")?;
                Ok(Some(self.push_node(JsonNode::Null)))
            }
            b't' => {
                self.parse_literal(b"true")?;
                Ok(Some(self.push_node(JsonNode::Bool)))
            }
            b'f' => {
                self.parse_literal(b"false")?;
                Ok(Some(self.push_node(JsonNode::Bool)))
            }
            b'-' | b'0'..=b'9' => {
                let number = self.parse_number()?.into();
                Ok(Some(self.push_node(JsonNode::Number(number))))
            }
            _ => Err(JsonParseError::Syntax),
        }
    }

    fn parse_literal(&mut self, literal: &[u8]) -> Result<(), JsonParseError> {
        if self.bytes.get(self.offset..self.offset + literal.len()) != Some(literal) {
            return Err(JsonParseError::Syntax);
        }
        self.offset += literal.len();
        Ok(())
    }

    fn parse_number(&mut self) -> Result<&'a str, JsonParseError> {
        let start = self.offset;
        if self.consume(b'-') && self.peek().is_none() {
            return Err(JsonParseError::Syntax);
        }

        match self.peek() {
            Some(b'0') => {
                self.offset += 1;
                if self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                    return Err(JsonParseError::Syntax);
                }
            }
            Some(b'1'..=b'9') => {
                self.offset += 1;
                while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                    self.offset += 1;
                }
            }
            _ => return Err(JsonParseError::Syntax),
        }

        if self.consume(b'.') {
            if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(JsonParseError::Syntax);
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.offset += 1;
            }
        }

        if self.peek().is_some_and(|byte| matches!(byte, b'e' | b'E')) {
            self.offset += 1;
            if self.peek().is_some_and(|byte| matches!(byte, b'+' | b'-')) {
                self.offset += 1;
            }
            if !self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                return Err(JsonParseError::Syntax);
            }
            while self.peek().is_some_and(|byte| byte.is_ascii_digit()) {
                self.offset += 1;
            }
        }

        Ok(&self.source[start..self.offset])
    }

    fn parse_string(&mut self) -> Result<String, JsonParseError> {
        if !self.consume(b'"') {
            return Err(JsonParseError::Syntax);
        }

        let mut output = String::new();
        let mut segment_start = self.offset;
        loop {
            let Some(byte) = self.peek() else {
                return Err(JsonParseError::Syntax);
            };
            match byte {
                b'"' => {
                    output.push_str(&self.source[segment_start..self.offset]);
                    self.offset += 1;
                    return Ok(output);
                }
                b'\\' => {
                    output.push_str(&self.source[segment_start..self.offset]);
                    self.offset += 1;
                    let escaped = self.peek().ok_or(JsonParseError::Syntax)?;
                    self.offset += 1;
                    match escaped {
                        b'"' => output.push('"'),
                        b'\\' => output.push('\\'),
                        b'/' => output.push('/'),
                        b'b' => output.push('\u{0008}'),
                        b'f' => output.push('\u{000c}'),
                        b'n' => output.push('\n'),
                        b'r' => output.push('\r'),
                        b't' => output.push('\t'),
                        b'u' => self.parse_unicode_escape(&mut output)?,
                        _ => return Err(JsonParseError::Syntax),
                    }
                    segment_start = self.offset;
                }
                0x00..=0x1f => return Err(JsonParseError::Syntax),
                _ => self.offset += 1,
            }
        }
    }

    fn parse_unicode_escape(&mut self, output: &mut String) -> Result<(), JsonParseError> {
        let first = self.parse_hex_quad()?;
        let scalar = if (0xd800..=0xdbff).contains(&first) {
            if !self.consume(b'\\') || !self.consume(b'u') {
                return Err(JsonParseError::Syntax);
            }
            let second = self.parse_hex_quad()?;
            if !(0xdc00..=0xdfff).contains(&second) {
                return Err(JsonParseError::Syntax);
            }
            0x1_0000 + (((first as u32 - 0xd800) << 10) | (second as u32 - 0xdc00))
        } else if (0xdc00..=0xdfff).contains(&first) {
            return Err(JsonParseError::Syntax);
        } else {
            first as u32
        };
        output.push(char::from_u32(scalar).ok_or(JsonParseError::Syntax)?);
        Ok(())
    }

    fn parse_hex_quad(&mut self) -> Result<u16, JsonParseError> {
        let mut value = 0_u16;
        for _ in 0..4 {
            let byte = self.peek().ok_or(JsonParseError::Syntax)?;
            self.offset += 1;
            let digit = match byte {
                b'0'..=b'9' => u16::from(byte - b'0'),
                b'a'..=b'f' => u16::from(byte - b'a') + 10,
                b'A'..=b'F' => u16::from(byte - b'A') + 10,
                _ => return Err(JsonParseError::Syntax),
            };
            value = value * 16 + digit;
        }
        Ok(value)
    }

    fn skip_whitespace(&mut self) {
        while self
            .peek()
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\n' | b'\r'))
        {
            self.offset += 1;
        }
    }

    fn consume(&mut self, expected: u8) -> bool {
        if self.peek() == Some(expected) {
            self.offset += 1;
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }

    fn push_node(&mut self, node: JsonNode) -> NodeId {
        let id = self.nodes.len();
        self.nodes.push(node);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::{JsonDocument, JsonNode, JsonParseError};

    #[test]
    fn parser_preserves_member_duplicates_and_array_order_without_recursion() {
        let document =
            JsonDocument::parse(r#"{"a":1,"a":2,"nested":[true,null,{"s":"\uD83D\uDE00"}]}"#)
                .unwrap();
        let JsonNode::Object(root) = document.node(document.root()) else {
            panic!("expected object")
        };
        assert_eq!(root.len(), 3);
        assert_eq!(root[0].name.as_ref(), "a");
        assert_eq!(root[1].name.as_ref(), "a");
        let JsonNode::Array(nested) = document.node(root[2].value) else {
            panic!("expected array")
        };
        assert_eq!(nested.len(), 3);
        let JsonNode::Object(last) = document.node(nested[2]) else {
            panic!("expected nested object")
        };
        let JsonNode::String(value) = document.node(last[0].value) else {
            panic!("expected string")
        };
        assert_eq!(value.as_ref(), "😀");
    }

    #[test]
    fn parser_distinguishes_syntax_from_data_after_a_complete_root() {
        assert_eq!(
            JsonDocument::parse(r#"{"a":]"#).unwrap_err(),
            JsonParseError::Syntax
        );
        assert_eq!(
            JsonDocument::parse("{} true").unwrap_err(),
            JsonParseError::TrailingData
        );
        assert!(JsonDocument::parse("{} \t\r\n").is_ok());
    }

    #[test]
    fn parser_rejects_unpaired_surrogates_and_invalid_number_grammar() {
        for invalid in [r#""\uD800""#, r#""\uDC00""#, "01", "1.", "1e"] {
            assert_eq!(
                JsonDocument::parse(invalid).unwrap_err(),
                JsonParseError::Syntax
            );
        }
    }
}
