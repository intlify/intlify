// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked explicit JS/TS call recognizer configuration.
//!
//! This module validates static callee chains, call kind, scope, domain, and
//! source-facing key syntax. It canonicalizes bindings by exact callee bytes and
//! never infers a scope, domain, kind, or syntax from source or API spelling.

use std::error::Error;
use std::fmt;

use intlify_contract::{CatalogKeyDomain, CatalogScopeId};

const CALLEE_BYTES_LIMIT: u64 = 255;
const CALLEE_SEGMENTS_LIMIT: u64 = 64;

/// Semantic meaning of one configured call-expression callee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JsRecognizerCallKind {
    /// One ordinary message lookup.
    Lookup,
    /// One explicit finite message-set declaration.
    Set,
}

impl JsRecognizerCallKind {
    /// Return the exact configuration token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lookup => "lookup",
            Self::Set => "set",
        }
    }
}

/// Source-facing key spelling selected by one recognizer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JsKeySyntax {
    /// The decoded source string is already a canonical domain value.
    Canonical,
    /// The complete decoded source string is one literal JSON-Pointer segment.
    Literal,
    /// The decoded source string uses the fixed dot-path grammar.
    DotPath,
}

impl JsKeySyntax {
    /// Return the exact configuration token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Canonical => "canonical",
            Self::Literal => "literal",
            Self::DotPath => "dot-path",
        }
    }
}

/// Immutable complete binding for one exact static callee chain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsRecognizerBinding {
    callee: Box<str>,
    kind: JsRecognizerCallKind,
    scope: CatalogScopeId,
    domain: CatalogKeyDomain,
    key_syntax: JsKeySyntax,
}

impl JsRecognizerBinding {
    /// Validate and retain one complete explicit recognizer binding.
    pub fn try_new(
        callee: impl Into<Box<str>>,
        kind: JsRecognizerCallKind,
        scope: CatalogScopeId,
        domain: CatalogKeyDomain,
        key_syntax: JsKeySyntax,
    ) -> Result<Self, JsRecognizerConfigurationError> {
        let callee = callee.into();
        validate_callee(&callee)?;
        if !matches!(key_syntax, JsKeySyntax::Canonical) && domain != CatalogKeyDomain::JsonPointer
        {
            return Err(JsRecognizerConfigurationError::UnsupportedKeySyntaxDomain);
        }
        Ok(Self {
            callee,
            kind,
            scope,
            domain,
            key_syntax,
        })
    }

    /// Return the exact checked static callee chain.
    #[must_use]
    pub fn callee(&self) -> &str {
        &self.callee
    }

    /// Return the configured call kind.
    #[must_use]
    pub const fn kind(&self) -> JsRecognizerCallKind {
        self.kind
    }

    /// Return the explicit project scope.
    #[must_use]
    pub const fn scope(&self) -> &CatalogScopeId {
        &self.scope
    }

    /// Return the explicit catalog-key domain.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the configured source-facing key syntax.
    #[must_use]
    pub const fn key_syntax(&self) -> JsKeySyntax {
        self.key_syntax
    }
}

/// Canonical duplicate-free set of explicit JS/TS recognizers.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsRecognizerSet {
    bindings: Box<[JsRecognizerBinding]>,
}

impl JsRecognizerSet {
    /// Construct an explicit empty set that recognizes no API implicitly.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            bindings: Box::new([]),
        }
    }

    /// Validate duplicates and canonicalize bindings by exact callee bytes.
    pub fn try_new(
        mut bindings: Vec<JsRecognizerBinding>,
    ) -> Result<Self, JsRecognizerConfigurationError> {
        bindings.sort_by(|left, right| left.callee.as_bytes().cmp(right.callee.as_bytes()));
        if bindings
            .windows(2)
            .any(|pair| pair[0].callee == pair[1].callee)
        {
            return Err(JsRecognizerConfigurationError::DuplicateCallee);
        }
        Ok(Self {
            bindings: bindings.into_boxed_slice(),
        })
    }

    /// Return bindings in canonical exact-callee order.
    #[must_use]
    pub const fn bindings(&self) -> &[JsRecognizerBinding] {
        &self.bindings
    }

    pub(crate) fn find(&self, callee: &str) -> Option<&JsRecognizerBinding> {
        self.bindings
            .binary_search_by(|binding| binding.callee.as_bytes().cmp(callee.as_bytes()))
            .ok()
            .map(|index| &self.bindings[index])
    }
}

/// Checked recognizer-configuration failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsRecognizerConfigurationError {
    /// The complete callee key exceeds 255 ASCII bytes.
    CalleeBytes {
        /// Inclusive configured ceiling.
        limit: u64,
        /// Submitted exact byte count.
        observed: u64,
    },
    /// The complete callee chain exceeds 64 segments.
    CalleeSegments {
        /// Inclusive configured ceiling.
        limit: u64,
        /// Submitted exact segment count.
        observed: u64,
    },
    /// The callee is not one admitted static ASCII identifier chain.
    InvalidCallee,
    /// The first identifier segment is a reserved language word.
    ReservedCalleeRoot,
    /// Literal and dot-path spelling were selected outside JSON Pointer.
    UnsupportedKeySyntaxDomain,
    /// Two submitted bindings use the same exact callee.
    DuplicateCallee,
}

impl fmt::Display for JsRecognizerConfigurationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CalleeBytes { limit, observed } => {
                write!(formatter, "callee bytes {observed} exceed {limit}")
            }
            Self::CalleeSegments { limit, observed } => {
                write!(formatter, "callee segments {observed} exceed {limit}")
            }
            Self::InvalidCallee => formatter.write_str("invalid static callee chain"),
            Self::ReservedCalleeRoot => formatter.write_str("reserved callee root"),
            Self::UnsupportedKeySyntaxDomain => {
                formatter.write_str("key syntax is unsupported for the selected domain")
            }
            Self::DuplicateCallee => formatter.write_str("duplicate recognizer callee"),
        }
    }
}

impl Error for JsRecognizerConfigurationError {}

fn validate_callee(callee: &str) -> Result<(), JsRecognizerConfigurationError> {
    let byte_count = callee.len() as u64;
    if byte_count > CALLEE_BYTES_LIMIT {
        return Err(JsRecognizerConfigurationError::CalleeBytes {
            limit: CALLEE_BYTES_LIMIT,
            observed: byte_count,
        });
    }
    if callee.is_empty() || !callee.is_ascii() {
        return Err(JsRecognizerConfigurationError::InvalidCallee);
    }

    let raw_segments: Vec<_> = callee.split('.').collect();
    let segment_count = raw_segments.len() as u64;
    if segment_count > CALLEE_SEGMENTS_LIMIT {
        return Err(JsRecognizerConfigurationError::CalleeSegments {
            limit: CALLEE_SEGMENTS_LIMIT,
            observed: segment_count,
        });
    }
    if raw_segments.iter().any(|segment| !is_identifier(segment)) {
        return Err(JsRecognizerConfigurationError::InvalidCallee);
    }
    if raw_segments[0] != "this" && is_reserved_root(raw_segments[0]) {
        return Err(JsRecognizerConfigurationError::ReservedCalleeRoot);
    }

    Ok(())
}

fn is_identifier(value: &str) -> bool {
    let bytes = value.as_bytes();
    let Some((&first, remaining)) = bytes.split_first() else {
        return false;
    };
    is_identifier_start(first) && remaining.iter().copied().all(is_identifier_continue)
}

const fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || matches!(byte, b'_' | b'$')
}

const fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn is_reserved_root(value: &str) -> bool {
    matches!(
        value,
        "await"
            | "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "enum"
            | "export"
            | "extends"
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "implements"
            | "import"
            | "in"
            | "instanceof"
            | "interface"
            | "let"
            | "new"
            | "null"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "return"
            | "static"
            | "super"
            | "switch"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        validate_callee, JsRecognizerConfigurationError, CALLEE_BYTES_LIMIT, CALLEE_SEGMENTS_LIMIT,
    };

    #[test]
    fn callee_limits_are_inclusive() {
        let exact_bytes = format!("{}{}", "a".repeat(129), ".a".repeat(63));
        assert_eq!(exact_bytes.len() as u64, CALLEE_BYTES_LIMIT);
        assert!(validate_callee(&exact_bytes).is_ok());

        let first_over = format!("{exact_bytes}a");
        assert_eq!(
            validate_callee(&first_over),
            Err(JsRecognizerConfigurationError::CalleeBytes {
                limit: CALLEE_BYTES_LIMIT,
                observed: CALLEE_BYTES_LIMIT + 1,
            })
        );

        let exact_segments = std::iter::repeat_n("a", CALLEE_SEGMENTS_LIMIT as usize)
            .collect::<Vec<_>>()
            .join(".");
        assert!(validate_callee(&exact_segments).is_ok());

        let first_over_segments = std::iter::repeat_n("a", CALLEE_SEGMENTS_LIMIT as usize + 1)
            .collect::<Vec<_>>()
            .join(".");
        assert_eq!(
            validate_callee(&first_over_segments),
            Err(JsRecognizerConfigurationError::CalleeSegments {
                limit: CALLEE_SEGMENTS_LIMIT,
                observed: CALLEE_SEGMENTS_LIMIT + 1,
            })
        );
    }
}
