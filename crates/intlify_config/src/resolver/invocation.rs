// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's invocation of a constructed resolver: one admitted Resolver
//! Invocation Input Set read as Phases 0 to 15 take it, and the one outcome
//! they return (D15:1106, 1150, 2129-2147).
//!
//! [`Resolver`] is a constructed resolver with its Resolver Construction
//! Identity, derived once after construction as a step of its own rather
//! than at every invocation (D15:2775), and with what it supports in each
//! specification slot. The pre-invocation boundary admits a set against that
//! identity and the Capability Specification construction admitted
//! ([`Resolver::admit`]), and [`Resolver::resolve`] reads nothing else of the
//! invocation and nothing from a fixture:
//!
//! - every bound is the admitted capability's: strict file entry's raw bytes
//!   and parser tokens are `bootstrapOnly.rawInput`'s, a Snapshot's are
//!   `bootstrapOnly.snapshot`'s, and the five root-value bounds and the
//!   structural ones are `policyComparable.configurationInput`'s (D15:2129,
//!   2130, 2675);
//! - a member on a fact's internal path is classified at its place in the
//!   resolver's configuration schema under the capability's `policyComparable`
//!   byte bounds, never a later Policy's, since a key's class is decided only
//!   from facts fixed when its record is constructed (D15:776);
//! - the entry's Evidence names the source the entry was admitted under, its
//!   identity marked non-secret only as its adapter or frontend marked it,
//!   with only that identity's admitted locator, and a programmatic entry's
//!   the call-site sidecar admitted beside it (D15:737, 800, 840);
//! - the outcome carries the admitted reference, which names the resolver's
//!   own identity; a set admitted against another identity is not resolved,
//!   since no outcome could carry an identity equal to both (D15:1150, 2147).
//!
//! What the outcome borrows, the evaluation, the Artifact Set Phase 9
//! admitted, and what the phases left, its caller holds in an
//! [`OutcomeStorage`] of its own for each invocation. The resolver itself
//! changes nothing, so nothing of one invocation, checked or blocked, reaches
//! the next, and no outcome refers to storage that a later invocation resets
//! (D15:2749, 2751, 3689).
//!
//! Beside the outcome of every invocation whose Phase 1 materialized a value,
//! never on request, [`Resolver::resolve`] returns the invocation's
//! Materialized Resolution Input Set, and none when Phase 1 blocked before a
//! value existed (D15:2657, 3440). The set owns its value and shares the
//! admitted common values, so it outlives the [`OutcomeStorage`] and the
//! admitted set it came from (D15:2749).

use std::sync::Arc;

use crate::boundary::common::CommonInputs;
use crate::boundary::invocation::{
    self as boundary, EntryInputs, InvocationFailures, InvocationInputs, SubmittedInvocation,
};
use crate::boundary::verification::VerificationAssertion;
use crate::construction::identity::ConstructionIdentity;
use crate::construction::resolver::ConstructedResolver;
use crate::construction::root::ConstructionRoot;
use crate::input_limits::{Bound, InputLimits};
use crate::locale::{EnvelopeReason, FormalData, Provider};

use super::artifact_resolution::ArtifactSet;
use super::entry::{Classify, EntryFile};
use super::evaluation::{Admitted, Evaluation};
use super::evidence::Source;
use super::evidence_path::KeyFacts;
use super::materialized::MaterializedInputs;
use super::outcome::{outcome, Outcome};
use super::profile_resolution::{
    invoke_file, invoke_programmatic, ConfigurationSchema, Invocation, Invoked, ResolverInvariant,
};
use super::snapshot_entry::{EntrySnapshot, SnapshotEntryLimits};
use super::specification::SupportedSpecifications;
use super::token::Sha256;

/// A constructed resolver, its Resolver Construction Identity, and what it
/// supports in each specification slot, each derived once.
pub(crate) struct Resolver<'r> {
    constructed: &'r ConstructedResolver,
    identity: ConstructionIdentity,
    supported: SupportedSpecifications<String>,
}

/// What one invocation's outcome borrows: the evaluation its phases
/// recorded, the Profile Resolution Artifact Set Phase 9 admitted, and what
/// the phases left. Its caller holds one for each invocation, and the outcome
/// borrows it for as long as the outcome lives (D15:2749).
#[derive(Default)]
pub(crate) struct OutcomeStorage<'s> {
    evaluation: Evaluation,
    set: Option<Admitted<ArtifactSet<'s>>>,
    invoked: Option<Invoked<'s, String>>,
}

/// What one invocation returns: its outcome, and beside it the Materialized
/// Resolution Input Set, `None` when Phase 1 blocked before a value existed
/// (D15:2657).
pub(crate) struct Resolution<'s, D> {
    pub(crate) outcome: Outcome<'s, 's>,
    pub(crate) materialized: Option<MaterializedInputs<D>>,
}

impl<'r> Resolver<'r> {
    /// `constructed`, which construction built under `root`.
    pub(crate) fn new(constructed: &'r ConstructedResolver, root: &ConstructionRoot) -> Self {
        Self {
            identity: constructed.identity(root),
            supported: constructed.supported(),
            constructed,
        }
    }

    /// The resolver's Resolver Construction Identity.
    pub(crate) const fn identity(&self) -> &ConstructionIdentity {
        &self.identity
    }

    /// Admit `submitted` against the resolver's identity and the Capability
    /// Specification construction admitted, with the trusted setup path's
    /// Verification Input `assertion` (D15:1106).
    pub(crate) fn admit<R, D>(
        &self,
        submitted: SubmittedInvocation<'_, R>,
        assertion: &VerificationAssertion,
    ) -> Result<InvocationInputs<D>, InvocationFailures>
    where
        R: FnOnce(Bound, Bound) -> Result<D, EnvelopeReason>,
        D: FormalData,
    {
        let capability = self.constructed.capability_specification();
        boundary::admit(submitted, &self.identity, &capability, assertion)
    }

    /// Phases 0 to 15 over `inputs`, the outcome borrowing `storage`, and
    /// beside it the Materialized Resolution Input Set once Phase 1
    /// materialized a value.
    ///
    /// # Errors
    ///
    /// Returns [`ResolverInvariant::OtherConstruction`] for a set admitted
    /// against another identity, and otherwise the resolver bug a phase found.
    pub(crate) fn resolve<'s, D, P>(
        &'s self,
        inputs: &'s InvocationInputs<D>,
        storage: &'s mut OutcomeStorage<'s>,
    ) -> Result<Resolution<'s, D>, ResolverInvariant>
    where
        D: FormalData<Provider = P>,
        P: Provider<Identity = String>,
    {
        if inputs.construction_identity != self.identity {
            return Err(ResolverInvariant::OtherConstruction);
        }
        let common = &inputs.common;
        let capability = &common.capability;
        let invocation = self.invocation(common);
        let comparable = capability.policy_comparable();
        let bootstrap = capability.bootstrap_only();
        let configuration = &comparable.configuration_input;
        let schema = self.configuration_schema();
        let classify: &Classify<'_> = &|edges, locales| {
            KeyFacts {
                comparable,
                locales,
            }
            .steps(schema, edges)
        };
        let OutcomeStorage {
            evaluation,
            set,
            invoked,
        } = storage;
        let selector = inputs.entry.selector();
        let mut value = None;
        let left = match &inputs.entry {
            EntryInputs::File(file) => {
                let entry = EntryFile {
                    bytes: Arc::clone(&file.bytes),
                    source: Source {
                        identity: file.source.identity(),
                        marked: file.marked,
                        locator: file.locator.as_ref(),
                    },
                    classify,
                };
                let limits = InputLimits {
                    raw: bootstrap.raw_input,
                    value: configuration.value,
                };
                let structure = (schema, configuration.structural, selector);
                invoke_file(
                    evaluation,
                    (&entry, limits),
                    structure,
                    &invocation,
                    &common.data,
                    (set, &mut value),
                )?
            }
            EntryInputs::Programmatic(programmatic) => {
                let entry = EntrySnapshot {
                    snapshot: &programmatic.snapshot,
                    source: Source {
                        identity: programmatic.source.identity(),
                        marked: programmatic.marked,
                        locator: programmatic.locator.as_ref(),
                    },
                    call_site: programmatic.call_site.as_ref(),
                    classify,
                };
                let limits = SnapshotEntryLimits {
                    snapshot: bootstrap.snapshot,
                    value: configuration.value,
                };
                let structure = (schema, configuration.structural, selector);
                invoke_programmatic(
                    evaluation,
                    (&entry, limits),
                    structure,
                    &invocation,
                    &common.data,
                    (set, &mut value),
                )?
            }
        };
        let invoked = invoked.insert(left);
        let outcome = outcome(
            evaluation,
            invoked,
            inputs.construction_identity,
            &invocation,
            &Sha256,
        )?;
        let materialized = value.map(|value| {
            let common = Arc::clone(&inputs.common);
            MaterializedInputs::new(
                inputs.construction_identity,
                value,
                selector.clone(),
                common,
            )
        });
        Ok(Resolution {
            outcome,
            materialized,
        })
    }

    /// What every phase after configuration entry reads besides the
    /// configuration: what the resolver supports, and the six values of
    /// `common`, an admitted set's or a retained one's.
    pub(super) fn invocation<'a, D>(
        &'a self,
        common: &'a CommonInputs<D>,
    ) -> Invocation<'a, String> {
        Invocation {
            supported: &self.supported,
            specifications: &common.specification,
            capability: &common.capability,
            verification: &common.verification,
            collection: &common.collection,
            vocabulary: &common.vocabulary,
        }
    }

    /// The configuration schema construction admitted, which structural
    /// admission reads and a member's class is decided at (D15:776).
    pub(super) fn configuration_schema(&self) -> &ConfigurationSchema {
        self.constructed.configuration_schema()
    }
}

// `materialized`'s tests resolve the same inputs.
#[cfg(test)]
pub(crate) mod tests {
    use std::collections::BTreeSet;

    use intlify_shared_json::encoding::encode;
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::collection::{CollectionFailure, CollectionReport};
    use crate::artifacts::envelope::{envelope_of, ArtifactKind, ExactTuple, FormalMember};
    use crate::artifacts::vectors::vocabulary as vectors;
    use crate::artifacts::verification::{sealed, unsealed, VerificationInputArtifact};
    use crate::artifacts::vocabulary::IntentSurfaceClassVocabularyArtifact;
    use crate::boundary::call_site::{CallSiteReport, SubmittedCallSite};
    use crate::boundary::common::Submitted;
    use crate::boundary::entry::{
        FileValue, ProgrammaticValue, SubmittedFileEntry, SubmittedProgrammaticEntry,
    };
    use crate::boundary::invocation::{InvocationFailures, SubmittedEntry};
    use crate::boundary::locator::{LocatorCandidate, LocatorVariant};
    use crate::boundary::origin::OriginRefusal;
    use crate::boundary::selector::SelectorReport;
    use crate::boundary::snapshot::{submitted, SnapshotReport};
    use crate::boundary::source::{call_site_source_revision, file_source_revision};
    use crate::boundary::step::{Component, InputFailure, Refusal};
    use crate::construction::flow::construct;
    use crate::construction::identity::derive;
    use crate::construction::registry::check::RecordKind;
    use crate::construction::resolver::{build, committed};
    use crate::locale::fixtures::FixtureProvider;
    use crate::locale::{AdmissionFacts, ProviderBinding, ProviderFailure, VersionedIdentity};
    use crate::resolver::call_site_span::fixtures::{mirrored, CALL_SITE};
    use crate::resolver::entry::fixtures::FILE;
    use crate::resolver::evaluation::Check;
    use crate::resolver::evidence_source::hex;
    use crate::resolver::limits::{BoundId, Group};
    use crate::resolver::origin::SafeOriginKind as Kind;
    use crate::resolver::outcome::tests::{
        blocked, entry, entry_views, no_json, reporting, shared, shown, Shown,
    };
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::snapshot_entry::fixtures::ENTRY;
    use crate::resolver::token::Digest;
    use crate::snapshot::{from_json, Marker, MarkerReason, Node, SnapshotBound};

    /// A bound no invocation here reaches.
    pub(crate) const ROOM: u64 = 1 << 30;

    /// What every secret canary of D15:3393's sweep starts with: it has no
    /// identifier, token, or locale syntax, so no field can admit it.
    const CANARY: &str = "T3 CANARY";

    /// The finite fixture's rows, bound under the pins the constructed
    /// resolver supports: CLDR 48.2.0's dataset at the semantic digest
    /// construction admitted. A test fixture, NOT that dataset: it answers
    /// only the fixture's rows, which the examples here spell, so the
    /// constructed resolver's Phase 5 binds it without the ICU4X artifact.
    #[derive(Clone, PartialEq, Eq)]
    pub(crate) struct PinnedFixture {
        rows: FixtureProvider,
        pub(crate) binding: ProviderBinding<String>,
    }

    impl PinnedFixture {
        fn new() -> Self {
            let supported = committed().locale_canonicalization();
            let named = |identity: &str| VersionedIdentity {
                identity: identity.to_owned(),
                revision: "0".to_owned(),
            };
            let asserted = named("intlify-locale-canonicalization");
            let binding = ProviderBinding {
                pins: supported.expected(&asserted).unwrap().clone(),
                admission: AdmissionFacts {
                    provider: named("test-only-pinned-fixture-provider"),
                    provider_schema: named("test-only-rust-fixture-table"),
                    artifact: "test-only-in-memory-representation-pin".to_owned(),
                },
            };
            Self {
                rows: FixtureProvider::new(),
                binding,
            }
        }
    }

    impl Provider for PinnedFixture {
        type Identity = String;

        fn binding(&self) -> &ProviderBinding<String> {
            &self.binding
        }

        fn canonicalize(&self, input: &str) -> Result<Arc<str>, ProviderFailure> {
            // The finite rows do not list a secret canary; a real provider
            // answers that it is no locale.
            if input.starts_with(CANARY) {
                return Err(ProviderFailure::InvalidIdentifier);
            }
            self.rows.canonicalize(input)
        }
    }

    /// The committed resolver.
    pub(crate) fn resolver() -> Resolver<'static> {
        Resolver::new(committed(), ConstructionRoot::built_in().unwrap())
    }

    /// The Profile Resolution Specification Input asserting what the
    /// constructed resolver supports, and the fixture vocabulary.
    fn specification() -> Vec<u8> {
        let slot = |identity: &str| json!({ "identity": identity, "revision": "0" });
        serde_json::to_vec(&json!({
            "project-profile": slot("intlify-project-profile"),
            "locale-canonicalization": slot("intlify-locale-canonicalization"),
            "locale-negotiation": slot("intlify-locale-negotiation"),
            "message-locale-fallback": slot("intlify-message-locale-fallback"),
            "coverage": slot("intlify-localization-coverage"),
            "intent-surface-class-vocabulary": {
                "identity": vectors::IDENTITY,
                "revision": vectors::REVISION,
                "semanticDigest": vectors::SEMANTIC_DIGEST,
            },
            "delivery-graph": slot("intlify-delivery-graph"),
            "delivery-placement": slot("intlify-delivery-placement"),
        }))
        .unwrap()
    }

    /// 018's Verification Input `input`, sealed, and the assertion trusted
    /// setup pins it with.
    pub(crate) fn seal(input: Value) -> (Vec<u8>, VerificationAssertion) {
        let verification = sealed(input).unwrap();
        let read: VerificationInputArtifact = serde_json::from_value(verification.clone()).unwrap();
        let assertion = VerificationAssertion {
            identity: read.identity().clone(),
            revision: read.revision().clone(),
            semantic_digest: read.semantic_digest().unwrap(),
        };
        (serde_json::to_vec(&verification).unwrap(), assertion)
    }

    /// The reference a Verification Input pins `tuple` with.
    fn pin(tuple: &ExactTuple) -> Value {
        json!({
            "kind": tuple.kind.spelling(),
            "identity": tuple.identity.as_str(),
            "revision": tuple.revision.as_str(),
            "specificationRevision": tuple.specification_revision.as_str(),
            "semanticDigest": tuple.semantic_digest.as_str(),
        })
    }

    const fn candidate(variant: LocatorVariant, payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant,
            payload,
            non_secret: true,
        }
    }

    /// The file entries' locator, and the Programmatic Entries'.
    const FILE_LOCATOR: &str = "storefront/intlify.config.json";
    const ENTRY_LOCATOR: &str = "storefront-app";

    /// The source identities and locator candidates an entry submits, and
    /// whether the call-site source's frontend marks its identity non-secret.
    #[derive(Clone, Copy)]
    struct Sources {
        file: &'static str,
        file_locator: LocatorCandidate<'static>,
        entry: &'static str,
        entry_locator: LocatorCandidate<'static>,
        call_site: &'static str,
        call_site_marked: bool,
        call_site_locator: Option<LocatorCandidate<'static>>,
    }

    impl Sources {
        /// The identities and locators of the fixtures, each marked and
        /// classified non-secret.
        const fn fixture() -> Self {
            Self {
                file: FILE,
                file_locator: candidate(LocatorVariant::RelativePath, FILE_LOCATOR),
                entry: ENTRY,
                entry_locator: candidate(LocatorVariant::SafeLabel, ENTRY_LOCATOR),
                call_site: CALL_SITE,
                call_site_marked: true,
                call_site_locator: None,
            }
        }
    }

    /// What a binding submits for one of `Inputs`' invocations, as the raw
    /// values the boundaries read: the capability's every `rawInput`,
    /// `snapshot`, `inputBoundary`, and `evidenceMetadata` bound at [`ROOM`]
    /// unless a test sets it.
    pub(crate) struct Raw {
        pub(crate) config: Value,
        pub(crate) selected: String,
        pub(crate) specification: Vec<u8>,
        pub(crate) vocabulary: Vec<u8>,
        pub(crate) provider: PinnedFixture,
        pub(crate) members: Vec<Vec<u8>>,
        pub(crate) verification: Vec<u8>,
        pub(crate) assertion: VerificationAssertion,
        capability: Value,
        sources: Sources,
        /// Every slot's Safe Origin Kind candidate in place of its own.
        origin_candidate: Option<&'static str>,
    }

    impl Raw {
        pub(crate) fn new(inputs: &Inputs) -> Self {
            let (config, selected, members, surfaces, capability, pinned) = inputs.submitted();
            let vocabulary = IntentSurfaceClassVocabularyArtifact::sealed(
                vectors::IDENTITY,
                vectors::REVISION,
                surfaces,
            )
            .unwrap();
            let (verification, assertion) = seal(unsealed("storefront-pins", "1", &[pin(pinned)]));
            let mut capability = capability.normalized();
            for group in ["rawInput", "snapshot", "inputBoundary", "evidenceMetadata"] {
                for (_, bound) in capability["bootstrapOnly"][group].as_object_mut().unwrap() {
                    *bound = json!(ROOM.to_string());
                }
            }
            Self {
                config: config.clone(),
                selected: selected.to_owned(),
                specification: specification(),
                vocabulary: serde_json::to_vec(&vocabulary).unwrap(),
                provider: PinnedFixture::new(),
                members: members
                    .iter()
                    .map(|m| serde_json::to_vec(m).unwrap())
                    .collect(),
                verification,
                assertion,
                capability,
                sources: Sources::fixture(),
                origin_candidate: None,
            }
        }

        /// The Safe Origin Kind candidate submitted beside `slot`'s value.
        fn origin(&self, slot: Kind) -> &str {
            self.origin_candidate.unwrap_or(slot.id())
        }

        /// The same, with the capability's `bootstrapOnly` bound `path` at
        /// `limit`.
        pub(crate) fn bounded(mut self, path: &str, limit: u64) -> Self {
            let pointer = format!("/bootstrapOnly/{}", path.replace('.', "/"));
            *self.capability.pointer_mut(&pointer).unwrap() = json!(limit.to_string());
            self
        }

        /// The configuration's file.
        fn bytes(&self) -> Arc<[u8]> {
            Arc::from(serde_json::to_vec(&self.config).unwrap())
        }

        fn selector(&self, slot: Kind) -> Component<'_, SelectorReport<'_>> {
            Component {
                value: SelectorReport::String(&self.selected),
                origin: Some(self.origin(slot)),
            }
        }

        /// The configuration's file, its identity marked non-secret when
        /// `marked`, admitted on `resolver`.
        pub(crate) fn file(
            &self,
            resolver: &Resolver<'_>,
            marked: bool,
        ) -> InvocationInputs<PinnedFixture> {
            self.file_of(resolver, self.bytes(), marked)
        }

        /// A file of `bytes` in place of the configuration's, otherwise as
        /// [`Self::file`].
        pub(crate) fn file_of(
            &self,
            resolver: &Resolver<'_>,
            bytes: Arc<[u8]>,
            marked: bool,
        ) -> InvocationInputs<PinnedFixture> {
            let members = self.member_bytes();
            let report = CollectionReport::Formal(&members);
            self.submit_file(resolver, bytes, marked, report).unwrap()
        }

        /// The file of `bytes` submitted beside the collection as `report`
        /// states it: the admitted set, or every failure.
        fn submit_file(
            &self,
            resolver: &Resolver<'_>,
            bytes: Arc<[u8]>,
            marked: bool,
            report: CollectionReport<'_>,
        ) -> Result<InvocationInputs<PinnedFixture>, InvocationFailures> {
            let revision = file_source_revision(&bytes);
            let file = FileValue {
                bytes,
                identity: Some(self.sources.file),
                marked,
                revision: Some(&revision),
                locator: Some(self.sources.file_locator),
            };
            self.submit(
                resolver,
                SubmittedEntry::File(SubmittedFileEntry {
                    file: Component {
                        value: file,
                        origin: Some(self.origin(Kind::ConfigurationFile)),
                    },
                    selector: self.selector(Kind::ProfileSelectorCli),
                }),
                report,
            )
        }

        /// A Programmatic Entry Snapshot of `root`, its identity marked
        /// non-secret when `marked`, beside a sidecar that mirrors the
        /// configuration's file when `mirror`, admitted on `resolver`.
        pub(crate) fn snapshot(
            &self,
            resolver: &Resolver<'_>,
            root: &Node,
            marked: bool,
            mirror: bool,
        ) -> InvocationInputs<PinnedFixture> {
            let digest = root.digest();
            let text = serde_json::to_string(&self.config).unwrap();
            let revision = call_site_source_revision(text.as_bytes());
            let call_site = mirror.then(|| {
                CallSiteReport::Formal(SubmittedCallSite {
                    identity: Some(self.sources.call_site),
                    marked: self.sources.call_site_marked,
                    revision: Some(&revision),
                    locator: self.sources.call_site_locator,
                    source: text.clone().into_bytes(),
                    mappings: mirrored(&text),
                })
            });
            let report = SnapshotReport::Formal {
                root: submitted(root),
                digest: digest.clone(),
            };
            let programmatic = ProgrammaticValue {
                snapshot: Some(&report),
                identity: Some(self.sources.entry),
                marked,
                revision: Some(&digest),
                locator: Some(self.sources.entry_locator),
                call_site,
            };
            self.admit(
                resolver,
                SubmittedEntry::Programmatic(SubmittedProgrammaticEntry {
                    programmatic: Component {
                        value: programmatic,
                        origin: Some(self.origin(Kind::ConfigurationProgrammatic)),
                    },
                    selector: self.selector(Kind::ProfileSelectorProgrammatic),
                }),
            )
        }

        /// A Snapshot of the configuration's value.
        pub(crate) fn programmatic(
            &self,
            resolver: &Resolver<'_>,
            marked: bool,
            mirror: bool,
        ) -> InvocationInputs<PinnedFixture> {
            self.snapshot(resolver, &from_json(&self.config), marked, mirror)
        }

        /// Each member's bytes, as a collection's formal report borrows them.
        fn member_bytes(&self) -> Vec<&[u8]> {
            self.members.iter().map(Vec::as_slice).collect()
        }

        /// The set of these values and `entry`, each beside its slot's own
        /// Safe Origin Kind, admitted on `resolver`.
        fn admit(
            &self,
            resolver: &Resolver<'_>,
            entry: SubmittedEntry<'_>,
        ) -> InvocationInputs<PinnedFixture> {
            let members = self.member_bytes();
            let report = CollectionReport::Formal(&members);
            self.submit(resolver, entry, report).unwrap()
        }

        /// These values and `entry`, with the collection as `report` states
        /// it, submitted on `resolver`: the admitted set, or every failure.
        fn submit(
            &self,
            resolver: &Resolver<'_>,
            entry: SubmittedEntry<'_>,
            report: CollectionReport<'_>,
        ) -> Result<InvocationInputs<PinnedFixture>, InvocationFailures> {
            let own = |slot: Kind| Some(self.origin(slot));
            let provider = self.provider.clone();
            let common = Submitted {
                specification: Component {
                    value: Some(&self.specification),
                    origin: own(Kind::ProfileResolutionSpecificationInput),
                },
                vocabulary: Component {
                    value: Some(&self.vocabulary),
                    origin: own(Kind::IntentSurfaceClassVocabulary),
                },
                data: Component {
                    value: move |_, _| Ok(provider),
                    origin: own(Kind::LocaleCanonicalizationData),
                },
                collection: Component {
                    value: Some(report),
                    origin: own(Kind::ProfileResolutionArtifactCollection),
                },
                verification: Component {
                    value: Some(&self.verification),
                    origin: own(Kind::ResourceLimitPolicyVerification),
                },
                capability: Component {
                    value: Some(&self.capability),
                    origin: own(Kind::ImplementationCapability),
                },
            };
            let reference = resolver.identity().text();
            let submitted = SubmittedInvocation {
                construction_identity: Some(&reference),
                common,
                entry,
            };
            resolver.admit(submitted, &self.assertion)
        }
    }

    /// What the outcome of `inputs` on `resolver` shows.
    pub(crate) fn resolved(
        resolver: &Resolver<'_>,
        inputs: &InvocationInputs<PinnedFixture>,
    ) -> Shown {
        let mut storage = OutcomeStorage::default();
        shown(&resolver.resolve(inputs, &mut storage).unwrap().outcome)
    }

    /// The Materialized Resolution Input Set `inputs` give on `resolver`,
    /// which outlives the storage its outcome borrowed.
    pub(crate) fn materialized(
        resolver: &Resolver<'_>,
        inputs: &InvocationInputs<PinnedFixture>,
    ) -> Option<MaterializedInputs<PinnedFixture>> {
        let mut storage = OutcomeStorage::default();
        resolver.resolve(inputs, &mut storage).unwrap().materialized
    }

    /// 015's example with `edit` applied, every input it needs.
    pub(crate) fn example(edit: impl FnOnce(Inputs) -> Inputs) -> Raw {
        Raw::new(&reporting(&edit(Inputs::complete()), &[], &[]))
    }

    /// A `policyComparable.configurationInput` bound.
    const fn input_bound(name: &'static str) -> BoundId {
        BoundId {
            group: Group::ConfigurationInput,
            name,
        }
    }

    /// The check of the one blocking Entry Finding that leaves `shown`
    /// without a Shared projection.
    fn unavailable(shown: &Shown) -> Check {
        assert_eq!(shown.shared, None);
        let [cause] = &shown.unavailable[..] else {
            panic!("{:?}", shown.unavailable);
        };
        cause.unit().check
    }

    /// The configuration Evidence in `shown`'s evidence index, which holds
    /// every Evidence its records retain.
    fn configuration_evidence(shown: &Shown) -> Vec<&Value> {
        let views = entry_views(shown, RecordKind::EvidenceIndex);
        let configuration = |view: &&Value| {
            let origin = view["origin"].as_str();
            origin.is_some_and(|origin| origin.starts_with("configuration-"))
        };
        let views: Vec<&Value> = views.into_iter().filter(configuration).collect();
        assert!(!views.is_empty());
        views
    }

    /// D15:1150, 2147: on the constructed resolver, a valid file or
    /// programmatic invocation is checked and carries the reference the
    /// boundary admitted, the resolver's own identity.
    #[test]
    fn a_valid_invocation_is_checked_under_the_admitted_reference() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        for inputs in [
            raw.file(&resolver, true),
            raw.programmatic(&resolver, true, false),
        ] {
            assert_eq!(inputs.construction_identity, *resolver.identity());
            let shown = resolved(&resolver, &inputs);
            assert_eq!(shown.identity, inputs.construction_identity);
            assert!(shown.checked.is_some());
        }
    }

    /// D15:1150: a set admitted against another resolver's identity is not
    /// resolved, since no outcome could carry an identity equal to both.
    #[test]
    fn a_set_admitted_against_another_identity_is_not_resolved() {
        let resolver = resolver();
        let mut inputs = example(|inputs| inputs).file(&resolver, true);
        let root = ConstructionRoot::built_in().unwrap();
        let mut admitted = construct(root, committed().input()).unwrap();
        admitted.registry_package.declared.revision = "1".to_owned();
        inputs.construction_identity = derive(root, &admitted);
        let mut storage = OutcomeStorage::default();
        assert_eq!(
            resolver.resolve(&inputs, &mut storage).err(),
            Some(ResolverInvariant::OtherConstruction)
        );
    }

    /// D15:2129, 2675: strict file entry reads the file under the
    /// capability's `bootstrapOnly.rawInput` bounds.
    #[test]
    fn a_file_is_read_under_the_capabilitys_raw_input_bounds() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        let limit = raw.bytes().len() as u64 - 1;
        let raw = raw.bounded("rawInput.maxFileBytes", limit);
        let shown = resolved(&resolver, &raw.file(&resolver, true));
        assert_eq!(unavailable(&shown), Check::WithinFileBytes);
    }

    /// D15:2129: either entry applies the capability's five root-value
    /// bounds, and a Snapshot is read under its `bootstrapOnly.snapshot`
    /// bounds, whose terminal marker names the bound's limit plus one.
    #[test]
    fn either_entry_is_read_under_the_capabilitys_value_and_snapshot_bounds() {
        let resolver = resolver();
        let raw = example(|inputs| inputs.bounding(&[(input_bound("maxNodes"), 10)], &[]));
        for inputs in [
            raw.file(&resolver, true),
            raw.programmatic(&resolver, true, false),
        ] {
            assert_eq!(
                unavailable(&resolved(&resolver, &inputs)),
                Check::WithinNodes
            );
        }
        let raw = example(|inputs| inputs);
        let mut root = from_json(&raw.config);
        let Node::Object(members) = &mut root else {
            panic!("the root is an object");
        };
        let witness = ROOM + 1;
        let terminal = Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::Nodes,
            witness,
        };
        // Last in canonical order, after every key the example writes.
        members.insert("~".to_owned(), Node::Marker(terminal));
        let shown = resolved(&resolver, &raw.snapshot(&resolver, &root, true, false));
        assert_eq!(unavailable(&shown), Check::WithinSnapshotNodes);
    }

    /// D15:776, 2130, 2131: structural admission and profile selection run
    /// under the capability's `configurationInput` bounds, and a key of
    /// `profiles` past its `maxProfileIdBytes` is classified as redacted.
    #[test]
    fn structural_bounds_and_key_classes_are_the_capabilitys() {
        let resolver = resolver();
        let units = input_bound("maxStructuralAnalysisUnits");
        let raw = example(|inputs| inputs.bounding(&[(units, 10)], &[]));
        for inputs in [
            raw.file(&resolver, true),
            raw.programmatic(&resolver, true, false),
        ] {
            let shown = resolved(&resolver, &inputs);
            let (shared, _) = shown.shared.as_ref().unwrap();
            assert!(shared.iter().any(|(_, kind, view)| {
                *kind == RecordKind::SharedFinding
                    && view["key"]["check"] == "structural.analysis.within-capability"
            }));
        }
        let bytes = input_bound("maxProfileIdBytes");
        let raw = example(|inputs| inputs.selecting("w").bounding(&[(bytes, 2)], &[]));
        let shown = resolved(&resolver, &raw.file(&resolver, true));
        let occurrences = entry_views(&shown, RecordKind::FindingOccurrence);
        let declared = occurrences
            .iter()
            .find(|view| view["finding"]["check"] == "selection.profile-id.within-capability")
            .unwrap();
        let path = &declared["primary"]["value"]["redacted-logical-path"];
        assert_eq!(path[0], json!({ "safe-member": "profiles" }));
        assert!(path[1].get("redacted-member").is_some(), "{path}");
    }

    /// D15:737, 800, 840: either entry's Evidence and dependencies name its
    /// admitted source, `safe-opaque` only when marked non-secret, with its
    /// own admitted locator.
    #[test]
    fn the_entry_is_named_by_the_source_it_was_admitted_under() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        for marked in [true, false] {
            for (inputs, identity, locator) in [
                (
                    raw.file(&resolver, marked),
                    FILE,
                    json!({ "relative-path": FILE_LOCATOR }),
                ),
                (
                    raw.programmatic(&resolver, marked, false),
                    ENTRY,
                    json!({ "safe-label": ENTRY_LOCATOR }),
                ),
            ] {
                let shown = resolved(&resolver, &inputs);
                let dependency = entry_views(&shown, RecordKind::EntryDependency)[0];
                let source = &dependency["source"];
                if marked {
                    assert_eq!(source, &json!({ "safe-opaque": hex(identity.as_bytes()) }));
                } else {
                    assert!(source.get("redacted").is_some(), "{source}");
                }
                assert_eq!(dependency["locator"], locator);
                for view in configuration_evidence(&shown) {
                    assert_eq!((&view["source"], &view["locator"]), (source, &locator));
                }
            }
        }
    }

    /// D15:747, 800: beside the sidecar admitted with it, a Snapshot's
    /// Evidence names the call-site source at the spans it maps, and
    /// without one the Programmatic Entry without a span.
    #[test]
    fn a_sidecar_gives_a_snapshot_its_call_site_spans() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        for (mirror, identity) in [(true, CALL_SITE), (false, ENTRY)] {
            let shown = resolved(&resolver, &raw.programmatic(&resolver, true, mirror));
            for view in configuration_evidence(&shown) {
                assert_eq!(
                    view["source"],
                    json!({ "safe-opaque": hex(identity.as_bytes()) })
                );
                assert_eq!(view.get("span").is_some(), mirror, "{view}");
            }
        }
    }

    /// D15:2751, 3689: after checked and blocked invocations, before or after
    /// materialization, a resolver gives each invocation the outcome a fresh
    /// resolver gives it.
    #[test]
    fn a_reused_resolver_resolves_as_a_fresh_one() {
        let root = ConstructionRoot::built_in().unwrap();
        let reused = resolver();
        let nodes = input_bound("maxNodes");
        let invocations = [
            example(|inputs| inputs),
            Raw::new(&blocked()),
            example(|inputs| inputs.bounding(&[(nodes, 10)], &[])),
            example(|inputs| inputs),
        ];
        for raw in &invocations {
            let fresh = build(root, committed().input().clone()).unwrap();
            let fresh = Resolver::new(&fresh, root);
            assert_eq!(
                resolved(&reused, &raw.file(&reused, true)),
                resolved(&fresh, &raw.file(&fresh, true))
            );
        }
    }

    /// D15:3228, 3414: a file and a Snapshot of its value, beside a mirroring
    /// sidecar or not, share the identity, the checked profile, and the Shared
    /// projection, whether the outcome is checked or blocked, and blocked on
    /// an unknown member nested in the declaration too.
    #[test]
    fn a_file_and_a_snapshot_of_its_value_share_the_shared_projection() {
        let resolver = resolver();
        let mut unknown = example(|inputs| inputs);
        unknown.config["profiles"]["web"]["extra"] = json!({ "deep": true });
        for raw in [example(|inputs| inputs), Raw::new(&blocked()), unknown] {
            let file = resolved(&resolver, &raw.file(&resolver, true));
            assert!(file.shared.is_some());
            for mirror in [false, true] {
                let snapshot = resolved(&resolver, &raw.programmatic(&resolver, true, mirror));
                assert_eq!(snapshot.identity, file.identity);
                assert_eq!(snapshot.checked, file.checked);
                assert_eq!(snapshot.shared, file.shared);
            }
        }
    }

    /// D15:2657, 3440: Phase 1 blocking before a complete JSON-compatible
    /// value exists, over a file that is no JSON, one with a duplicate
    /// member, one past a root-value bound, or a Snapshot with an ordinary or
    /// a bootstrap-limit marker, returns no Materialized Resolution Input Set
    /// beside the blocked outcome.
    #[test]
    fn no_set_is_returned_without_a_materialized_value() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        let duplicate = Arc::from(&br#"{"schemaVersion":"0","schemaVersion":"0"}"#[..]);
        let bounded = example(|inputs| inputs.bounding(&[(input_bound("maxNodes"), 10)], &[]));
        let mut entries = vec![
            raw.file_of(&resolver, no_json(), true),
            raw.file_of(&resolver, duplicate, true),
            bounded.file(&resolver, true),
        ];
        let terminal = Marker::BootstrapLimitExceeded {
            bound: SnapshotBound::Nodes,
            witness: ROOM + 1,
        };
        for marker in [Marker::Rejected(MarkerReason::Cycle), terminal] {
            let mut root = from_json(&raw.config);
            let Node::Object(members) = &mut root else {
                panic!("the root is an object");
            };
            // Last in canonical order, after every key the example writes.
            members.insert("~".to_owned(), Node::Marker(marker));
            entries.push(raw.snapshot(&resolver, &root, true, false));
        }
        for inputs in &entries {
            let mut storage = OutcomeStorage::default();
            let resolution = resolver.resolve(inputs, &mut storage).unwrap();
            assert!(shown(&resolution.outcome).shared.is_none());
            assert!(resolution.materialized.is_none());
        }
    }

    /// D15:2657: once Phase 1 materialized the value, either entry returns a
    /// set beside its outcome, checked or blocked by a later phase.
    #[test]
    fn a_set_is_returned_however_a_later_phase_ends() {
        let resolver = resolver();
        let units = input_bound("maxStructuralAnalysisUnits");
        for (raw, checked) in [
            (example(|inputs| inputs), true),
            (
                example(|inputs| inputs.bounding(&[(units, 10)], &[])),
                false,
            ),
            (Raw::new(&blocked()), false),
        ] {
            for inputs in [
                raw.file(&resolver, true),
                raw.programmatic(&resolver, true, false),
            ] {
                let mut storage = OutcomeStorage::default();
                let resolution = resolver.resolve(&inputs, &mut storage).unwrap();
                assert_eq!(shown(&resolution.outcome).checked.is_some(), checked);
                assert!(resolution.materialized.is_some());
            }
        }
    }

    /// D15:3155, 3321: a report without a formal container is one typed
    /// failure of the collection's slot. The pattern names every field of the
    /// refused set, so nothing resolver-shaped can come beside it.
    #[test]
    fn a_refused_container_leaves_no_resolver_shaped_output() {
        let resolver = resolver();
        let raw = example(|inputs| inputs);
        for (report, failure) in [
            (
                CollectionReport::Unsafe,
                CollectionFailure::UnsafeHostInspection,
            ),
            (
                CollectionReport::MalformedContainer,
                CollectionFailure::MalformedContainer,
            ),
        ] {
            let refused = raw.submit_file(&resolver, raw.bytes(), true, report);
            let InvocationFailures {
                construction_identity,
                common,
                entry,
            } = refused.err().unwrap();
            assert_eq!(construction_identity, None);
            assert_eq!(entry, None);
            let expected = InputFailure {
                slot: Kind::ProfileResolutionArtifactCollection,
                refusal: Refusal::Collection(failure),
            };
            assert_eq!(common, Some(vec![expected]));
        }
    }

    /// D15:3165, 3322: a formal container is admitted with every representable
    /// invalid member and occurrence, in the order submitted, and the
    /// resolver reports them: one Shared Finding for the repeated member and
    /// one Entry occurrence for each time it was submitted. The same members
    /// in a container reported as malformed reach no resolver.
    #[test]
    fn an_admitted_container_keeps_its_invalid_members_for_the_resolver() {
        let resolver = resolver();
        let mut raw = example(|inputs| inputs);
        let invalid = br#"{"kind":"not-an-artifact"}"#.to_vec();
        raw.members.insert(1, invalid.clone());
        raw.members.push(invalid);
        let inputs = raw.file(&resolver, true);
        let expected: Vec<FormalMember> = raw
            .members
            .iter()
            .map(|bytes| FormalMember::new(serde_json::from_slice(bytes).unwrap()).unwrap())
            .collect();
        assert_eq!(inputs.common.collection.members(), expected.as_slice());
        let shown = resolved(&resolver, &inputs);
        assert!(shown.checked.is_none());
        let (records, _) = shown.shared.as_ref().unwrap();
        let findings: Vec<&Value> = records
            .iter()
            .filter(|(_, kind, _)| *kind == RecordKind::SharedFinding)
            .map(|(_, _, view)| view)
            .collect();
        let [finding] = findings[..] else {
            panic!("one Finding for the repeated member: {findings:?}");
        };
        let key = &finding["key"];
        assert_eq!(
            key["code"],
            "project-profile-artifact-collection-not-admitted"
        );
        assert_eq!(key["reason"], "kind-unsupported");
        let occurrences = entry_views(&shown, RecordKind::FindingOccurrence);
        assert_eq!(occurrences.len(), 2);
        assert!(occurrences.iter().all(|view| view["finding"] == *key));
        let report = CollectionReport::MalformedContainer;
        let refused = raw.submit_file(&resolver, raw.bytes(), true, report);
        assert!(refused.is_err());
    }

    /// Where each member key and string value of `value` is, a key as its
    /// path with `true` beside it.
    fn string_leaves(value: &Value) -> Vec<(Vec<Value>, bool)> {
        fn walk(value: &Value, at: &[Value], leaves: &mut Vec<(Vec<Value>, bool)>) {
            let step = |edge: Value| [at, &[edge]].concat();
            match value {
                Value::Object(members) => {
                    for (key, member) in members {
                        let here = step(json!(key));
                        leaves.push((here.clone(), true));
                        walk(member, &here, leaves);
                    }
                }
                Value::Array(items) => {
                    for (index, item) in items.iter().enumerate() {
                        walk(item, &step(json!(index)), leaves);
                    }
                }
                Value::String(_) => leaves.push((at.to_vec(), false)),
                _ => {}
            }
        }
        let mut leaves = Vec::new();
        walk(value, &[], &mut leaves);
        leaves
    }

    /// `config` with the key or the string at `path` replaced by `canary`.
    fn place(config: &mut Value, path: &[Value], is_key: bool, canary: &str) {
        fn child<'v>(node: &'v mut Value, edge: &Value) -> &'v mut Value {
            match edge {
                Value::String(key) => &mut node[key.as_str()],
                index => &mut node[usize::try_from(index.as_u64().unwrap()).unwrap()],
            }
        }
        let (last, parents) = path.split_last().unwrap();
        let node = parents.iter().fold(config, child);
        if is_key {
            let members = node.as_object_mut().unwrap();
            let moved = members.remove(last.as_str().unwrap()).unwrap();
            members.insert(canary.to_owned(), moved);
        } else {
            *child(node, last) = json!(canary);
        }
    }

    /// What retained text must not hold for `canary`: its bytes, their
    /// hexadecimal spelling, and their direct digest and that of their
    /// canonical framing, in hexadecimal and as a digest's `Debug` lists its
    /// bytes (D15:2693, 3393).
    fn traces(canary: &str) -> Vec<String> {
        let framed = encode(&json!(canary)).unwrap();
        let mut traces = vec![canary.to_owned(), hex(canary.as_bytes())];
        for digest in [Sha256.digest(canary.as_bytes()), Sha256.digest(&framed)] {
            traces.push(hex(&digest));
            traces.push(format!("{digest:?}"));
        }
        traces
    }

    /// Every representation `outcome` retains, as one text: the `Debug` of
    /// the outcome, which holds every record, its owner, the terminal cursors,
    /// and a checked profile, and of the unavailability causes; and the view
    /// of every record and terminal Finding, state, and cause (D15:2697).
    fn retained(outcome: &Outcome<'_, '_>) -> String {
        let evidence = &outcome.evidence;
        let mut text = format!("{outcome:?}{:?}", evidence.unavailable());
        for projection in std::iter::once(&evidence.entry).chain(&evidence.shared) {
            for (_, record) in &projection.records {
                text.push_str(&record.view.to_string());
            }
            if let Some(terminal) = &projection.terminal {
                text.push_str(&terminal.finding.to_string());
                text.push_str(&terminal.state.to_string());
                if let Some(cause) = &terminal.cause {
                    text.push_str(&cause.to_string());
                }
            }
        }
        text
    }

    /// What D15:3393's sweep has scanned: each record kind, and each exhausted
    /// domain's terminal record.
    #[derive(Default)]
    struct Sweep {
        kinds: BTreeSet<RecordKind>,
        terminals: usize,
    }

    impl Sweep {
        /// What the outcome of `inputs` shows, once none of what it retains
        /// holds a trace of any of `canaries`.
        fn scan(
            &mut self,
            resolver: &Resolver<'_>,
            inputs: &InvocationInputs<PinnedFixture>,
            canaries: &[&str],
        ) -> Shown {
            let mut storage = OutcomeStorage::default();
            let resolution = resolver.resolve(inputs, &mut storage).unwrap();
            let outcome = &resolution.outcome;
            let text = retained(outcome);
            for canary in canaries {
                for trace in traces(canary) {
                    assert!(!text.contains(&trace), "{canary} left {trace}");
                }
            }
            let evidence = &outcome.evidence;
            for projection in std::iter::once(&evidence.entry).chain(&evidence.shared) {
                let kinds = projection.records.iter().map(|(_, record)| record.kind);
                self.kinds.extend(kinds);
                self.terminals += usize::from(projection.terminal.is_some());
            }
            shown(outcome)
        }
    }

    /// Whether an Evidence of `shown`'s occurrences holds a span over the
    /// bytes `[start, end)`, one no wider than `within` bytes.
    fn locates(shown: &Shown, (start, end): (usize, usize), within: usize) -> bool {
        let coordinate = |at: &Value| at.as_str().and_then(|text| text.parse::<usize>().ok());
        entry_views(shown, RecordKind::FindingOccurrence)
            .iter()
            .flat_map(|view| {
                let related = view["related"].as_array().into_iter().flatten();
                std::iter::once(&view["primary"]).chain(related)
            })
            .filter_map(|evidence| {
                let span = &evidence["span"];
                Some((coordinate(&span["start"])?, coordinate(&span["end"])?))
            })
            .any(|(from, to)| from <= start && end <= to && to - from <= within)
    }

    /// D15:3393 over the configuration: each member key and each string of
    /// the example, replaced in turn by a canary, through either entry. The
    /// canary is admitted nowhere, so the outcome is blocked and a Finding's
    /// span holds it, narrower than the root's but for a renamed
    /// `schemaVersion`, whose absence stops selection first. Only `$schema`,
    /// which no check reads, leaves the outcome checked.
    fn sweep_configuration(sweep: &mut Sweep, resolver: &Resolver<'_>) {
        let mut raw = example(|inputs| inputs);
        let base = raw.config.clone();
        let leaves = string_leaves(&base);
        assert_eq!(leaves.len(), 168);
        for (n, (path, is_key)) in leaves.iter().enumerate() {
            let canary = format!("{CANARY} {n}");
            raw.config = base.clone();
            place(&mut raw.config, path, *is_key, &canary);
            let bytes = serde_json::to_vec(&raw.config).unwrap();
            let token = serde_json::to_vec(&canary).unwrap();
            let start = bytes
                .windows(token.len())
                .position(|window| window == token)
                .unwrap();
            let renamed = *is_key && path[..] == [json!("schemaVersion")];
            let within = bytes.len() - usize::from(!renamed);
            for inputs in [
                raw.file(resolver, false),
                raw.programmatic(resolver, false, true),
            ] {
                let shown = sweep.scan(resolver, &inputs, &[&canary]);
                if shown.checked.is_some() {
                    assert_eq!(path[..], [json!("$schema")]);
                } else {
                    let at = (start, start + token.len());
                    assert!(locates(&shown, at, within), "{path:?}");
                }
            }
        }
    }

    const FILE_IDENTITY: &str = "intlify-file-source.v0:T3 CANARY file identity";
    const ENTRY_IDENTITY: &str = "intlify-programmatic-entry.v0:T3 CANARY entry identity";
    const SITE_IDENTITY: &str = "intlify-call-site.v0:T3 CANARY site identity";

    /// Locator candidates no source admits, each with the canary it carries
    /// in what 015 excludes: an unclassified label and path, a path that
    /// climbs its root, and a URI with user information, a query, a fragment,
    /// the `file` scheme, an escaped unreserved byte, or an uppercase scheme
    /// (D15:762-766).
    const LOCATORS: [(LocatorVariant, &str, bool, &str); 9] = [
        (
            LocatorVariant::SafeLabel,
            "T3 CANARY label",
            false,
            "T3 CANARY label",
        ),
        (
            LocatorVariant::RelativePath,
            "t3canarypath/intlify.json",
            false,
            "t3canarypath",
        ),
        (
            LocatorVariant::RelativePath,
            "../t3canarydots.json",
            true,
            "t3canarydots",
        ),
        (
            LocatorVariant::SafeUri,
            "https://t3canaryuser@example.com/a",
            true,
            "t3canaryuser",
        ),
        (
            LocatorVariant::SafeUri,
            "https://example.com/a?t3canaryquery",
            true,
            "t3canaryquery",
        ),
        (
            LocatorVariant::SafeUri,
            "https://example.com/a#t3canaryfragment",
            true,
            "t3canaryfragment",
        ),
        (
            LocatorVariant::SafeUri,
            "file:///t3canaryfile",
            true,
            "t3canaryfile",
        ),
        (
            LocatorVariant::SafeUri,
            "https://example.com/%7Et3canaryescape",
            true,
            "t3canaryescape",
        ),
        (
            LocatorVariant::SafeUri,
            "HTTPS://example.com/t3canaryscheme",
            true,
            "t3canaryscheme",
        ),
    ];

    /// D15:3393 over the sources: unmarked identities, each locator candidate
    /// a source does not admit, through a file and a Snapshot beside a sidecar
    /// or none. Every configuration Evidence names its source by a redacted
    /// token and has no locator.
    fn sweep_sources(sweep: &mut Sweep, resolver: &Resolver<'_>) {
        let mut raw = example(|inputs| inputs);
        raw.config["extra"] = json!(true);
        let clean = example(|inputs| inputs);
        for (variant, payload, non_secret, canary) in LOCATORS {
            let submitted = LocatorCandidate {
                variant,
                payload,
                non_secret,
            };
            let sources = Sources {
                file: FILE_IDENTITY,
                file_locator: submitted,
                entry: ENTRY_IDENTITY,
                entry_locator: submitted,
                call_site: SITE_IDENTITY,
                call_site_marked: false,
                call_site_locator: Some(submitted),
            };
            raw.sources = sources;
            let canaries = [
                canary,
                FILE_IDENTITY,
                ENTRY_IDENTITY,
                SITE_IDENTITY,
                payload,
            ];
            for inputs in [
                raw.file(resolver, false),
                raw.programmatic(resolver, false, false),
                raw.programmatic(resolver, false, true),
            ] {
                let shown = sweep.scan(resolver, &inputs, &canaries);
                assert!(shown.checked.is_none());
                for view in configuration_evidence(&shown) {
                    assert!(view["source"].get("redacted").is_some(), "{view}");
                    assert!(view.get("locator").is_none(), "{view}");
                }
            }
        }
        let mut checked = clean;
        checked.sources = raw.sources;
        for inputs in [
            checked.file(resolver, false),
            checked.programmatic(resolver, false, true),
        ] {
            let shown = sweep.scan(
                resolver,
                &inputs,
                &[FILE_IDENTITY, ENTRY_IDENTITY, SITE_IDENTITY],
            );
            assert!(shown.checked.is_some());
        }
    }

    /// D15:3393 over a selector no declaration has, which its Evidence keeps
    /// only as a redacted string with its length.
    fn sweep_selector(sweep: &mut Sweep, resolver: &Resolver<'_>) {
        let mut raw = example(|inputs| inputs);
        raw.selected = format!("{CANARY} selector");
        for inputs in [
            raw.file(resolver, false),
            raw.programmatic(resolver, false, false),
        ] {
            let shown = sweep.scan(resolver, &inputs, &[&raw.selected]);
            let occurrences = entry_views(&shown, RecordKind::FindingOccurrence);
            let found = occurrences
                .iter()
                .find_map(|view| view["primary"].get("selector"))
                .expect("the selector is the Finding's Evidence");
            let value = &found["redacted-value"];
            assert_eq!(value["type"], "string", "{found}");
            assert!(value.get("length").is_some(), "{found}");
        }
    }

    /// D15:3393, 3367 over the collection and the vocabulary: a member of
    /// each way to be rejected, every occurrence's Evidence a redacted key.
    fn sweep_artifacts(sweep: &mut Sweep, resolver: &Resolver<'_>) {
        const CANARIES: [&str; 9] = [
            "T3 CANARY member",
            "T3 CANARY key",
            "T3 CANARY value",
            "T3 CANARY kind",
            "t3canarytuple",
            "t3canaryduplicate",
            "t3canaryconflict",
            "t3canarybody",
            "t3canaryother",
        ];
        let sealed = |identity: &str, content: Value| {
            envelope_of(ArtifactKind::TrustPolicy, identity, "1", content)
        };
        let mut broken = sealed("t3canarytuple", json!({ "t3canarybody": true }));
        broken["integrityDigest"] = json!(format!("sha256:{}", "0".repeat(64)));
        let twice = sealed("t3canaryduplicate", json!({ "t3canarybody": true }));
        let mut raw = example(|inputs| inputs);
        for member in [
            json!("T3 CANARY member"),
            json!({ "T3 CANARY key": "T3 CANARY value" }),
            json!({ "kind": "T3 CANARY kind", "schemaRevision": "0" }),
            broken,
            twice.clone(),
            twice,
            sealed("t3canaryconflict", json!({ "t3canarybody": true })),
            sealed("t3canaryconflict", json!({ "t3canaryother": true })),
        ] {
            raw.members.push(serde_json::to_vec(&member).unwrap());
        }
        let shown = sweep.scan(resolver, &raw.file(resolver, false), &CANARIES);
        let occurrences = entry_views(&shown, RecordKind::FindingOccurrence);
        let reasons: BTreeSet<&str> = occurrences
            .iter()
            .filter(|view| view["primary"]["kind"] == "artifact-input")
            .inspect(|view| {
                let key = &view["primary"]["component"][1];
                assert!(key.get("redacted-key").is_some(), "{view}");
            })
            .filter_map(|view| view["finding"]["reason"].as_str())
            .collect();
        assert_eq!(
            reasons,
            BTreeSet::from([
                "conflicting-content",
                "duplicate-envelope",
                "envelope-invalid",
                "kind-unsupported",
            ])
        );
        let surface = "T3 CANARY surface";
        let raw = example(|mut inputs| {
            inputs.submit_surfaces(&["nav", "T3 CANARY surface"]);
            inputs
        });
        let shown = sweep.scan(resolver, &raw.file(resolver, false), &[surface]);
        let occurrences = entry_views(&shown, RecordKind::FindingOccurrence);
        assert!(occurrences.iter().any(|view| {
            view["primary"]["origin"] == "intent-surface-class-vocabulary"
                && view["primary"]["component"][1]
                    .get("redacted-key")
                    .is_some()
        }));
    }

    /// D15:3393 over what Phase 1 rejects: a repeated key and a truncated
    /// file, and a Snapshot's marker under a canary key, each observed with
    /// the key redacted and no value built.
    fn sweep_entries(sweep: &mut Sweep, resolver: &Resolver<'_>) {
        let raw = example(|inputs| inputs);
        let key = "T3 CANARY duplicate";
        let repeated = format!(r#"{{"{key}":true,"{key}":false}}"#);
        let cut = r#"{"T3 CANARY truncated":"T3 CANARY value"#;
        let files: [(&str, &[&str]); 2] = [
            (&repeated, &[key]),
            (cut, &["T3 CANARY truncated", "T3 CANARY value"]),
        ];
        for (text, canaries) in files {
            let inputs = raw.file_of(resolver, Arc::from(text.as_bytes()), false);
            let shown = sweep.scan(resolver, &inputs, canaries);
            assert!(shown.shared.is_none());
            let occurrences = entry_views(&shown, RecordKind::FindingOccurrence);
            assert!(!occurrences.is_empty());
            for view in occurrences {
                assert!(view["primary"].get("span").is_some(), "{view}");
            }
        }
        let marker = "T3 CANARY marker";
        let mut root = from_json(&raw.config);
        let Node::Object(members) = &mut root else {
            panic!("the root is an object");
        };
        members.insert(
            marker.to_owned(),
            Node::Marker(Marker::Rejected(MarkerReason::DynamicProperty)),
        );
        let inputs = raw.snapshot(resolver, &root, false, false);
        let shown = sweep.scan(resolver, &inputs, &[marker]);
        assert!(entry_views(&shown, RecordKind::FindingOccurrence)
            .iter()
            .any(|view| view["primary"]["path"][0].get("redacted-member").is_some()));
    }

    /// D15:776, 3393 over identifiers past the capability's byte bound for
    /// them: a key is then unsafe whatever its syntax, and a member or an
    /// endpoint naming one is no admitted identity.
    fn sweep_over_bound_identifiers(sweep: &mut Sweep, resolver: &Resolver<'_>) {
        type Edit = fn(&mut Value, &str);
        let target = |name| BoundId {
            group: Group::TargetGrouping,
            name,
        };
        let tight = [
            (input_bound("maxProfileIdBytes"), 8),
            (target("maxTargetIdBytes"), 8),
            (target("maxGroupIdBytes"), 8),
        ];
        let edits: [(&str, Edit); 5] = [
            ("t3canaryprofile", |config, id| {
                config["profiles"][id] = config["profiles"]["web"].clone();
            }),
            ("t3canarytarget", |config, id| {
                let targets = &mut config["profiles"]["web"]["targetProfiles"];
                targets[id] = targets["server"].clone();
            }),
            ("t3canarygroup", |config, id| {
                let groups = &mut config["profiles"]["web"]["deploymentGroups"];
                groups[id] = groups["web"].clone();
            }),
            ("t3canarymember", |config, id| {
                config["profiles"]["web"]["deploymentGroups"]["web"]["members"][0] = json!(id);
            }),
            ("t3canaryendpoint", |config, id| {
                let group = &mut config["profiles"]["web"]["deploymentGroups"]["web"];
                group["hydrationRelations"][0]["client"] = json!(id);
            }),
        ];
        for (canary, edit) in edits {
            let mut raw = example(|inputs| inputs.bounding(&tight, &tight));
            edit(&mut raw.config, canary);
            for inputs in [
                raw.file(resolver, false),
                raw.programmatic(resolver, false, true),
            ] {
                let shown = sweep.scan(resolver, &inputs, &[canary]);
                assert!(shown.checked.is_none(), "{canary}");
            }
        }
    }

    /// D15:3393 over the terminal records: a domain exhausted by canary keys
    /// names its cursor by tokens alone.
    fn sweep_terminals(sweep: &mut Sweep, resolver: &Resolver<'_>) {
        let tight = [(entry("maxRecords"), 3), (shared("maxRecords"), 3)];
        let mut raw = Raw::new(&reporting(&Inputs::complete(), &tight, &tight));
        let canaries = ["T3 CANARY one", "T3 CANARY two"];
        for canary in canaries {
            raw.config[canary] = json!(canary);
        }
        let before = sweep.terminals;
        sweep.scan(resolver, &raw.file(resolver, false), &canaries);
        assert!(sweep.terminals > before);
    }

    /// D15:3393, 3144: what no resolver reads is refused before invocation,
    /// each failure typed with nothing of the input: an origin candidate
    /// beside every slot, and an unreadable specification input, vocabulary,
    /// Verification Input, and capability.
    fn sweep_refusals(resolver: &Resolver<'_>) {
        let refused = |raw: &Raw, canary: &str| {
            let members = raw.member_bytes();
            let report = CollectionReport::Formal(&members);
            let failures = raw
                .submit_file(resolver, raw.bytes(), false, report)
                .err()
                .expect("the invocation is refused");
            let text = format!("{failures:?}");
            for trace in traces(canary) {
                assert!(!text.contains(&trace), "{canary} left {trace}");
            }
            failures
        };
        let mut raw = example(|inputs| inputs);
        raw.origin_candidate = Some("T3 CANARY origin");
        let failures = refused(&raw, "T3 CANARY origin");
        let origins = failures.common.iter().chain(&failures.entry).flatten();
        let unknown = Refusal::Origin(OriginRefusal::Unknown);
        assert_eq!(
            origins.filter(|failure| failure.refusal == unknown).count(),
            8
        );
        let body = br#"{"T3 CANARY input":"T3 CANARY input"}"#;
        for edit in [
            (|raw, body| raw.specification = body) as fn(&mut Raw, Vec<u8>),
            |raw, body| raw.vocabulary = body,
            |raw, body| raw.verification = body,
            |raw, body| raw.capability = serde_json::from_slice(&body).unwrap(),
        ] {
            let mut raw = example(|inputs| inputs);
            edit(&mut raw, body.to_vec());
            refused(&raw, "T3 CANARY input");
        }
    }

    /// No fixture holds what the sweep's inputs carry (D15:3393).
    fn assert_no_fixture_holds_a_canary() {
        fn walk(dir: &std::path::Path, files: &mut usize) {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    walk(&path, files);
                } else {
                    *files += 1;
                    let bytes = std::fs::read(&path).unwrap();
                    for canary in [CANARY.as_bytes(), b"t3canary"] {
                        let held = bytes.windows(canary.len()).any(|window| window == canary);
                        assert!(!held, "{}", path.display());
                    }
                }
            }
        }
        let mut files = 0;
        walk(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures"),
            &mut files,
        );
        assert!(files > 0);
    }

    /// D15:2693, 2697, 3393: secret canaries in every input kind that reaches
    /// analysis produce Findings that locate them, while neither the bytes nor
    /// a direct digest of them is in any record of any of the twelve kinds,
    /// a terminal record, an owner, or the `Debug` of the outcome, of a
    /// refusal, or of a fixture. The crate has no log, cache, or telemetry
    /// channel for them to reach.
    #[test]
    fn a_secret_canary_leaves_no_trace_in_anything_an_outcome_retains() {
        let resolver = resolver();
        let mut sweep = Sweep::default();
        sweep_configuration(&mut sweep, &resolver);
        sweep_sources(&mut sweep, &resolver);
        sweep_selector(&mut sweep, &resolver);
        sweep_artifacts(&mut sweep, &resolver);
        sweep_entries(&mut sweep, &resolver);
        sweep_over_bound_identifiers(&mut sweep, &resolver);
        sweep_terminals(&mut sweep, &resolver);
        assert_eq!(sweep.kinds.len(), 12, "{:?}", sweep.kinds);
        sweep_refusals(&resolver);
        assert_no_fixture_holds_a_canary();
    }
}
