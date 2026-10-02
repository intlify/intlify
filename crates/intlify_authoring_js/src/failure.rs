// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Why an invocation could not run.
//!
//! Every variant is an operational failure: the invocation returns no result,
//! and nothing is reported to an author as a diagnostic. The caller assembled
//! the invocation wrongly, a bound the caller chose was reached, the host
//! parser failed itself, or the caller asked to stop. What an author can fix,
//! such as a unit that is not text or that the host rejects, is reported as a
//! failed unit instead, and the other units are still read.

use intlify_authoring::{AuthoringFailure, ContextKind, SnapshotMismatch, Token};

use crate::limits::JsLimitKind;

/// Complete failure of one Producer invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProducerFailure {
    /// The context claims a production kind this phase does not admit.
    ///
    /// Production admission needs checked 015 inputs and the 017/018 work
    /// later phases own, as it does in `intlify_authoring`.
    ProductionContextUnsupported(ContextKind),
    /// The context pins an authoring profile this Producer does not implement.
    ProfileMismatch,
    /// The context's usage profile is not the one this Producer assigns from.
    ///
    /// A profile that admits a DOM global needs the context to register this
    /// Producer's usage profile, because automatically recognized text
    /// carries a usage from it. A context registering another usage profile
    /// is refused whatever the profile admits.
    UsageProfileMismatch,
    /// A named bound was exhausted.
    Limit(JsLimitKind),
    /// Two membership entries name one unit.
    DuplicateMember {
        /// The repeated unit.
        unit: Token,
    },
    /// Two supplied units name one unit.
    DuplicateUnit {
        /// The repeated unit.
        unit: Token,
    },
    /// A supplied unit belongs to a different owner than the context.
    ForeignOwner {
        /// The foreign unit.
        unit: Token,
    },
    /// A supplied unit names a grammar outside the registry.
    UnregisteredGrammar {
        /// The unit naming it.
        unit: Token,
    },
    /// A supplied unit is not a member of the declared scope at its revision.
    NotAMember {
        /// The unit outside the scope.
        unit: Token,
    },
    /// A complete scope was declared without the bytes of one of its members.
    ///
    /// Reading the rest and calling the result complete would claim the
    /// absence of declarations nobody looked for.
    MissingMember {
        /// The member whose bytes are missing.
        unit: Token,
    },
    /// Supplied bytes are not the ones a unit's snapshot names.
    Snapshot {
        /// The unit whose bytes do not match.
        unit: Token,
        /// Whether the length or the digest disagreed.
        mismatch: SnapshotMismatch,
    },
    /// The host parser stopped without saying why.
    ///
    /// A parser that gives up has to name what it rejected. One that does not
    /// has failed itself, so the unit is not reported as the author's mistake.
    ParserInvariant {
        /// The unit being read.
        unit: Token,
    },
    /// This crate's literal decoder and the host parser read one literal
    /// differently.
    ///
    /// The two are written independently so that a disagreement exposes a
    /// defect in one of them. Neither reading is trusted over the other, so
    /// the unit yields nothing rather than a message from either.
    DecoderDisagreement {
        /// The unit holding the literal.
        unit: Token,
    },
    /// Two declarations claim overlapping source.
    ///
    /// Each piece of source is read as at most one declaration, so an overlap
    /// means the recognizer extracted something twice.
    DeclarationOverlap {
        /// The unit holding the declarations.
        unit: Token,
    },
    /// The shared authoring semantics could not run.
    Authoring(AuthoringFailure),
    /// The caller's probe asked the invocation to stop.
    ///
    /// Cancellation is a control-flow result, never evidence: a cancelled
    /// invocation establishes nothing about the units it did reach.
    Cancelled,
}

impl From<AuthoringFailure> for ProducerFailure {
    fn from(failure: AuthoringFailure) -> Self {
        match failure {
            // A stop is a stop whichever crate noticed it.
            AuthoringFailure::Cancelled => Self::Cancelled,
            other => Self::Authoring(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use intlify_authoring::LimitKind;

    use super::*;

    #[test]
    fn a_shared_failure_keeps_its_cause_and_a_stop_stays_a_stop() {
        assert_eq!(
            ProducerFailure::from(AuthoringFailure::Limit(LimitKind::Declarations)),
            ProducerFailure::Authoring(AuthoringFailure::Limit(LimitKind::Declarations))
        );
        assert_eq!(
            ProducerFailure::from(AuthoringFailure::LocaleProviderUnavailable),
            ProducerFailure::Authoring(AuthoringFailure::LocaleProviderUnavailable)
        );
        // Whichever crate noticed the probe, the caller sees one kind of stop.
        assert_eq!(
            ProducerFailure::from(AuthoringFailure::Cancelled),
            ProducerFailure::Cancelled
        );
    }
}
