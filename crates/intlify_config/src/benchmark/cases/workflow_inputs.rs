// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What a binding submits for one file entry's complete invocation of an
//! Evidence recipe on a resolver built from the committed construction input:
//! the recipe's configuration file and members, as the Evidence stages read
//! them, and every other component as the raw value its pre-invocation
//! boundary admits.
//!
//! The Profile Resolution Specification Input asserts what the resolver
//! supports in each slot, and the benchmark's vocabulary in its own. The
//! canonicalization data is the committed ICU4X artifact, CLDR 48.2.0's, the
//! one dataset the resolver supports, so every invocation binds one exact
//! provider, specification, and data identity (D15:2790). The Verification
//! Input pins the policy the declaration selects. The capability is the
//! owner's for the recipe's bounds, with the `bootstrapOnly` groups the
//! invocation reads above their minima, which neither the file nor the data
//! is within. A case reused after a blocked outcome also holds the Blocked
//! recipe's submission, which it resolves before each start mark. Everything
//! is built before any interval (D15:2824).
//!
//! A programmatic entry submits, in place of the file, the formal Snapshot
//! of the value the file's bytes spell, so a programmatic case pairs with
//! the file case of its recipe (D15:2788), or, for the recipe no file has,
//! that value of the example holding an ordinary marker.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::artifacts::verification::{sealed, unsealed, VerificationInputArtifact};
use crate::benchmark::workflow::{Before, RawEntry, RawInvocation};
use crate::benchmark::{evidence, locale_icu4x};
use crate::boundary::capability::ImplementationCapability;
use crate::boundary::snapshot::SnapshotReport;
use crate::boundary::source::file_source_revision;
use crate::boundary::verification::VerificationAssertion;
use crate::construction::resolver::ConstructedResolver;
use crate::construction::root::ConstructionRoot;
use crate::locale::VersionedIdentity;
use crate::resolver::capability::{BootstrapGroup, BOOTSTRAP_ONLY};
use crate::resolver::limits::ResolutionLimits;
use crate::resolver::specification::SupportedSpecifications;

use super::policy_inputs::{reference_value, tuple_of};
use super::snapshot_inputs::{spelled, SnapshotRecipe};
use super::{EntryKind, EvidenceRecipe, Recipe, ResolverState};

/// A programmatic entry's recipes: each Evidence recipe whose value a
/// Snapshot represents, submitting the value its file spells, and the
/// example's value holding the ordinary marker of an unsupported host value,
/// which Phase 1 blocks with no value (D15:861, 2129). The malformed file has
/// no value, and the inadmissible one's repeated member and lone surrogate
/// are no host value's.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum ProgrammaticRecipe {
    Storefront,
    Blocked,
    Dense,
    Truncated,
    RejectedMarker,
}

impl ProgrammaticRecipe {
    pub(in crate::benchmark) const ALL: [Self; 5] = [
        Self::Storefront,
        Self::Blocked,
        Self::Dense,
        Self::Truncated,
        Self::RejectedMarker,
    ];

    /// The file recipe whose value the Snapshot submits.
    pub(in crate::benchmark) const fn file(self) -> EvidenceRecipe {
        match self {
            Self::Storefront | Self::RejectedMarker => EvidenceRecipe::Storefront,
            Self::Blocked => EvidenceRecipe::Blocked,
            Self::Dense => EvidenceRecipe::Dense,
            Self::Truncated => EvidenceRecipe::Truncated,
        }
    }

    /// The Snapshot recipe whose root member the value holds beside it.
    const fn beside(self) -> Option<SnapshotRecipe> {
        match self {
            Self::RejectedMarker => Some(SnapshotRecipe::UnsupportedType),
            _ => None,
        }
    }
}

/// The `bootstrapOnly` groups the invocation reads that the owner's capability
/// leaves at their minima: strict file entry's, a Snapshot's, which a
/// programmatic entry under the same capability reads, the other
/// pre-invocation boundaries', the canonicalization data's, and the Evidence
/// metadata's. `artifactBootstrap` is already as large as a bound can be, and
/// `mandatoryReserve` stays at its minima.
const WIDENED: [BootstrapGroup; 5] = [
    BootstrapGroup::RawInput,
    BootstrapGroup::Snapshot,
    BootstrapGroup::InputBoundary,
    BootstrapGroup::LocaleData,
    BootstrapGroup::EvidenceMetadata,
];

/// A bound no count of these invocations reaches.
pub(super) const ROOM: u64 = 1 << 30;

/// The Verification Input trusted setup supplies, by identity and revision.
const VERIFICATION: (&str, &str) = ("intlify-benchmark-verification-input", "0");

/// The Profile ID every recipe's selector names.
const SELECTED: &str = "app";

/// What a binding submits for `recipe`'s invocation on `constructed`, which
/// construction built under `root`, `None` when a fixture does not encode.
pub(in crate::benchmark) fn submission(
    recipe: EvidenceRecipe,
    constructed: &ConstructedResolver,
    root: &ConstructionRoot,
) -> Option<RawInvocation> {
    let limits = recipe.limits();
    let fixture = recipe.fixture(&limits);
    let (verification, assertion) = verification(fixture.members.first()?)?;
    let members = fixture.members.iter().map(serde_json::to_vec);
    let file: Arc<[u8]> = Arc::from(recipe.source(&limits));
    let (manifest, payloads) = locale_icu4x::submitted();
    Some(RawInvocation {
        reference: constructed.identity(root).text(),
        specification: specification(&constructed.supported())?,
        vocabulary: evidence::vocabulary_bytes()?,
        manifest,
        payloads,
        members: members.collect::<Result<_, _>>().ok()?,
        verification,
        capability: capability(limits),
        assertion,
        entry: RawEntry::File {
            revision: file_source_revision(&file),
            bytes: file,
        },
        selector: SELECTED.to_owned(),
    })
}

/// What a binding submits for `recipe`'s invocation through a programmatic
/// entry on `constructed`: its file recipe's submission with the formal
/// Snapshot of the value the file spells in place of the file, its revision
/// the Snapshot's digest (D15:889).
pub(in crate::benchmark) fn programmatic(
    recipe: ProgrammaticRecipe,
    constructed: &ConstructedResolver,
    root: &ConstructionRoot,
) -> Option<RawInvocation> {
    let mut raw = submission(recipe.file(), constructed, root)?;
    let RawEntry::File { bytes, .. } = &raw.entry else {
        return None;
    };
    let snapshot = spelled(Arc::clone(bytes), recipe.beside())?;
    let SnapshotReport::Formal { digest, .. } = &snapshot else {
        return None;
    };
    let revision = digest.clone();
    raw.entry = RawEntry::Programmatic { snapshot, revision };
    Some(raw)
}

/// What each repetition of a case invoking a resolver in `state` runs before
/// its start mark, on `constructed`: for a resolver reused after a blocked
/// outcome, the submission of the same entry's Blocked recipe, the semantic
/// blocked one, on that resolver (plan 025 decision #316). `None` when that
/// fixture does not encode.
pub(in crate::benchmark) fn before(
    state: ResolverState,
    entry: EntryKind,
    constructed: &ConstructedResolver,
    root: &ConstructionRoot,
) -> Option<Before> {
    Some(match state {
        ResolverState::Fresh => Before::Fresh,
        ResolverState::Reused => Before::Reused,
        ResolverState::ReusedAfterBlocked => {
            let blocked = match entry {
                EntryKind::File => submission(EvidenceRecipe::Blocked, constructed, root),
                EntryKind::Programmatic => {
                    programmatic(ProgrammaticRecipe::Blocked, constructed, root)
                }
            }?;
            Before::AfterBlocked(Box::new(blocked))
        }
    })
}

/// What a binding submits for a complete invocation case's `fixture` on
/// `constructed`, through the entry it names, and what each of its
/// repetitions runs first. `None` for any other recipe, or when a fixture
/// does not encode.
pub(in crate::benchmark) fn rebuilt(
    fixture: Recipe,
    constructed: &ConstructedResolver,
    root: &ConstructionRoot,
) -> Option<(RawInvocation, Before)> {
    let (raw, resolver, entry) = match fixture {
        Recipe::Workflow { input, resolver } => (
            submission(input, constructed, root)?,
            resolver,
            EntryKind::File,
        ),
        Recipe::ProgrammaticWorkflow { input, resolver } => (
            programmatic(input, constructed, root)?,
            resolver,
            EntryKind::Programmatic,
        ),
        _ => return None,
    };
    Some((raw, before(resolver, entry, constructed, root)?))
}

/// The Profile Resolution Specification Input in the raw form 015-165 fixes:
/// each slot asserting what `supported` supports, and the vocabulary slot the
/// benchmark's vocabulary.
fn specification(supported: &SupportedSpecifications<String>) -> Option<Vec<u8>> {
    let canonicalization = supported.locale_canonicalization().entries().first()?;
    let vocabulary = evidence::specifications().intent_surface_class_vocabulary?;
    let input = json!({
        "project-profile": slot(&supported.project_profile().specification),
        "locale-canonicalization": slot(&canonicalization.specification),
        "locale-negotiation": slot(supported.locale_negotiation()),
        "message-locale-fallback": slot(supported.message_locale_fallback()),
        "coverage": slot(supported.coverage()),
        "intent-surface-class-vocabulary": {
            "identity": vocabulary.identity,
            "revision": vocabulary.revision,
            "semanticDigest": vocabulary.semantic_digest,
        },
        "delivery-graph": slot(supported.delivery_graph()),
        "delivery-placement": slot(supported.delivery_placement()),
    });
    serde_json::to_vec(&input).ok()
}

/// A slot's assertion of the specification `pinned`.
fn slot(pinned: &VersionedIdentity<String>) -> Value {
    json!({ "identity": pinned.identity, "revision": pinned.revision })
}

/// The sealed Verification Input pinning `selected`, and the assertion trusted
/// setup pins it with.
fn verification(selected: &Value) -> Option<(Vec<u8>, VerificationAssertion)> {
    let pins = [reference_value(&tuple_of(selected))];
    let input = sealed(unsealed(VERIFICATION.0, VERIFICATION.1, &pins)).ok()?;
    let read: VerificationInputArtifact = serde_json::from_value(input.clone()).ok()?;
    let assertion = VerificationAssertion {
        identity: read.identity().clone(),
        revision: read.revision().clone(),
        semantic_digest: read.semantic_digest().ok()?,
    };
    Some((serde_json::to_vec(&input).ok()?, assertion))
}

/// The owner's capability for `limits` as its normalized value, with every
/// bound of the [`WIDENED`] groups at [`ROOM`].
fn capability(limits: ResolutionLimits) -> Value {
    let mut capability = ImplementationCapability::fixture_owned(limits).normalized();
    for group in WIDENED {
        if let Some(bounds) = capability[BOOTSTRAP_ONLY][group.path()].as_object_mut() {
            for bound in bounds.values_mut() {
                *bound = Value::from(ROOM.to_string());
            }
        }
    }
    capability
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::workflow::{complete, WorkflowFailure};
    use crate::boundary::capability::admit as admit_capability;
    use crate::construction::registry::check::RecordKind;
    use crate::construction::resolver::committed;
    use crate::input_limits::Bound;
    use crate::locale::fixtures::FixtureProvider;
    use crate::locale::icu4x::Icu4xData;
    use crate::locale::{FormalData, Provider};
    use crate::resolver::entry::fixtures::FILE;
    use crate::resolver::evidence_source::hex;
    use crate::resolver::invocation::{OutcomeStorage, Resolver};
    use crate::resolver::outcome::tests::{entry_views, shown};

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    /// D15:2772, 2775, 2826: on the committed resolver, each recipe's
    /// submission is admitted and resolves to an outcome under the resolver's
    /// identity: checked for the example alone, blocked for every other
    /// recipe, with a Shared projection and beside a Materialized Resolution
    /// Input Set exactly when Phase 1 materialized a value, and with the
    /// Shared domain exhausted only under `Truncated`'s bound. The file's
    /// Evidence names the source identity its owner marked non-secret,
    /// without a locator.
    #[test]
    fn each_recipe_is_admitted_and_resolves_as_declared() {
        let resolver = Resolver::new(committed(), root());
        for recipe in EvidenceRecipe::ALL {
            let raw = submission(recipe, committed(), root()).unwrap();
            let members = raw.members();
            let mut slot = None;
            let mut storage = OutcomeStorage::default();
            let submitted = raw.submitted(&members);
            let resolved = complete((
                &resolver,
                submitted,
                &raw.assertion,
                &mut slot,
                &mut storage,
            ));
            let resolution = resolved.unwrap();
            let outcome = shown(&resolution.outcome);
            assert_eq!(outcome.identity, *resolver.identity(), "{recipe:?}");
            assert_eq!(outcome.checked.is_some(), recipe.checked(), "{recipe:?}");
            let valued = !matches!(
                recipe,
                EvidenceRecipe::Malformed | EvidenceRecipe::Inadmissible
            );
            let shared = outcome.shared.as_ref();
            assert_eq!(shared.is_some(), valued, "{recipe:?}");
            let exhausted = shared.is_some_and(|(_, exhausted)| exhausted.is_some());
            assert_eq!(exhausted, recipe == EvidenceRecipe::Truncated, "{recipe:?}");
            let materialized = resolution.materialized.as_ref();
            assert_eq!(materialized.is_some(), valued, "{recipe:?}");
            if let Some(materialized) = materialized {
                assert_eq!(materialized.construction_identity(), resolver.identity());
            }
            let dependency = entry_views(&outcome, RecordKind::EntryDependency)[0];
            let source = json!({ "safe-opaque": hex(FILE.as_bytes()) });
            assert_eq!(dependency["source"], source, "{recipe:?}");
            assert!(dependency["locator"].is_null(), "{recipe:?}");
        }
    }

    /// D15:2826: a submission naming another resolver's identity is refused
    /// at the pre-invocation boundary, a benchmark failure rather than an
    /// outcome.
    #[test]
    fn a_submission_naming_another_identity_is_a_failure() {
        let resolver = Resolver::new(committed(), root());
        let mut raw = submission(EvidenceRecipe::Storefront, committed(), root()).unwrap();
        raw.reference = crate::base64url::construction_identity(&[0; 32]);
        let members = raw.members();
        let mut slot = None;
        let mut storage = OutcomeStorage::default();
        let submitted = raw.submitted(&members);
        let resolved = complete((
            &resolver,
            submitted,
            &raw.assertion,
            &mut slot,
            &mut storage,
        ));
        let Some(WorkflowFailure::Refused(failures)) = resolved.err() else {
            panic!("the boundary refuses another identity");
        };
        assert!(failures.construction_identity.is_some());
        assert_eq!((failures.common, failures.entry), (None, None));
    }

    /// The capability keeps the owner's `policyComparable` bounds and its
    /// `artifactBootstrap` and `mandatoryReserve` bounds, and sets every bound
    /// of the five `bootstrapOnly` groups the invocation reads at the room,
    /// for every recipe.
    #[test]
    fn the_capability_widens_only_the_groups_the_invocation_reads() {
        let room = Bound::new(ROOM).unwrap();
        let specification = committed().capability_specification();
        for recipe in EvidenceRecipe::ALL {
            let raw = submission(recipe, committed(), root()).unwrap();
            let admitted = admit_capability(Some(&raw.capability), &specification).unwrap();
            let owned = ImplementationCapability::fixture_owned(recipe.limits());
            assert_eq!(admitted.policy_comparable(), owned.policy_comparable());
            let bounds = admitted.bootstrap_only().bounds();
            for ((id, bound), (_, kept)) in bounds.into_iter().zip(owned.bootstrap_only().bounds())
            {
                let owners = matches!(
                    id.group,
                    BootstrapGroup::ArtifactBootstrap | BootstrapGroup::MandatoryReserve
                );
                assert_eq!(bound, if owners { kept } else { room }, "{id:?}");
            }
        }
    }

    /// D15:2790: the submitted data, read and admitted under the submitted
    /// capability, binds the pins the resolver supports and canonicalizes the
    /// locales the recipes spell as the resolver tests' fixture rows do.
    #[test]
    fn the_submitted_data_binds_the_supported_pins() {
        let raw = submission(EvidenceRecipe::Dense, committed(), root()).unwrap();
        let specification = committed().capability_specification();
        let admitted = admit_capability(Some(&raw.capability), &specification).unwrap();
        let room = Bound::new(ROOM).unwrap();
        let data = Icu4xData::read(Some(raw.manifest), &raw.payloads, room, room).unwrap();
        let provider = data.admit(&admitted.bootstrap_only().locale_data).unwrap();
        let supported = committed().locale_canonicalization();
        assert_eq!(&provider.binding().pins, &supported.entries()[0]);
        let rows = FixtureProvider::new();
        for locale in ["ja", "en", "de", "fr"] {
            assert_eq!(provider.canonicalize(locale), rows.canonicalize(locale));
        }
    }

    /// D15:2788: each programmatic recipe submits the value of the file
    /// recipe of its own name, and the marked one the example's.
    #[test]
    fn each_programmatic_recipe_submits_the_value_of_the_file_it_names() {
        for recipe in ProgrammaticRecipe::ALL {
            let named = if recipe == ProgrammaticRecipe::RejectedMarker {
                json!("storefront")
            } else {
                serde_json::to_value(recipe).unwrap()
            };
            assert_eq!(serde_json::to_value(recipe.file()).unwrap(), named);
        }
    }
}
