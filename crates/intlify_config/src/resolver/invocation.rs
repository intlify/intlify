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
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::envelope::ExactTuple;
    use crate::artifacts::vectors::vocabulary as vectors;
    use crate::artifacts::verification::{sealed, unsealed, VerificationInputArtifact};
    use crate::artifacts::vocabulary::IntentSurfaceClassVocabularyArtifact;
    use crate::boundary::call_site::{CallSiteReport, SubmittedCallSite};
    use crate::boundary::common::Submitted;
    use crate::boundary::entry::{
        FileValue, ProgrammaticValue, SubmittedFileEntry, SubmittedProgrammaticEntry,
    };
    use crate::boundary::invocation::SubmittedEntry;
    use crate::boundary::locator::{LocatorCandidate, LocatorVariant};
    use crate::boundary::selector::SelectorReport;
    use crate::boundary::snapshot::{submitted, SnapshotReport};
    use crate::boundary::source::{call_site_source_revision, file_source_revision};
    use crate::boundary::step::Component;
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
    use crate::resolver::outcome::tests::{blocked, entry_views, no_json, reporting, shown, Shown};
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::snapshot_entry::fixtures::ENTRY;
    use crate::snapshot::{from_json, Marker, MarkerReason, Node, SnapshotBound};

    /// A bound no invocation here reaches.
    pub(crate) const ROOM: u64 = 1 << 30;

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

    fn candidate(variant: LocatorVariant, payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant,
            payload,
            non_secret: true,
        }
    }

    /// The file entries' locator, and the Programmatic Entries'.
    const FILE_LOCATOR: &str = "storefront/intlify.config.json";
    const ENTRY_LOCATOR: &str = "storefront-app";

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
            }
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
                origin: Some(slot.id()),
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
            let revision = file_source_revision(&bytes);
            let file = FileValue {
                bytes,
                identity: Some(FILE),
                marked,
                revision: Some(&revision),
                locator: Some(candidate(LocatorVariant::RelativePath, FILE_LOCATOR)),
            };
            self.admit(
                resolver,
                SubmittedEntry::File(SubmittedFileEntry {
                    file: Component {
                        value: file,
                        origin: Some(Kind::ConfigurationFile.id()),
                    },
                    selector: self.selector(Kind::ProfileSelectorCli),
                }),
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
                    identity: Some(CALL_SITE),
                    marked: true,
                    revision: Some(&revision),
                    locator: None,
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
                identity: Some(ENTRY),
                marked,
                revision: Some(&digest),
                locator: Some(candidate(LocatorVariant::SafeLabel, ENTRY_LOCATOR)),
                call_site,
            };
            self.admit(
                resolver,
                SubmittedEntry::Programmatic(SubmittedProgrammaticEntry {
                    programmatic: Component {
                        value: programmatic,
                        origin: Some(Kind::ConfigurationProgrammatic.id()),
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

        /// The set of these values and `entry`, each beside its slot's own
        /// Safe Origin Kind, admitted on `resolver`.
        fn admit(
            &self,
            resolver: &Resolver<'_>,
            entry: SubmittedEntry<'_>,
        ) -> InvocationInputs<PinnedFixture> {
            let own = |slot: Kind| Some(slot.id());
            let members: Vec<&[u8]> = self.members.iter().map(Vec::as_slice).collect();
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
                    value: Some(&members),
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
            resolver.admit(submitted, &self.assertion).unwrap()
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

    /// D15:3228: a file and a Snapshot of its value, beside a mirroring
    /// sidecar or not, share the identity, the checked profile, and the Shared
    /// projection, whether the outcome is checked or blocked.
    #[test]
    fn a_file_and_a_snapshot_of_its_value_share_the_shared_projection() {
        let resolver = resolver();
        for raw in [example(|inputs| inputs), Raw::new(&blocked())] {
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
}
