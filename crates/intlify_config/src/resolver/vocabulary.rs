// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The surface classes of the Intent Surface-Class Vocabulary Artifact that
//! 015's Phase 5 admits against the `intent-surface-class-vocabulary` slot:
//! what coverage selectors may name and what the coverage decision table
//! spans. Not the artifact, its reader, or its admission; until those exist,
//! only tests build a vocabulary.

use intlify_shared_json::token::Token;

/// An admitted vocabulary's members: `IdentityToken`s, unique and in
/// ascending unsigned UTF-8 byte order. The set may be empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SurfaceVocabulary(Vec<Token>);

impl SurfaceVocabulary {
    pub(crate) fn members(&self) -> &[Token] {
        &self.0
    }

    /// The member `spelling` names, when it names one exactly.
    pub(crate) fn member(&self, spelling: &str) -> Option<&Token> {
        self.0
            .binary_search_by(|member| member.as_str().cmp(spelling))
            .ok()
            .map(|index| &self.0[index])
    }

    /// The vocabulary of `members`, given in any order and each once, standing
    /// in for one Phase 5 admitted.
    #[cfg(test)]
    pub(crate) fn fixture_owned(members: &[&str]) -> Self {
        let mut members: Vec<Token> = members
            .iter()
            .map(|member| Token::new(member).expect("a fixture member is an IdentityToken"))
            .collect();
        members.sort_unstable();
        assert!(
            members.windows(2).all(|pair| pair[0] < pair[1]),
            "a fixture names each member once"
        );
        Self(members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_are_sorted_by_their_bytes() {
        let vocabulary = SurfaceVocabulary::fixture_owned(&["nav", "checkout", "internal-tool"]);
        assert_eq!(
            vocabulary
                .members()
                .iter()
                .map(Token::as_str)
                .collect::<Vec<_>>(),
            ["checkout", "internal-tool", "nav"]
        );
        assert!(SurfaceVocabulary::fixture_owned(&[]).members().is_empty());
    }

    #[test]
    fn a_member_is_found_only_by_its_exact_spelling() {
        let vocabulary = SurfaceVocabulary::fixture_owned(&["checkout", "nav"]);
        assert_eq!(vocabulary.member("nav").map(Token::as_str), Some("nav"));
        for other in ["Nav", "na", "nav ", "help", ""] {
            assert_eq!(vocabulary.member(other), None);
        }
    }
}
