// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reconstructing the semantic projection from parser-owned facts.
//!
//! The projection is built from read-only CST views after a diagnostic-free
//! parse and successful semantic validation. It is never built from a partially
//! understood message, so any shape this walker does not recognize means the
//! parser changed or this code is wrong, and both are operational failures
//! rather than authoring diagnostics a caller could act on.
//!
//! Literal content is decoded here rather than read from the parser's cooked
//! values. The parser normalizes a cooked literal so that matching works, and
//! 017 requires the revision to carry the actual decoded content: two variant
//! keys the parser matches identically can still be different message content,
//! and collapsing them would silently merge two revisions.
//!
//! Names are the opposite case. Name resolution is parser-owned, so a name must
//! read exactly as the parser resolved it. The cooking below mirrors the
//! parser's rule and a differential test holds the two together.

use ox_mf2_parser::{CstChild, CstNodeView, CstView, SyntaxKind};
use unicode_normalization::UnicodeNormalization;

use super::model::{
    Attribute, CatchAllKey, CatchAllTag, Declaration, Expression, ExpressionTag, Function,
    InputDeclaration, InputTag, LiteralKey, LiteralTag, LiteralValue, LocalDeclaration, LocalTag,
    MarkupForm, MarkupPart, MarkupTag, MatchBody, MatchTag, MessageBody, MessageProjection, Opt,
    PatternBody, PatternPart, PatternTag, TextPart, TextTag, Value, VariableTag, VariableValue,
    Variant, VariantKey,
};
use crate::limits::{AuthoringLimits, LimitKind};

/// Complete failure of a projection construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProjectionFailure {
    /// A named bound was exhausted.
    Limit(LimitKind),
    /// The admitted syntax tree had a shape this walker does not recognize.
    UnsupportedStructure,
}

/// Strip bidirectional isolate and mark characters, then apply NFC.
///
/// This mirrors the parser's own name cooking. Applying it to a name keeps the
/// projection aligned with parser-owned resolution; it is deliberately never
/// applied to literal content.
fn cook_name(raw: &str) -> String {
    raw.chars()
        .filter(|character| {
            !matches!(
                character,
                '\u{061c}'
                    | '\u{200e}'
                    | '\u{200f}'
                    | '\u{2066}'
                    | '\u{2067}'
                    | '\u{2068}'
                    | '\u{2069}'
            )
        })
        .nfc()
        .collect()
}

/// Cook an identifier, preserving its optional namespace separator.
fn cook_identifier(raw: &str) -> String {
    raw.split(':').map(cook_name).collect::<Vec<_>>().join(":")
}

pub(crate) struct Builder<'a> {
    view: CstView<'a>,
    limits: &'a AuthoringLimits,
    nodes: u64,
}

impl<'a> Builder<'a> {
    pub(crate) const fn new(view: CstView<'a>, limits: &'a AuthoringLimits) -> Self {
        Self {
            view,
            limits,
            nodes: 0,
        }
    }

    fn account(&mut self) -> Result<(), ProjectionFailure> {
        self.nodes += 1;
        if self.nodes > self.limits.projection_nodes {
            return Err(ProjectionFailure::Limit(LimitKind::ProjectionNodes));
        }
        Ok(())
    }

    fn depth(&self, depth: u32) -> Result<(), ProjectionFailure> {
        if u64::from(depth) > self.limits.projection_depth {
            return Err(ProjectionFailure::Limit(LimitKind::ProjectionDepth));
        }
        Ok(())
    }

    /// Collect a node's content text, decoding MF2 escapes.
    ///
    /// Content tokens can sit one level down, as a variable's name does, so the
    /// walk descends. Trivia is excluded because it belongs to tokens rather
    /// than to the run, which keeps a bidi mark around a literal out of its
    /// value. Delimiters are skipped by kind rather than by position, while a
    /// namespace separator is kept because it is part of an identifier.
    fn content_text(node: &CstNodeView<'a>) -> String {
        let mut text = String::new();
        Self::collect_content(node, &mut text);
        text
    }

    fn collect_content(node: &CstNodeView<'a>, text: &mut String) {
        for child in node.children() {
            match child {
                CstChild::Node(inner) => Self::collect_content(&inner, text),
                CstChild::Token(token) => match token.kind() {
                    // `\X` carries exactly the escaped scalar.
                    SyntaxKind::EscapeToken => text.push_str(&token.text()[1..]),
                    SyntaxKind::TextToken
                    | SyntaxKind::QuotedTextToken
                    | SyntaxKind::NameToken
                    | SyntaxKind::ColonToken => text.push_str(token.text()),
                    _ => {}
                },
            }
        }
    }

    /// Unwrap the placeholder a declaration wraps its expression in.
    fn unwrap_placeholder(node: &CstNodeView<'a>) -> Result<CstNodeView<'a>, ProjectionFailure> {
        if node.kind() == SyntaxKind::Placeholder {
            Self::first_child(node)
        } else {
            Ok(*node)
        }
    }

    fn child_nodes(node: &CstNodeView<'a>) -> Vec<CstNodeView<'a>> {
        node.children()
            .filter_map(|child| match child {
                CstChild::Node(inner) => Some(inner),
                CstChild::Token(_) => None,
            })
            .collect()
    }

    fn first_child(node: &CstNodeView<'a>) -> Result<CstNodeView<'a>, ProjectionFailure> {
        Self::child_nodes(node)
            .into_iter()
            .next()
            .ok_or(ProjectionFailure::UnsupportedStructure)
    }

    /// Build the complete message from the parse root.
    pub(crate) fn message(&mut self) -> Result<MessageProjection, ProjectionFailure> {
        let root = self
            .view
            .root()
            .ok_or(ProjectionFailure::UnsupportedStructure)?;
        let message = Self::first_child(&root)?;
        match message.kind() {
            SyntaxKind::SimpleMessage => {
                let pattern = Self::first_child(&message)?;
                let parts = self.pattern(&pattern, 1)?;
                self.account()?;
                Ok(MessageProjection {
                    declarations: Box::new([]),
                    body: MessageBody::Pattern(PatternBody {
                        kind: PatternTag::Value,
                        parts,
                    }),
                })
            }
            SyntaxKind::ComplexMessage => self.complex(&message),
            _ => Err(ProjectionFailure::UnsupportedStructure),
        }
    }

    fn complex(
        &mut self,
        message: &CstNodeView<'a>,
    ) -> Result<MessageProjection, ProjectionFailure> {
        let mut declarations = Vec::new();
        let mut body = None;
        for child in Self::child_nodes(message) {
            match child.kind() {
                SyntaxKind::InputDeclaration => {
                    declarations.push(Declaration::Input(self.input(&child)?));
                }
                SyntaxKind::LocalDeclaration => {
                    declarations.push(Declaration::Local(self.local(&child)?));
                }
                SyntaxKind::ComplexBody => body = Some(self.body(&child)?),
                SyntaxKind::QuotedPattern | SyntaxKind::Matcher => {
                    body = Some(self.body_inner(&child)?);
                }
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        self.account()?;
        Ok(MessageProjection {
            declarations: declarations.into_boxed_slice(),
            body: body.ok_or(ProjectionFailure::UnsupportedStructure)?,
        })
    }

    fn body(&mut self, node: &CstNodeView<'a>) -> Result<MessageBody, ProjectionFailure> {
        let inner = Self::first_child(node)?;
        self.body_inner(&inner)
    }

    fn body_inner(&mut self, node: &CstNodeView<'a>) -> Result<MessageBody, ProjectionFailure> {
        match node.kind() {
            SyntaxKind::QuotedPattern => {
                // The quoted wrapper is syntax; it adds no projection node.
                let pattern = Self::first_child(node)?;
                Ok(MessageBody::Pattern(PatternBody {
                    kind: PatternTag::Value,
                    parts: self.pattern(&pattern, 1)?,
                }))
            }
            SyntaxKind::Matcher => self.matcher(node),
            _ => Err(ProjectionFailure::UnsupportedStructure),
        }
    }

    fn matcher(&mut self, node: &CstNodeView<'a>) -> Result<MessageBody, ProjectionFailure> {
        let mut selectors = Vec::new();
        let mut variants = Vec::new();
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::Selector => {
                    let variable = Self::first_child(&child)?;
                    self.account()?;
                    selectors.push(cook_name(&Self::content_text(&variable)));
                }
                SyntaxKind::Variant => variants.push(self.variant(&child)?),
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        Ok(MessageBody::Match(MatchBody {
            kind: MatchTag::Value,
            selectors: selectors.into_boxed_slice(),
            variants: variants.into_boxed_slice(),
        }))
    }

    fn variant(&mut self, node: &CstNodeView<'a>) -> Result<Variant, ProjectionFailure> {
        let mut keys = Vec::new();
        let mut parts = None;
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::VariantKey => keys.push(self.variant_key(&child)?),
                SyntaxKind::CatchAllKey => {
                    self.account()?;
                    keys.push(VariantKey::CatchAll(CatchAllKey {
                        kind: CatchAllTag::Value,
                    }));
                }
                SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral => {
                    self.account()?;
                    keys.push(VariantKey::Literal(LiteralKey {
                        kind: LiteralTag::Value,
                        value: Self::content_text(&child),
                    }));
                }
                SyntaxKind::QuotedPattern => {
                    let pattern = Self::first_child(&child)?;
                    parts = Some(self.pattern(&pattern, 2)?);
                }
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        self.account()?;
        Ok(Variant {
            keys: keys.into_boxed_slice(),
            parts: parts.ok_or(ProjectionFailure::UnsupportedStructure)?,
        })
    }

    fn variant_key(&mut self, node: &CstNodeView<'a>) -> Result<VariantKey, ProjectionFailure> {
        self.account()?;
        let children = Self::child_nodes(node);
        match children.first() {
            Some(inner) if inner.kind() == SyntaxKind::CatchAllKey => {
                Ok(VariantKey::CatchAll(CatchAllKey {
                    kind: CatchAllTag::Value,
                }))
            }
            Some(inner) => Ok(VariantKey::Literal(LiteralKey {
                kind: LiteralTag::Value,
                value: Self::content_text(inner),
            })),
            // A bare `*` key has no child node of its own.
            None if node
                .tokens()
                .any(|token| token.kind() == SyntaxKind::StarToken) =>
            {
                Ok(VariantKey::CatchAll(CatchAllKey {
                    kind: CatchAllTag::Value,
                }))
            }
            None => Ok(VariantKey::Literal(LiteralKey {
                kind: LiteralTag::Value,
                value: Self::content_text(node),
            })),
        }
    }

    /// Build one pattern's parts, merging adjacent text and omitting empty runs.
    fn pattern(
        &mut self,
        node: &CstNodeView<'a>,
        depth: u32,
    ) -> Result<Box<[PatternPart]>, ProjectionFailure> {
        self.depth(depth)?;
        if node.kind() != SyntaxKind::Pattern {
            return Err(ProjectionFailure::UnsupportedStructure);
        }
        let mut parts: Vec<PatternPart> = Vec::new();
        let mut run = String::new();
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::Text => run.push_str(&Self::content_text(&child)),
                SyntaxKind::Placeholder => {
                    self.flush(&mut run, &mut parts)?;
                    let inner = Self::first_child(&child)?;
                    parts.push(self.placeholder(&inner, depth + 1)?);
                }
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        self.flush(&mut run, &mut parts)?;
        Ok(parts.into_boxed_slice())
    }

    fn flush(
        &mut self,
        run: &mut String,
        parts: &mut Vec<PatternPart>,
    ) -> Result<(), ProjectionFailure> {
        if run.is_empty() {
            return Ok(());
        }
        self.account()?;
        parts.push(PatternPart::Text(TextPart {
            kind: TextTag::Value,
            value: std::mem::take(run),
        }));
        Ok(())
    }

    fn placeholder(
        &mut self,
        node: &CstNodeView<'a>,
        depth: u32,
    ) -> Result<PatternPart, ProjectionFailure> {
        self.depth(depth)?;
        match node.kind() {
            SyntaxKind::LiteralExpression
            | SyntaxKind::VariableExpression
            | SyntaxKind::FunctionExpression => {
                Ok(PatternPart::Expression(self.expression(node, depth)?))
            }
            SyntaxKind::Markup => Ok(PatternPart::Markup(self.markup(node, depth)?)),
            _ => Err(ProjectionFailure::UnsupportedStructure),
        }
    }

    fn expression(
        &mut self,
        node: &CstNodeView<'a>,
        depth: u32,
    ) -> Result<Expression, ProjectionFailure> {
        self.depth(depth)?;
        self.account()?;
        let mut operand = None;
        let mut function = None;
        let mut attributes = Vec::new();
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral => {
                    operand = Some(Value::Literal(LiteralValue {
                        kind: LiteralTag::Value,
                        value: Self::content_text(&child),
                    }));
                }
                SyntaxKind::Variable => {
                    operand = Some(Value::Variable(VariableValue {
                        kind: VariableTag::Value,
                        name: cook_name(&Self::content_text(&child)),
                    }));
                }
                SyntaxKind::Function => function = Some(self.function(&child, depth + 1)?),
                SyntaxKind::Attribute => attributes.push(self.attribute(&child)?),
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        if operand.is_none() && function.is_none() {
            return Err(ProjectionFailure::UnsupportedStructure);
        }
        Ok(Expression {
            kind: ExpressionTag::Value,
            operand,
            function,
            attributes: attributes.into_boxed_slice(),
        })
    }

    fn markup(
        &mut self,
        node: &CstNodeView<'a>,
        depth: u32,
    ) -> Result<MarkupPart, ProjectionFailure> {
        self.depth(depth)?;
        self.account()?;
        if node.kind() != SyntaxKind::Markup {
            return Err(ProjectionFailure::UnsupportedStructure);
        }
        // `{#name}` opens, `{/name}` closes, and `{#name /}` stands alone, so
        // the form is read from the punctuation rather than from a node kind.
        let opens = node
            .tokens()
            .any(|token| token.kind() == SyntaxKind::HashToken);
        let closes = node
            .tokens()
            .any(|token| token.kind() == SyntaxKind::SlashToken);
        let form = match (opens, closes) {
            (true, true) => MarkupForm::Standalone,
            (true, false) => MarkupForm::Open,
            (false, true) => MarkupForm::Close,
            (false, false) => return Err(ProjectionFailure::UnsupportedStructure),
        };
        let mut name = None;
        let mut options = Vec::new();
        let mut attributes = Vec::new();
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::Identifier | SyntaxKind::Name if name.is_none() => {
                    name = Some(cook_identifier(&Self::content_text(&child)));
                }
                SyntaxKind::Option => options.push(self.option(&child)?),
                SyntaxKind::Attribute => attributes.push(self.attribute(&child)?),
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        Ok(MarkupPart {
            kind: MarkupTag::Value,
            form,
            name: name.ok_or(ProjectionFailure::UnsupportedStructure)?,
            options: options.into_boxed_slice(),
            attributes: attributes.into_boxed_slice(),
        })
    }

    fn function(
        &mut self,
        node: &CstNodeView<'a>,
        depth: u32,
    ) -> Result<Function, ProjectionFailure> {
        self.depth(depth)?;
        self.account()?;
        let mut name = None;
        let mut options = Vec::new();
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::Identifier | SyntaxKind::Name if name.is_none() => {
                    name = Some(cook_identifier(&Self::content_text(&child)));
                }
                SyntaxKind::Option => options.push(self.option(&child)?),
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        Ok(Function {
            name: name.ok_or(ProjectionFailure::UnsupportedStructure)?,
            options: options.into_boxed_slice(),
        })
    }

    fn option(&mut self, node: &CstNodeView<'a>) -> Result<Opt, ProjectionFailure> {
        self.account()?;
        let mut name = None;
        let mut value = None;
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::Identifier | SyntaxKind::Name if name.is_none() => {
                    name = Some(cook_identifier(&Self::content_text(&child)));
                }
                SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral => {
                    value = Some(Value::Literal(LiteralValue {
                        kind: LiteralTag::Value,
                        value: Self::content_text(&child),
                    }));
                }
                SyntaxKind::Variable => {
                    value = Some(Value::Variable(VariableValue {
                        kind: VariableTag::Value,
                        name: cook_name(&Self::content_text(&child)),
                    }));
                }
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        Ok(Opt {
            name: name.ok_or(ProjectionFailure::UnsupportedStructure)?,
            value: value.ok_or(ProjectionFailure::UnsupportedStructure)?,
        })
    }

    fn attribute(&mut self, node: &CstNodeView<'a>) -> Result<Attribute, ProjectionFailure> {
        self.account()?;
        let mut name = None;
        let mut value = None;
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::Identifier | SyntaxKind::Name if name.is_none() => {
                    name = Some(cook_identifier(&Self::content_text(&child)));
                }
                SyntaxKind::QuotedLiteral | SyntaxKind::UnquotedLiteral => {
                    value = Some(Self::content_text(&child));
                }
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        Ok(Attribute {
            name: name.ok_or(ProjectionFailure::UnsupportedStructure)?,
            value,
        })
    }

    fn input(&mut self, node: &CstNodeView<'a>) -> Result<InputDeclaration, ProjectionFailure> {
        self.account()?;
        let expression = Self::child_nodes(node)
            .into_iter()
            .find_map(|child| {
                let inner = Self::unwrap_placeholder(&child).ok()?;
                (inner.kind() == SyntaxKind::VariableExpression).then_some(inner)
            })
            .ok_or(ProjectionFailure::UnsupportedStructure)?;
        let built = self.expression(&expression, 1)?;
        let name = match built.operand {
            Some(Value::Variable(variable)) => variable.name,
            _ => return Err(ProjectionFailure::UnsupportedStructure),
        };
        Ok(InputDeclaration {
            kind: InputTag::Value,
            name,
            function: built.function,
            attributes: built.attributes,
        })
    }

    fn local(&mut self, node: &CstNodeView<'a>) -> Result<LocalDeclaration, ProjectionFailure> {
        self.account()?;
        let mut name = None;
        let mut expression = None;
        for child in Self::child_nodes(node) {
            match child.kind() {
                SyntaxKind::Variable if name.is_none() => {
                    name = Some(cook_name(&Self::content_text(&child)));
                }
                SyntaxKind::Placeholder
                | SyntaxKind::LiteralExpression
                | SyntaxKind::VariableExpression
                | SyntaxKind::FunctionExpression => {
                    let inner = Self::unwrap_placeholder(&child)?;
                    expression = Some(self.expression(&inner, 1)?);
                }
                _ => return Err(ProjectionFailure::UnsupportedStructure),
            }
        }
        Ok(LocalDeclaration {
            kind: LocalTag::Value,
            name: name.ok_or(ProjectionFailure::UnsupportedStructure)?,
            expression: expression.ok_or(ProjectionFailure::UnsupportedStructure)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ox_mf2_parser::{build_semantic_model, parse_message, DeclarationKind};

    /// Names in the projection must read exactly as the parser resolved them.
    ///
    /// Resolution is parser-owned, so if this crate cooked names differently,
    /// two messages the parser treats as one variable would get two revisions.
    /// The corpus deliberately includes bidi marks and a decomposed sequence.
    #[test]
    fn cooked_names_agree_with_the_parser_for_every_declaration() {
        for source in [
            ".local $plain = {|x|}\n{{{$plain}}}",
            ".input {$a\u{e9}}\n{{{$a\u{e9}}}}",
            ".input {$ae\u{301}}\n{{{$ae\u{301}}}}",
            ".local $\u{2066}isolated\u{2069} = {|x|}\n{{ok}}",
            ".input {$a}\n.local $b = {$a}\n{{{$b}}}",
        ] {
            let parsed = parse_message(source).unwrap();
            let result = parsed.result();
            assert!(result.diagnostics.is_empty(), "{source:?} did not parse");
            let model = build_semantic_model(parsed.sources(), result).unwrap();

            for declaration in model.semantic_declarations() {
                let expected = declaration.name();
                assert_eq!(
                    cook_name(expected),
                    expected,
                    "{source:?}: cooking a parser name must be idempotent"
                );
            }

            // Build the projection and compare its declaration names with the
            // parser's own, in order.
            let view = CstView::new(parsed.sources(), result.source, &result.cst);
            let limits = crate::limits::tests::generous();
            let message = Builder::new(view, &limits).message().unwrap();

            let parser_names: Vec<&str> = model
                .semantic_declarations()
                .iter()
                .map(ox_mf2_parser::SemanticDeclaration::name)
                .collect();
            let projected_names: Vec<&str> = message
                .declarations
                .iter()
                .map(|declaration| match declaration {
                    Declaration::Input(input) => input.name.as_str(),
                    Declaration::Local(local) => local.name.as_str(),
                })
                .collect();
            assert_eq!(projected_names, parser_names, "{source:?}");

            // An input declaration keeps its parser-assigned kind.
            for (declaration, semantic) in message
                .declarations
                .iter()
                .zip(model.semantic_declarations().iter())
            {
                let is_input = matches!(declaration, Declaration::Input(_));
                assert_eq!(is_input, semantic.kind() == DeclarationKind::Input);
            }
        }
    }

    #[test]
    fn literal_content_is_never_normalized_the_way_a_name_is() {
        // The same decomposed sequence is one value as a literal and another
        // as a name, which is exactly the distinction 017 requires.
        let decomposed = "ae\u{301}";
        assert_ne!(cook_name(decomposed), decomposed);

        let parsed = parse_message("{|ae\u{301}| :string}").unwrap();
        let result = parsed.result();
        assert!(result.diagnostics.is_empty());
        let view = CstView::new(parsed.sources(), result.source, &result.cst);
        let limits = crate::limits::tests::generous();
        let message = Builder::new(view, &limits).message().unwrap();

        let MessageBody::Pattern(body) = &message.body else {
            panic!("expected a pattern body");
        };
        let PatternPart::Expression(expression) = &body.parts[0] else {
            panic!("expected an expression");
        };
        let Some(Value::Literal(literal)) = &expression.operand else {
            panic!("expected a literal operand");
        };
        assert_eq!(literal.value, decomposed, "literal content must stay exact");
    }

    #[test]
    fn a_namespaced_identifier_keeps_its_separator() {
        let parsed = parse_message("{$a :ns:fn}").unwrap();
        let result = parsed.result();
        assert!(result.diagnostics.is_empty());
        let view = CstView::new(parsed.sources(), result.source, &result.cst);
        let limits = crate::limits::tests::generous();
        let message = Builder::new(view, &limits).message().unwrap();
        let MessageBody::Pattern(body) = &message.body else {
            panic!("expected a pattern body");
        };
        let PatternPart::Expression(expression) = &body.parts[0] else {
            panic!("expected an expression");
        };
        assert_eq!(
            expression
                .function
                .as_ref()
                .map(|function| function.name.as_str()),
            Some("ns:fn")
        );
    }

    #[test]
    fn projection_bounds_are_reported_by_their_exact_kind() {
        let parsed = parse_message("{{a{$b}c{$d}e}}").unwrap();
        let result = parsed.result();
        let view = CstView::new(parsed.sources(), result.source, &result.cst);

        let mut nodes = crate::limits::tests::generous();
        nodes.projection_nodes = 3;
        assert_eq!(
            Builder::new(view, &nodes).message(),
            Err(ProjectionFailure::Limit(LimitKind::ProjectionNodes))
        );

        let mut depth = crate::limits::tests::generous();
        depth.projection_depth = 1;
        assert_eq!(
            Builder::new(view, &depth).message(),
            Err(ProjectionFailure::Limit(LimitKind::ProjectionDepth))
        );
    }
}
