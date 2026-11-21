// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's complete in-memory invocation, which the `profile_resolve_e2e`
//! pair measures from an already acquired entry through the final outcome
//! (D15:2772).
//!
//! Before any interval, a case holds what a binding submits as the raw values
//! the pre-invocation boundaries read: the Resolver Construction Identity
//! reference, the Common Invocation Input Set with the committed ICU4X Locale
//! Canonicalization Data Artifact, and one file Entry Admission Input Set
//! (D15:2824). [`complete`] is the production path an end-to-end interval
//! runs on a constructed resolver: the pre-invocation admission of that set,
//! and Phases 0 to 15 over what it admitted, through the outcome (D15:2775).
//! A checked or a blocked outcome is an expected result; a refused set or a
//! resolver defect is a benchmark failure (D15:2826).

use std::collections::BTreeMap;
use std::sync::Arc;

use serde_json::Value;

use crate::boundary::common::Submitted;
use crate::boundary::entry::{FileValue, SubmittedFileEntry};
use crate::boundary::invocation::{
    InvocationFailures, InvocationInputs, SubmittedEntry, SubmittedInvocation,
};
use crate::boundary::selector::SelectorReport;
use crate::boundary::step::Component;
use crate::boundary::verification::VerificationAssertion;
use crate::input_limits::Bound;
use crate::locale::icu4x::Icu4xData;
use crate::locale::EnvelopeReason;
use crate::resolver::entry::fixtures::FILE;
use crate::resolver::invocation::{OutcomeStorage, Resolution, Resolver};
use crate::resolver::origin::SafeOriginKind as Kind;
use crate::resolver::profile_resolution::ResolverInvariant;

/// What a binding submits for one file entry's invocation, as the raw values
/// the pre-invocation boundaries read, each in its typed slot. The file is
/// named by [`FILE`], which its owner marks non-secret, with the revision of
/// its bytes and no locator, as the Evidence stages' file is.
pub(super) struct RawInvocation {
    /// The text of the Resolver Construction Identity the set names.
    pub(super) reference: String,
    pub(super) specification: Vec<u8>,
    pub(super) vocabulary: Vec<u8>,
    /// The canonicalization data's manifest, and its payloads by the addresses
    /// it names.
    pub(super) manifest: &'static [u8],
    pub(super) payloads: BTreeMap<String, Arc<[u8]>>,
    /// The collection's members, in submission order.
    pub(super) members: Vec<Vec<u8>>,
    pub(super) verification: Vec<u8>,
    pub(super) capability: Value,
    /// What trusted setup asserts about the Verification Input, beside the
    /// set.
    pub(super) assertion: VerificationAssertion,
    pub(super) file: Arc<[u8]>,
    /// The Source-content revision of the file's bytes.
    pub(super) revision: String,
    /// The Profile ID the selector names.
    pub(super) selector: String,
}

impl RawInvocation {
    /// The collection's members as the slot borrows them.
    pub(super) fn members(&self) -> Vec<&[u8]> {
        self.members.iter().map(Vec::as_slice).collect()
    }

    /// The set as submitted, each value beside its slot's own Safe Origin Kind,
    /// borrowing these values and `members`, which [`Self::members`] gives. The
    /// data slot's reader is the ICU4X adapter's envelope reader over the
    /// manifest and the payloads.
    pub(super) fn submitted<'a>(
        &'a self,
        members: &'a [&'a [u8]],
    ) -> SubmittedInvocation<'a, impl FnOnce(Bound, Bound) -> Result<Icu4xData, EnvelopeReason> + 'a>
    {
        let own = |slot: Kind| Some(slot.id());
        let (manifest, payloads) = (self.manifest, &self.payloads);
        let common = Submitted {
            specification: Component {
                value: Some(self.specification.as_slice()),
                origin: own(Kind::ProfileResolutionSpecificationInput),
            },
            vocabulary: Component {
                value: Some(self.vocabulary.as_slice()),
                origin: own(Kind::IntentSurfaceClassVocabulary),
            },
            data: Component {
                value: move |bytes, depth| Icu4xData::read(Some(manifest), payloads, bytes, depth),
                origin: own(Kind::LocaleCanonicalizationData),
            },
            collection: Component {
                value: Some(members),
                origin: own(Kind::ProfileResolutionArtifactCollection),
            },
            verification: Component {
                value: Some(self.verification.as_slice()),
                origin: own(Kind::ResourceLimitPolicyVerification),
            },
            capability: Component {
                value: Some(&self.capability),
                origin: own(Kind::ImplementationCapability),
            },
        };
        let file = FileValue {
            bytes: Arc::clone(&self.file),
            identity: Some(FILE),
            marked: true,
            revision: Some(&self.revision),
            locator: None,
        };
        let entry = SubmittedEntry::File(SubmittedFileEntry {
            file: Component {
                value: file,
                origin: own(Kind::ConfigurationFile),
            },
            selector: Component {
                value: SelectorReport::String(self.selector.clone()),
                origin: own(Kind::ProfileSelectorCli),
            },
        });
        SubmittedInvocation {
            construction_identity: Some(&self.reference),
            common,
            entry,
        }
    }
}

/// Why a complete invocation yields no outcome. Neither is an expected
/// result: the case's submission is one the resolver admits, and no input
/// causes a resolver defect (D15:2826). The defect is boxed, so that the
/// failure stays small.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum WorkflowFailure {
    Refused(InvocationFailures),
    Invariant(Box<ResolverInvariant>),
}

/// The complete invocation: `resolver` admits `submitted` with trusted setup's
/// `assertion`, keeps the admitted set in `slot`, and resolves it, the outcome
/// borrowing `storage`. Both are the caller's, prepared before the interval,
/// and the outcome and its Materialized Resolution Input Set are returned as
/// the resolver returns them.
pub(super) fn complete<'s, R>(
    (resolver, submitted, assertion, slot, storage): (
        &'s Resolver<'_>,
        SubmittedInvocation<'_, R>,
        &VerificationAssertion,
        &'s mut Option<InvocationInputs<Icu4xData>>,
        &'s mut OutcomeStorage<'s>,
    ),
) -> Result<Resolution<'s, Icu4xData>, WorkflowFailure>
where
    R: FnOnce(Bound, Bound) -> Result<Icu4xData, EnvelopeReason>,
{
    let inputs = resolver
        .admit(submitted, assertion)
        .map_err(WorkflowFailure::Refused)?;
    resolver
        .resolve(slot.insert(inputs), storage)
        .map_err(|invariant| WorkflowFailure::Invariant(Box::new(invariant)))
}
