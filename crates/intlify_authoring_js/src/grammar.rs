// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The grammars a unit can be read under.
//!
//! A unit's grammar is part of its snapshot, and the caller chooses it: the
//! caller knows whether a file is loaded as a module or as a classic script.
//! Nothing here reads a file name, a suffix, a package manifest, or the source
//! itself to guess one. A unit is parsed exactly once under the grammar it
//! names, and a failed attempt is not retried under another goal, because a
//! second reading that succeeds would be a different program from the one the
//! caller loads.
//!
//! The registry is closed. JSX and TSX are absent on purpose: a JSX profile
//! has to say how text between tags is extracted, and admitting the grammar
//! without that rule would leave such text silently unlocalized.

use intlify_authoring::VersionedIdentity;
use oxc_span::SourceType;

/// Revision shared by every grammar in this registry.
///
/// A grammar revision names what the Producer accepts, not which parser
/// version it links. `tests/grammar_pin.rs` records that acceptance for a
/// fixed corpus, so a parser upgrade that changes it fails there and forces a
/// decision about this revision.
pub const GRAMMAR_REVISION: &str = "0";

/// One registered host grammar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Grammar {
    /// JavaScript under the module goal.
    JsModule,
    /// JavaScript under the script goal.
    JsScript,
    /// TypeScript under the module goal.
    TsModule,
    /// TypeScript under the script goal.
    TsScript,
}

impl Grammar {
    /// Every registered grammar, in spelling order.
    pub const ALL: [Self; 4] = [
        Self::JsModule,
        Self::JsScript,
        Self::TsModule,
        Self::TsScript,
    ];

    /// Return the exact identity spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JsModule => "intlify-grammar-js-module",
            Self::JsScript => "intlify-grammar-js-script",
            Self::TsModule => "intlify-grammar-ts-module",
            Self::TsScript => "intlify-grammar-ts-script",
        }
    }

    /// Return the pin a snapshot names to be read under this grammar.
    #[must_use]
    pub fn identity(self) -> VersionedIdentity {
        VersionedIdentity::literal(self.as_str(), GRAMMAR_REVISION)
    }

    /// Find the registered grammar a snapshot names.
    ///
    /// Both the identity and the revision have to match. A known identity at
    /// another revision is a grammar this Producer does not implement.
    #[must_use]
    pub fn from_identity(identity: &VersionedIdentity) -> Option<Self> {
        if identity.revision().as_str() != GRAMMAR_REVISION {
            return None;
        }
        Self::ALL
            .into_iter()
            .find(|grammar| grammar.as_str() == identity.identity().as_str())
    }

    /// Return whether this grammar reads a classic script.
    pub(crate) const fn is_script(self) -> bool {
        matches!(self, Self::JsScript | Self::TsScript)
    }

    /// Return the parser configuration for this grammar.
    ///
    /// Each goal is set explicitly. The parser's own TypeScript default decides
    /// between module and script from the source, which is the inference this
    /// registry exists to avoid.
    pub(crate) const fn source_type(self) -> SourceType {
        match self {
            Self::JsModule => SourceType::mjs(),
            Self::JsScript => SourceType::script(),
            Self::TsModule => SourceType::ts().with_module(true),
            Self::TsScript => SourceType::ts().with_script(true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_grammar_is_found_only_by_its_exact_identity_and_revision() {
        for grammar in Grammar::ALL {
            assert_eq!(Grammar::from_identity(&grammar.identity()), Some(grammar));
        }
        assert_eq!(
            Grammar::from_identity(&VersionedIdentity::literal(
                "intlify-grammar-js-module",
                "1"
            )),
            None,
            "a known grammar at another revision is not implemented here"
        );
        assert_eq!(
            Grammar::from_identity(&VersionedIdentity::literal(
                "intlify-grammar-jsx-module",
                "0"
            )),
            None,
            "JSX is not registered"
        );
    }

    #[test]
    fn every_grammar_sets_its_goal_rather_than_inferring_it() {
        for grammar in Grammar::ALL {
            let source_type = grammar.source_type();
            assert_eq!(source_type.is_script(), grammar.is_script());
            assert_eq!(source_type.is_module(), !grammar.is_script());
            assert!(!source_type.is_unambiguous());
            assert!(!source_type.is_jsx());
        }
        assert!(Grammar::TsModule.source_type().is_typescript());
        assert!(Grammar::TsScript.source_type().is_typescript());
        assert!(!Grammar::JsModule.source_type().is_typescript());
    }

    #[test]
    fn spellings_are_distinct_and_in_order() {
        let spellings: Vec<&str> = Grammar::ALL
            .iter()
            .map(|grammar| grammar.as_str())
            .collect();
        let mut sorted = spellings.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted, spellings);
    }
}
