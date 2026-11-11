// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The parts every one-step admission shares (015-168, 015-172): a typed input
//! slot as a binding submits it, one boundary's typed refusal, one failure of
//! a step, and the admission of a slot's origin beside its value. A step
//! admits every origin, records an origin's failure before its value's, and
//! takes its slots in the rank of the Safe Origin Kind each fixes.

use crate::artifacts::collection::CollectionFailure;
use crate::artifacts::vocabulary::EnvelopeFailure;
use crate::locale::EnvelopeReason;
use crate::resolver::origin::SafeOriginKind;

use super::capability::CapabilityRefusal;
use super::origin::{self, OriginRefusal};
use super::selector::SelectorRefusal;
use super::source::SourceRefusal;
use super::specification::SpecificationRefusal;
use super::verification::VerificationRefusal;

/// One typed input slot as a binding submits it: the raw value its boundary
/// reads, and beside it the Safe Origin Kind candidate, `None` when absent.
pub(crate) struct Component<'a, T> {
    pub(crate) value: T,
    pub(crate) origin: Option<&'a str>,
}

/// One boundary's refusal, as that boundary types it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    Origin(OriginRefusal),
    Source(SourceRefusal),
    Selector(SelectorRefusal),
    Specification(SpecificationRefusal),
    Vocabulary(EnvelopeFailure),
    Data(EnvelopeReason),
    Collection(CollectionFailure),
    Verification(VerificationRefusal),
    Capability(CapabilityRefusal),
}

impl Refusal {
    /// The boundary kind, as 015's boundary table spells it.
    pub(crate) const fn boundary_kind(self) -> &'static str {
        match self {
            Self::Origin(_) => "safe-origin-kind",
            Self::Source(_) => "file-source-identity-revision",
            Self::Selector(_) => "external-profile-selector-binding",
            Self::Specification(_) => "profile-resolution-specification-input",
            Self::Vocabulary(_) => "intent-surface-class-vocabulary-artifact",
            Self::Data(_) => "locale-canonicalization-data-artifact",
            Self::Collection(_) => "submitted-profile-resolution-artifact-collection",
            Self::Verification(_) => "resource-limit-policy-verification-input",
            Self::Capability(_) => "implementation-capability",
        }
    }

    /// The reason, as the boundary's row spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Origin(refusal) => refusal.reason(),
            Self::Source(refusal) => refusal.reason(),
            Self::Selector(refusal) => refusal.reason(),
            Self::Specification(refusal) => refusal.reason(),
            Self::Vocabulary(failure) => failure.reason(),
            Self::Data(reason) => reason.reason(),
            Self::Collection(failure) => failure.reason(),
            Self::Verification(refusal) => refusal.reason(),
            Self::Capability(refusal) => refusal.reason(),
        }
    }
}

/// One failure of the step: the slot, named by the Safe Origin Kind it fixes,
/// and the refusal of its origin or its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InputFailure {
    pub(crate) slot: SafeOriginKind,
    pub(crate) refusal: Refusal,
}

/// Admit one slot's origin candidate beside its value's result, which is
/// `None` when the value's boundary was not evaluated, and record the origin's
/// failure before the value's. The admitted value is returned whatever the
/// origin's result: any recorded failure refuses the whole set.
pub(super) fn slot<T>(
    failures: &mut Vec<InputFailure>,
    slot: SafeOriginKind,
    candidate: Option<&str>,
    value: Option<Result<T, Refusal>>,
) -> Option<T> {
    if let Err(refusal) = origin::admit(slot, candidate) {
        failures.push(InputFailure {
            slot,
            refusal: Refusal::Origin(refusal),
        });
    }
    match value? {
        Ok(value) => Some(value),
        Err(refusal) => {
            failures.push(InputFailure { slot, refusal });
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each refusal names its boundary kind and one reason of that kind's row
    /// of 015's table.
    #[test]
    fn each_refusal_names_its_boundary_and_reason() {
        let named = [
            (
                Refusal::Origin(OriginRefusal::NonCanonical),
                "safe-origin-kind",
                "non-canonical-safe-origin-kind",
            ),
            (
                Refusal::Source(SourceRefusal::UnstableIdentity),
                "file-source-identity-revision",
                "unstable-source-identity",
            ),
            (
                Refusal::Selector(SelectorRefusal::MalformedNormalized),
                "external-profile-selector-binding",
                "malformed-normalized-selector",
            ),
            (
                Refusal::Specification(SpecificationRefusal::DuplicateMember),
                "profile-resolution-specification-input",
                "duplicate-member",
            ),
            (
                Refusal::Vocabulary(EnvelopeFailure::Unsupported),
                "intent-surface-class-vocabulary-artifact",
                "malformed-envelope",
            ),
            (
                Refusal::Data(EnvelopeReason::OverBoundEnvelope),
                "locale-canonicalization-data-artifact",
                "over-bound-envelope",
            ),
            (
                Refusal::Collection(CollectionFailure::BeyondCapacity),
                "submitted-profile-resolution-artifact-collection",
                "formal-envelope-over-bound",
            ),
            (
                Refusal::Verification(VerificationRefusal::SecretBearing),
                "resource-limit-policy-verification-input",
                "secret-bearing-input",
            ),
            (
                Refusal::Capability(CapabilityRefusal::Incomplete),
                "implementation-capability",
                "incomplete-capability",
            ),
        ];
        for (refusal, kind, reason) in named {
            assert_eq!(
                (refusal.boundary_kind(), refusal.reason()),
                (kind, reason),
                "{refusal:?}"
            );
        }
    }
}
