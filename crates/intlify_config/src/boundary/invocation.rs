// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's admission of one Resolver Invocation Input Set: the Resolver
//! Construction Identity reference, the Common Invocation Input Set, and
//! exactly one file or programmatic Entry Admission Input Set (D15:201,
//! 1106).
//!
//! The three are admitted in stages of their own: the reference at its
//! boundary, the common set in its step, and the entry in the step of its
//! kind, each with that step's own order (D15:3159). Every stage is
//! evaluated whatever the others yield, and each keeps its own typed result,
//! so several invalid components give several results, never one combined
//! failure (D15:3159). The entry step reads the capability the common step's
//! capability boundary admitted, and leaves its bound-reading boundaries
//! unevaluated only when that boundary refused it, never because another
//! common component was refused (plan 025 decision #304). The Verification
//! Input assertion is the trusted setup path's parameter beside the set
//! (D15:1106). The set exists only when every stage admits its part
//! (D15:3159).

use std::sync::Arc;

use crate::construction::identity::ConstructionIdentity;
use crate::input_limits::Bound;
use crate::locale::{EnvelopeReason, FormalData, VersionedIdentity};
use crate::structural::selection::SelectorInput;

use super::common::{self, CommonInputs};
use super::construction_identity::{self, IdentityRefusal};
use super::entry::{
    self, FileEntryInputs, ProgrammaticEntryInputs, SubmittedFileEntry, SubmittedProgrammaticEntry,
};
use super::step::InputFailure;
use super::verification::VerificationAssertion;

/// The one Entry Admission Input Set an invocation submits.
pub(crate) enum SubmittedEntry<'a> {
    File(SubmittedFileEntry<'a>),
    Programmatic(SubmittedProgrammaticEntry<'a>),
}

/// A Resolver Invocation Input Set as submitted. The reference is the
/// identity's text, `None` when absent.
pub(crate) struct SubmittedInvocation<'a, R> {
    pub(crate) construction_identity: Option<&'a str>,
    pub(crate) common: common::Submitted<'a, R>,
    pub(crate) entry: SubmittedEntry<'a>,
}

/// An admitted Entry Admission Input Set, of the kind submitted.
pub(crate) enum EntryInputs {
    File(FileEntryInputs),
    Programmatic(ProgrammaticEntryInputs),
}

impl EntryInputs {
    /// The normalized External Profile Selector Input the entry submitted.
    pub(crate) const fn selector(&self) -> &SelectorInput {
        match self {
            Self::File(file) => &file.selector,
            Self::Programmatic(programmatic) => &programmatic.selector,
        }
    }
}

/// Two entries are equal when they are of one kind and equal by value. A
/// file and a programmatic entry differ at least in their fixed Safe Origin
/// Kinds (D15:2583).
impl PartialEq for EntryInputs {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::File(left), Self::File(right)) => left == right,
            (Self::Programmatic(left), Self::Programmatic(right)) => left == right,
            _ => false,
        }
    }
}

/// An admitted Resolver Invocation Input Set. No `Debug`: an entry can carry
/// configuration and an arbitrary selector. The common set is shared, so a
/// Materialized Resolution Input Set holds it without copying it and without
/// borrowing this set (D15:2749).
pub(crate) struct InvocationInputs<D> {
    pub(crate) construction_identity: ConstructionIdentity,
    pub(crate) common: Arc<CommonInputs<D>>,
    pub(crate) entry: EntryInputs,
}

/// Two sets are equal when every logical field and occurrence is: the
/// reference, the common set, and the entry, each by value (D15:2583, 2589).
impl<D: Eq> PartialEq for InvocationInputs<D> {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            construction_identity,
            common,
            entry,
        } = self;
        *construction_identity == other.construction_identity
            && **common == *other.common
            && *entry == other.entry
    }
}

/// A refused Resolver Invocation Input Set: each stage's own failures, `None`
/// when that stage admitted its part. Under a refused capability the entry's
/// list holds only its origin failures and may be empty, never the
/// capability's own failure, which the common list holds (D15:3163).
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct InvocationFailures {
    pub(crate) construction_identity: Option<IdentityRefusal>,
    pub(crate) common: Option<Vec<InputFailure>>,
    pub(crate) entry: Option<Vec<InputFailure>>,
}

/// Admit `submitted` against the resolver's Resolver Construction Identity
/// `identity` and its Capability Specification `capability`, with the trusted
/// setup path's Verification Input `assertion`: the admitted set, or every
/// stage's failures.
pub(crate) fn admit<R, D, Id>(
    submitted: SubmittedInvocation<'_, R>,
    identity: &ConstructionIdentity,
    capability: &VersionedIdentity<Id>,
    assertion: &VerificationAssertion,
) -> Result<InvocationInputs<D>, InvocationFailures>
where
    R: FnOnce(Bound, Bound) -> Result<D, EnvelopeReason>,
    D: FormalData,
    Id: AsRef<str>,
{
    let construction_identity =
        construction_identity::admit(submitted.construction_identity, identity);
    let common = common::admit(submitted.common, capability, assertion);
    // The capability's own boundary alone decides whether the entry's
    // bound-reading boundaries are evaluated (plan 025 decision #304).
    let read = match &common {
        Ok(set) => Some(&set.capability),
        Err(refused) => refused.capability.as_deref(),
    };
    let entry = match submitted.entry {
        SubmittedEntry::File(file) => entry::admit(file, read).map(EntryInputs::File),
        SubmittedEntry::Programmatic(programmatic) => {
            entry::admit_programmatic(programmatic, read).map(EntryInputs::Programmatic)
        }
    };
    match (construction_identity, common, entry) {
        (Ok(construction_identity), Ok(common), Ok(entry)) => Ok(InvocationInputs {
            construction_identity,
            common: Arc::new(common),
            entry,
        }),
        (construction_identity, common, entry) => Err(InvocationFailures {
            construction_identity: construction_identity.err(),
            common: common.err().map(|refused| refused.failures),
            entry: entry.err(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::artifacts::vocabulary::EnvelopeFailure;
    use crate::base64url::{encode, CONSTRUCTION_IDENTITY_PREFIX};
    use crate::boundary::capability::CapabilityRefusal;
    use crate::boundary::common::tests::{constructed, read, Envelope, Inputs};
    use crate::boundary::entry::tests::{
        submitted as file_entry, widen_entry_bounds, Programmatic, BYTES, ENTRY,
        IDENTITY as FILE_IDENTITY,
    };
    use crate::boundary::origin::OriginRefusal;
    use crate::boundary::selector::{SelectorRefusal, SelectorReport};
    use crate::boundary::source::{file_source_revision, SourceRefusal};
    use crate::boundary::step::Refusal;
    use crate::resolver::origin::SafeOriginKind as Kind;
    use crate::resolver::outcome::tests::identity;

    /// The envelope reader of every common set here.
    type Data = fn(Bound, Bound) -> Result<Envelope, EnvelopeReason>;

    /// Common inputs every common boundary admits, under a capability whose
    /// bounds every entry here is also within.
    fn inputs() -> Inputs {
        let mut inputs = Inputs::new();
        widen_entry_bounds(&mut inputs.capability);
        inputs
    }

    /// Admit the common inputs as submitted after `edit`, beside `reference`
    /// and `entry`, against the committed construction.
    fn admit_with<'a>(
        inputs: &'a Inputs,
        reference: Option<&'a str>,
        edit: impl FnOnce(&mut common::Submitted<'a, Data>),
        entry: SubmittedEntry<'a>,
    ) -> Result<InvocationInputs<Envelope>, InvocationFailures> {
        let mut common = inputs.submitted(read as Data);
        edit(&mut common);
        let submitted = SubmittedInvocation {
            construction_identity: reference,
            common,
            entry,
        };
        admit(submitted, &identity(), &constructed(), &inputs.assertion)
    }

    fn refused<'a>(
        inputs: &'a Inputs,
        reference: Option<&'a str>,
        edit: impl FnOnce(&mut common::Submitted<'a, Data>),
        entry: SubmittedEntry<'a>,
    ) -> InvocationFailures {
        admit_with(inputs, reference, edit, entry)
            .err()
            .expect("the invocation is refused")
    }

    fn failure(slot: Kind, refusal: Refusal) -> InputFailure {
        InputFailure { slot, refusal }
    }

    fn failures(
        construction_identity: Option<IdentityRefusal>,
        common: Option<Vec<InputFailure>>,
        entry: Option<Vec<InputFailure>>,
    ) -> InvocationFailures {
        InvocationFailures {
            construction_identity,
            common,
            entry,
        }
    }

    /// The admitted set carries the constructed identity, the common set its
    /// step admits on its own, and the file entry as submitted.
    #[test]
    fn a_valid_file_invocation_is_admitted_with_each_value() {
        let inputs = inputs();
        let reference = identity().text();
        let revision = file_source_revision(BYTES);
        let entry = SubmittedEntry::File(file_entry(&revision, "app"));
        let Ok(admitted) = admit_with(&inputs, Some(&reference), |_| {}, entry) else {
            panic!("the invocation is admitted");
        };
        assert_eq!(admitted.construction_identity, identity());
        let alone = inputs.admit(inputs.submitted(read)).unwrap();
        let common = &admitted.common;
        assert_eq!(common.specification, alone.specification);
        assert_eq!(common.vocabulary, alone.vocabulary);
        assert_eq!(common.data, alone.data);
        assert_eq!(common.collection, alone.collection);
        assert_eq!(common.verification, alone.verification);
        assert_eq!(common.capability, alone.capability);
        let EntryInputs::File(file) = admitted.entry else {
            panic!("a file entry is admitted as one");
        };
        assert_eq!(&*file.bytes, BYTES);
        assert_eq!(file.source.identity().as_str(), FILE_IDENTITY);
        assert_eq!(file.source.revision(), revision);
    }

    #[test]
    fn a_valid_programmatic_invocation_is_admitted_as_a_programmatic_entry() {
        let inputs = inputs();
        let reference = identity().text();
        let programmatic = Programmatic::new();
        let entry = SubmittedEntry::Programmatic(programmatic.entry("app"));
        let Ok(admitted) = admit_with(&inputs, Some(&reference), |_| {}, entry) else {
            panic!("the invocation is admitted");
        };
        assert_eq!(admitted.construction_identity, identity());
        let EntryInputs::Programmatic(entry) = admitted.entry else {
            panic!("a programmatic entry is admitted as one");
        };
        assert_eq!(entry.snapshot.digest(), programmatic.digest);
        assert_eq!(entry.source.identity().as_str(), ENTRY);
        assert!(entry.call_site.is_some());
    }

    /// A reference other than the constructed identity is the one failure of
    /// an invocation whose other stages admit their parts.
    #[test]
    fn a_wrong_reference_alone_gives_only_its_refusal() {
        let inputs = inputs();
        let revision = file_source_revision(BYTES);
        let mut bytes = *identity().bytes();
        bytes[0] ^= 0x01;
        let other = format!("{CONSTRUCTION_IDENTITY_PREFIX}{}", encode(&bytes));
        for (reference, refusal) in [
            (None, IdentityRefusal::Missing),
            (
                Some(CONSTRUCTION_IDENTITY_PREFIX),
                IdentityRefusal::Malformed,
            ),
            (Some(other.as_str()), IdentityRefusal::Mismatch),
        ] {
            let entry = SubmittedEntry::File(file_entry(&revision, "app"));
            assert_eq!(
                refused(&inputs, reference, |_| {}, entry),
                failures(Some(refusal), None, None),
                "{refusal:?}"
            );
        }
    }

    /// Each stage is evaluated whatever the others yield, and keeps its own
    /// failures.
    #[test]
    fn every_stage_keeps_its_own_failures() {
        let inputs = inputs();
        let revision = file_source_revision(BYTES);
        let mut entry = file_entry(&revision, "app");
        entry.selector.value = SelectorReport::Unsafe;
        assert_eq!(
            refused(
                &inputs,
                None,
                |common| common.vocabulary.value = None,
                SubmittedEntry::File(entry),
            ),
            failures(
                Some(IdentityRefusal::Missing),
                Some(vec![failure(
                    Kind::IntentSurfaceClassVocabulary,
                    Refusal::Vocabulary(EnvelopeFailure::Missing)
                )]),
                Some(vec![failure(
                    Kind::ProfileSelectorCli,
                    Refusal::Selector(SelectorRefusal::UnsafeHostInspection)
                )]),
            )
        );
    }

    /// Another refused common component, or the capability's own refused
    /// origin, leaves the capability admitted, so the entry's bound-reading
    /// boundaries are evaluated: an admitted entry has no failures, and a
    /// refused one its own (plan 025 decision #304).
    #[test]
    fn a_refused_component_leaves_the_entry_read_under_the_admitted_capability() {
        let inputs = inputs();
        let reference = identity().text();
        let revision = file_source_revision(BYTES);
        let programmatic = Programmatic::new();
        let vocabulary = vec![failure(
            Kind::IntentSurfaceClassVocabulary,
            Refusal::Vocabulary(EnvelopeFailure::Missing),
        )];
        let entry = SubmittedEntry::File(file_entry(&revision, "app"));
        assert_eq!(
            refused(
                &inputs,
                Some(&reference),
                |common| common.vocabulary.value = None,
                entry
            ),
            failures(None, Some(vocabulary.clone()), None)
        );
        let mut entry = file_entry(&revision, "app");
        entry.file.value.identity = None;
        assert_eq!(
            refused(
                &inputs,
                Some(&reference),
                |common| common.vocabulary.value = None,
                SubmittedEntry::File(entry),
            ),
            failures(
                None,
                Some(vocabulary),
                Some(vec![failure(
                    Kind::ConfigurationFile,
                    Refusal::Source(SourceRefusal::MissingIdentity)
                )]),
            )
        );
        let mut entry = programmatic.entry("app");
        entry.programmatic.value.identity = None;
        assert_eq!(
            refused(
                &inputs,
                Some(&reference),
                |common| common.capability.origin = Some("configuration-file"),
                SubmittedEntry::Programmatic(entry),
            ),
            failures(
                None,
                Some(vec![failure(
                    Kind::ImplementationCapability,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                )]),
                Some(vec![failure(
                    Kind::ConfigurationProgrammatic,
                    Refusal::EntryIdentity(SourceRefusal::MissingIdentity)
                )]),
            )
        );
    }

    /// A refused capability leaves the entry only its origin failures, none
    /// when its origins are valid, and its own failure in the common list.
    #[test]
    fn a_refused_capability_leaves_the_entry_only_its_origins() {
        let inputs = inputs();
        let reference = identity().text();
        let revision = file_source_revision(BYTES);
        let programmatic = Programmatic::new();
        let capability = vec![failure(
            Kind::ImplementationCapability,
            Refusal::Capability(CapabilityRefusal::Missing),
        )];
        let mut entry = file_entry(&revision, "app");
        entry.file.value.identity = None;
        entry.selector.origin = Some(" Profile-Selector-CLI");
        assert_eq!(
            refused(
                &inputs,
                Some(&reference),
                |common| common.capability.value = None,
                SubmittedEntry::File(entry),
            ),
            failures(
                None,
                Some(capability.clone()),
                Some(vec![failure(
                    Kind::ProfileSelectorCli,
                    Refusal::Origin(OriginRefusal::NonCanonical)
                )]),
            )
        );
        let entry = SubmittedEntry::Programmatic(programmatic.entry("app"));
        assert_eq!(
            refused(
                &inputs,
                Some(&reference),
                |common| common.capability.value = None,
                entry
            ),
            failures(None, Some(capability), Some(Vec::new()))
        );
    }
}
