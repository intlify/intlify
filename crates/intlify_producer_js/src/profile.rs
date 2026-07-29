// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exact JS/TS suffix selection and parser profile construction.
//!
//! Profiles are derived only from the final portable filename segment. This
//! module does not inspect project metadata, source contents, editor language
//! IDs, MIME information, or another tool's loader configuration.

use std::error::Error;
use std::fmt;

use intlify_contract::SourceDocumentIdentity;
use oxc_span::SourceType;

/// Language grammar selected for one admitted source suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JsGrammarProfile {
    /// JavaScript without JSX.
    JavaScript,
    /// JavaScript with JSX.
    JavaScriptJsx,
    /// TypeScript without JSX.
    TypeScript,
    /// TypeScript with JSX.
    TypeScriptJsx,
    /// TypeScript declaration source.
    TypeScriptDeclaration,
}

/// Contract source-goal policy selected from one exact suffix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JsSourceGoal {
    /// Parse as module, then retry once as script only after syntax rejection.
    BoundedUnambiguous,
    /// Parse exactly once with module goal.
    Module,
    /// Parse exactly once with CommonJS/script goal.
    CommonJs,
}

/// Authoritative source goal selected after complete frontend admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum JsSelectedSourceGoal {
    /// The admitted AST was parsed with module goal.
    Module,
    /// The admitted AST was parsed with script/CommonJS goal.
    Script,
}

/// Immutable grammar and source-goal profile for one selected logical source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JsSourceProfile {
    grammar: JsGrammarProfile,
    goal: JsSourceGoal,
}

impl JsSourceProfile {
    /// Derive a profile from the source's exact final path segment.
    pub fn from_source(source: &SourceDocumentIdentity) -> Result<Self, JsSourceProfileError> {
        let filename = source
            .path()
            .segments()
            .last()
            .expect("checked source paths are non-empty")
            .as_str();
        Self::from_filename(filename)
    }

    /// Derive a profile from one exact case-sensitive filename.
    #[allow(clippy::case_sensitive_file_extension_comparisons)]
    pub fn from_filename(filename: &str) -> Result<Self, JsSourceProfileError> {
        let profile = if filename.ends_with(".d.mts") {
            Self::new(
                JsGrammarProfile::TypeScriptDeclaration,
                JsSourceGoal::Module,
            )
        } else if filename.ends_with(".d.cts") {
            Self::new(
                JsGrammarProfile::TypeScriptDeclaration,
                JsSourceGoal::CommonJs,
            )
        } else if filename.ends_with(".d.ts") {
            Self::new(
                JsGrammarProfile::TypeScriptDeclaration,
                JsSourceGoal::BoundedUnambiguous,
            )
        } else if filename.ends_with(".mjs") {
            Self::new(JsGrammarProfile::JavaScript, JsSourceGoal::Module)
        } else if filename.ends_with(".cjs") {
            Self::new(JsGrammarProfile::JavaScript, JsSourceGoal::CommonJs)
        } else if filename.ends_with(".jsx") {
            Self::new(
                JsGrammarProfile::JavaScriptJsx,
                JsSourceGoal::BoundedUnambiguous,
            )
        } else if filename.ends_with(".js") {
            Self::new(
                JsGrammarProfile::JavaScript,
                JsSourceGoal::BoundedUnambiguous,
            )
        } else if filename.ends_with(".tsx") {
            Self::new(
                JsGrammarProfile::TypeScriptJsx,
                JsSourceGoal::BoundedUnambiguous,
            )
        } else if filename.ends_with(".mts") {
            Self::new(JsGrammarProfile::TypeScript, JsSourceGoal::Module)
        } else if filename.ends_with(".cts") {
            Self::new(JsGrammarProfile::TypeScript, JsSourceGoal::CommonJs)
        } else if filename.ends_with(".ts") {
            Self::new(
                JsGrammarProfile::TypeScript,
                JsSourceGoal::BoundedUnambiguous,
            )
        } else {
            return Err(JsSourceProfileError::UnsupportedSuffix);
        };
        Ok(profile)
    }

    const fn new(grammar: JsGrammarProfile, goal: JsSourceGoal) -> Self {
        Self { grammar, goal }
    }

    /// Return the selected language grammar.
    #[must_use]
    pub const fn grammar(self) -> JsGrammarProfile {
        self.grammar
    }

    /// Return the selected source-goal policy.
    #[must_use]
    pub const fn goal(self) -> JsSourceGoal {
        self.goal
    }

    pub(crate) const fn source_type(self, selected: JsSelectedSourceGoal) -> SourceType {
        let base = match self.grammar {
            JsGrammarProfile::JavaScript => SourceType::mjs(),
            JsGrammarProfile::JavaScriptJsx => SourceType::jsx(),
            JsGrammarProfile::TypeScript => SourceType::ts(),
            JsGrammarProfile::TypeScriptJsx => SourceType::tsx(),
            JsGrammarProfile::TypeScriptDeclaration => SourceType::d_ts(),
        };
        match selected {
            JsSelectedSourceGoal::Module => base.with_module(true),
            JsSelectedSourceGoal::Script => base.with_script(true),
        }
    }
}

/// Profile-selection failure independent of operational source evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JsSourceProfileError {
    /// No exact lowercase supported suffix matched.
    UnsupportedSuffix,
}

impl fmt::Display for JsSourceProfileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("unsupported JavaScript/TypeScript source suffix")
    }
}

impl Error for JsSourceProfileError {}

#[cfg(test)]
mod tests {
    use super::{JsGrammarProfile, JsSourceGoal, JsSourceProfile};

    #[test]
    fn declaration_suffixes_win_before_their_shorter_suffixes() {
        let dts = JsSourceProfile::from_filename("types.d.ts").unwrap();
        assert_eq!(dts.grammar(), JsGrammarProfile::TypeScriptDeclaration);
        assert_eq!(dts.goal(), JsSourceGoal::BoundedUnambiguous);

        let dmts = JsSourceProfile::from_filename("types.d.mts").unwrap();
        assert_eq!(dmts.grammar(), JsGrammarProfile::TypeScriptDeclaration);
        assert_eq!(dmts.goal(), JsSourceGoal::Module);

        let dcts = JsSourceProfile::from_filename("types.d.cts").unwrap();
        assert_eq!(dcts.grammar(), JsGrammarProfile::TypeScriptDeclaration);
        assert_eq!(dcts.goal(), JsSourceGoal::CommonJs);
    }
}
