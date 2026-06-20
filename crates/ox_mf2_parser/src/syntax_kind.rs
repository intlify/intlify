//! `SyntaxKind` enum shared by parser, tables, diagnostics, and snapshots.
//!
//! Numeric values are part of the Phase 2 Binary AST snapshot wire format
//! contract. Do not reorder, reuse, or change their meaning incompatibly.
//! New kinds get new values; obsolete kinds become reserved holes.
//!
//! Categories are based on the MF2 grammar productions in
//! `refers/message-format-wg/spec/message.abnf` plus implementation-specific
//! `Error`, `Missing`, and `Unknown` nodes used by recovery.

/// Shared classification for nodes, tokens, trivia, errors, and missing
/// productions. Compact `u16` representation that is stored directly in node,
/// token, and trivia records.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u16)]
#[non_exhaustive]
pub enum SyntaxKind {
    // ── Sentinel ───────────────────────────────────────────────────────────
    /// Uninitialised slot. Never emitted by the parser; used as a default to
    /// detect forgotten initialisation in table builders.
    Tombstone = 0,

    // ── Root / messages (1..10) ────────────────────────────────────────────
    /// Top-level parse root; always exactly one per parse.
    Root = 1,
    /// `message = simple-message`. Holds a single `pattern`.
    SimpleMessage = 2,
    /// `message = complex-message`. Declarations + `complex-body`.
    ComplexMessage = 3,

    // ── Pattern / text (10..20) ────────────────────────────────────────────
    /// `pattern = *(text-char / escaped-char / placeholder)`.
    Pattern = 10,
    /// Run of `text-char` (and `escaped-char`) inside a `pattern`.
    Text = 11,
    /// `quoted-pattern = "{{" pattern "}}"`.
    QuotedPattern = 12,
    /// `placeholder` wrapper containing an `expression` or `markup`.
    Placeholder = 13,

    // ── Expression (20..30) ────────────────────────────────────────────────
    /// `literal-expression`.
    LiteralExpression = 20,
    /// `variable-expression`.
    VariableExpression = 21,
    /// `function-expression`.
    FunctionExpression = 22,
    /// `function = ":" identifier *(s option)`.
    Function = 23,
    /// `option = identifier o "=" o (literal / variable)`.
    Option = 24,
    /// `attribute = "@" identifier [o "=" o literal]`.
    Attribute = 25,

    // ── Declarations / matcher / variants (30..50) ─────────────────────────
    /// `local-declaration = local s variable o "=" o expression`.
    LocalDeclaration = 30,
    /// `input-declaration = input o variable-expression`.
    InputDeclaration = 31,
    /// `complex-body` wrapper: either `quoted-pattern` or `matcher`.
    ComplexBody = 32,
    /// `matcher = match-statement s variant *(o variant)`.
    Matcher = 33,
    /// `selector = variable`. Reference target of `match-statement`.
    Selector = 34,
    /// `variant = key *(s key) o quoted-pattern`.
    Variant = 35,
    /// `key = literal / "*"`.
    VariantKey = 36,
    /// Catch-all key `*`.
    CatchAllKey = 37,

    // ── Markup (50..60) ────────────────────────────────────────────────────
    /// `markup` parent node.
    Markup = 50,
    /// Open markup: `{ # identifier ... }`.
    MarkupOpen = 51,
    /// Standalone markup: `{ # identifier ... / }`.
    MarkupStandalone = 52,
    /// Close markup: `{ / identifier ... }`.
    MarkupClose = 53,

    // ── Literal / name / identifier (60..80) ───────────────────────────────
    /// `quoted-literal = "|" ... "|"`.
    QuotedLiteral = 60,
    /// `unquoted-literal = 1*name-char`.
    UnquotedLiteral = 61,
    /// `name = [bidi] name-start *name-char [bidi]`.
    Name = 62,
    /// `identifier = [namespace ":"] name`.
    Identifier = 63,
    /// `variable = "$" name`.
    Variable = 64,

    // ── Punctuation tokens (100..150) ──────────────────────────────────────
    /// `{` — placeholder / expression open, also the head of `{{`.
    LeftBraceToken = 100,
    /// `}` — placeholder / expression close, also the tail of `}}`.
    RightBraceToken = 101,
    /// `{{` — quoted-pattern open.
    LeftDoubleBraceToken = 102,
    /// `}}` — quoted-pattern close.
    RightDoubleBraceToken = 103,
    /// `.` — declaration / keyword prefix.
    DotToken = 104,
    /// `@` — attribute prefix.
    AtToken = 105,
    /// `|` — quoted-literal delimiter.
    PipeToken = 106,
    /// `=` — option / attribute assignment.
    EqualsToken = 107,
    /// `:` — function / identifier namespace separator.
    ColonToken = 108,
    /// `$` — variable prefix.
    DollarToken = 109,
    /// `/` — markup close prefix or standalone marker.
    SlashToken = 110,
    /// `*` — catch-all key.
    StarToken = 111,
    /// `#` — markup open prefix.
    HashToken = 112,

    // ── Keyword tokens (150..170) ──────────────────────────────────────────
    /// `.input`.
    InputKeyword = 150,
    /// `.local`.
    LocalKeyword = 151,
    /// `.match`.
    MatchKeyword = 152,

    // ── Content tokens (170..200) ──────────────────────────────────────────
    /// `name` production lexeme.
    NameToken = 170,
    /// `text-char` run; payload of [`SyntaxKind::Text`].
    TextToken = 171,
    /// `quoted-char` run; payload of [`SyntaxKind::QuotedLiteral`].
    QuotedTextToken = 172,
    /// `escaped-char` (`\{`, `\}`, `\|`, `\\`).
    EscapeToken = 173,

    // ── Trivia (200..220) ──────────────────────────────────────────────────
    /// `ws` — `SP`, `HTAB`, `CR`, `LF`, `U+3000`.
    WhitespaceTrivia = 200,
    /// `bidi` — ALM, LRM, RLM, LRI, RLI, FSI, PDI.
    BidiTrivia = 201,

    // ── Error / Missing / Unknown (300..320) ───────────────────────────────
    /// Synthetic node containing skipped / unrecognised input for recovery.
    Error = 300,
    /// Synthetic placeholder for a required production that was absent.
    Missing = 301,
    /// Reserved escape hatch for future / forward-compatibility decoders.
    Unknown = 302,
}

impl Default for SyntaxKind {
    fn default() -> Self {
        Self::Tombstone
    }
}

impl SyntaxKind {
    #[inline]
    pub const fn as_u16(self) -> u16 {
        self as u16
    }

    /// Recovery markers used by the parser when input is malformed.
    #[inline]
    pub const fn is_error(self) -> bool {
        matches!(self, Self::Error | Self::Missing | Self::Unknown)
    }

    #[inline]
    pub const fn is_root(self) -> bool {
        matches!(self, Self::Root)
    }

    /// `message` family: root, simple, complex.
    #[inline]
    pub const fn is_message(self) -> bool {
        matches!(self, Self::SimpleMessage | Self::ComplexMessage)
    }

    /// `pattern` family.
    #[inline]
    pub const fn is_pattern(self) -> bool {
        matches!(self, Self::Pattern | Self::QuotedPattern)
    }

    /// `expression` family.
    #[inline]
    pub const fn is_expression(self) -> bool {
        matches!(
            self,
            Self::LiteralExpression | Self::VariableExpression | Self::FunctionExpression
        )
    }

    /// `declaration` family.
    #[inline]
    pub const fn is_declaration(self) -> bool {
        matches!(self, Self::LocalDeclaration | Self::InputDeclaration)
    }

    /// `markup` family — open, standalone, close (plus the wrapping node).
    #[inline]
    pub const fn is_markup(self) -> bool {
        matches!(
            self,
            Self::Markup | Self::MarkupOpen | Self::MarkupStandalone | Self::MarkupClose
        )
    }

    /// `literal` family.
    #[inline]
    pub const fn is_literal(self) -> bool {
        matches!(self, Self::QuotedLiteral | Self::UnquotedLiteral)
    }

    /// Punctuation tokens.
    #[inline]
    pub const fn is_punctuation_token(self) -> bool {
        matches!(
            self,
            Self::LeftBraceToken
                | Self::RightBraceToken
                | Self::LeftDoubleBraceToken
                | Self::RightDoubleBraceToken
                | Self::DotToken
                | Self::AtToken
                | Self::PipeToken
                | Self::EqualsToken
                | Self::ColonToken
                | Self::DollarToken
                | Self::SlashToken
                | Self::StarToken
                | Self::HashToken
        )
    }

    /// `.input` / `.local` / `.match` keyword tokens.
    #[inline]
    pub const fn is_keyword_token(self) -> bool {
        matches!(
            self,
            Self::InputKeyword | Self::LocalKeyword | Self::MatchKeyword
        )
    }

    /// `ws` or `bidi` trivia.
    #[inline]
    pub const fn is_trivia(self) -> bool {
        matches!(self, Self::WhitespaceTrivia | Self::BidiTrivia)
    }

    /// All token kinds — punctuation, keyword, content.
    #[inline]
    pub const fn is_token(self) -> bool {
        self.is_punctuation_token()
            || self.is_keyword_token()
            || matches!(
                self,
                Self::NameToken | Self::TextToken | Self::QuotedTextToken | Self::EscapeToken
            )
    }

    /// True if the kind represents a CST node (non-token, non-trivia, non-sentinel).
    #[inline]
    pub const fn is_node(self) -> bool {
        !self.is_token() && !self.is_trivia() && !matches!(self, Self::Tombstone)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lock in the numeric category boundaries so accidental reorderings show
    /// up here rather than as silent snapshot corruption.
    #[test]
    fn category_boundaries_are_stable() {
        assert_eq!(SyntaxKind::Tombstone.as_u16(), 0);
        assert_eq!(SyntaxKind::Root.as_u16(), 1);
        assert_eq!(SyntaxKind::SimpleMessage.as_u16(), 2);
        assert_eq!(SyntaxKind::ComplexMessage.as_u16(), 3);
        assert_eq!(SyntaxKind::Pattern.as_u16(), 10);
        assert_eq!(SyntaxKind::QuotedPattern.as_u16(), 12);
        assert_eq!(SyntaxKind::Placeholder.as_u16(), 13);
        assert_eq!(SyntaxKind::LiteralExpression.as_u16(), 20);
        assert_eq!(SyntaxKind::VariableExpression.as_u16(), 21);
        assert_eq!(SyntaxKind::FunctionExpression.as_u16(), 22);
        assert_eq!(SyntaxKind::LocalDeclaration.as_u16(), 30);
        assert_eq!(SyntaxKind::InputDeclaration.as_u16(), 31);
        assert_eq!(SyntaxKind::Matcher.as_u16(), 33);
        assert_eq!(SyntaxKind::Variant.as_u16(), 35);
        assert_eq!(SyntaxKind::CatchAllKey.as_u16(), 37);
        assert_eq!(SyntaxKind::Markup.as_u16(), 50);
        assert_eq!(SyntaxKind::QuotedLiteral.as_u16(), 60);
        assert_eq!(SyntaxKind::UnquotedLiteral.as_u16(), 61);
        assert_eq!(SyntaxKind::Name.as_u16(), 62);
        assert_eq!(SyntaxKind::Identifier.as_u16(), 63);
        assert_eq!(SyntaxKind::Variable.as_u16(), 64);
        assert_eq!(SyntaxKind::LeftBraceToken.as_u16(), 100);
        assert_eq!(SyntaxKind::InputKeyword.as_u16(), 150);
        assert_eq!(SyntaxKind::WhitespaceTrivia.as_u16(), 200);
        assert_eq!(SyntaxKind::Error.as_u16(), 300);
        assert_eq!(SyntaxKind::Missing.as_u16(), 301);
        assert_eq!(SyntaxKind::Unknown.as_u16(), 302);
    }

    #[test]
    fn category_predicates_are_consistent() {
        assert!(SyntaxKind::Root.is_root());
        assert!(SyntaxKind::SimpleMessage.is_message());
        assert!(SyntaxKind::Pattern.is_pattern());
        assert!(SyntaxKind::LiteralExpression.is_expression());
        assert!(SyntaxKind::LocalDeclaration.is_declaration());
        assert!(SyntaxKind::MarkupOpen.is_markup());
        assert!(SyntaxKind::QuotedLiteral.is_literal());
        assert!(SyntaxKind::LeftBraceToken.is_punctuation_token());
        assert!(SyntaxKind::InputKeyword.is_keyword_token());
        assert!(SyntaxKind::WhitespaceTrivia.is_trivia());
        assert!(SyntaxKind::TextToken.is_token());
        assert!(SyntaxKind::Pattern.is_node());

        assert!(SyntaxKind::Error.is_error());
        assert!(SyntaxKind::Missing.is_error());
        assert!(SyntaxKind::Unknown.is_error());
        assert!(!SyntaxKind::Root.is_error());

        // tokens are not nodes, trivia is not a node
        assert!(!SyntaxKind::LeftBraceToken.is_node());
        assert!(!SyntaxKind::WhitespaceTrivia.is_node());
        assert!(!SyntaxKind::Tombstone.is_node());
    }
}
