// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's admission of a file or programmatic Entry Admission Input Set, each
//! in a step of its own (015-172, 015-201, D15:3164).
//!
//! A file entry has two typed input slots. The `configuration-file` slot's
//! value is the raw configuration bytes with their exact logical source
//! identity, whether the adapter marks that identity non-secret, the
//! Source-content revision, and an optional locator candidate; the
//! `profile-selector-cli` slot's value is the binding's selector report. A
//! programmatic entry's `configuration-programmatic` slot's value is the
//! binding's Snapshot report with the Programmatic Entry identity, whether
//! the frontend marks it non-secret, the revision, an optional locator
//! candidate, and an optional call-site sidecar report; its
//! `profile-selector-programmatic` slot's value is the selector report. A
//! binding submits each value beside its candidate for the Safe Origin Kind
//! the slot fixes. The step borrows the Snapshot and selector reports, which
//! stay the binding's, so an interval that admits them frees neither
//! (D15:141, 2810). A mark is carried as supplied and never decides
//! admission; an adapter that supplies none leaves the identity unmarked
//! (Decision 015-185).
//!
//! Each step works like the Common Invocation Input Set's. Every value
//! boundary and every locator is read under the admitted capability's bounds,
//! so without a capability none of them is evaluated, while both origins are;
//! the step then yields no admitted set and only its origin failures, which
//! may be none. Failures come in the rank of their slots, an origin's before
//! its values'. The `configuration-programmatic` slot's value failures come in
//! the boundary table's order: `programmatic-entry-identity-revision`,
//! `programmatic-entry-snapshot`, and then
//! `programmatic-call-site-evidence-input`. The identity-revision boundary
//! compares the revision with the digest of the admitted Snapshot, so it is
//! evaluated after the Snapshot's and yields no result when the Snapshot is
//! refused. A locator candidate never fails: one that is not admitted is
//! absence in the admitted set, as an absent call-site sidecar is. The
//! construction-identity reference is admitted outside both steps.

use std::sync::Arc;

use crate::resolver::origin::SafeOriginKind;
use crate::structural::selection::SelectorInput;

use super::call_site::{self, AdmittedCallSite, CallSiteReport};
use super::capability::ImplementationCapability;
use super::locator::{self, Locator, LocatorCandidate};
use super::selector::{self, SelectorReport};
use super::snapshot::{self, AdmittedSnapshot, SnapshotReport};
use super::source::{self, SourcePair};
use super::step::{admit_origin, record, slot, Component, InputFailure, Refusal};

/// The `configuration-file` slot's value as submitted. The raw bytes are
/// always present; each other part is `None` when absent.
pub(crate) struct FileValue<'a> {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) identity: Option<&'a str>,
    /// Whether the adapter marks the identity non-secret.
    pub(crate) marked: bool,
    pub(crate) revision: Option<&'a str>,
    pub(crate) locator: Option<LocatorCandidate<'a>>,
}

/// A file Entry Admission Input Set as submitted, its slots in rank order.
pub(crate) struct SubmittedFileEntry<'a> {
    pub(crate) file: Component<'a, FileValue<'a>>,
    pub(crate) selector: Component<'a, SelectorReport<'a>>,
}

/// An admitted file Entry Admission Input Set. Each origin is the Safe Origin
/// Kind its slot fixes, so the set does not repeat it. No `Debug`: the
/// selector can still carry an arbitrary secret.
pub(crate) struct FileEntryInputs {
    pub(crate) bytes: Arc<[u8]>,
    pub(crate) source: SourcePair,
    /// Whether the adapter marked the source's identity non-secret.
    pub(crate) marked: bool,
    pub(crate) locator: Option<Locator>,
    pub(crate) selector: SelectorInput,
}

/// Two sets are equal by value, as invocation staleness compares them
/// (D15:2584): the raw bytes, the pair, the mark, the locator, and the
/// selector. The pattern names every field, so a part added to the set fails
/// to compile here.
impl PartialEq for FileEntryInputs {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            bytes,
            source,
            marked,
            locator,
            selector,
        } = self;
        *bytes == other.bytes
            && *source == other.source
            && *marked == other.marked
            && *locator == other.locator
            && *selector == other.selector
    }
}

/// Admit `submitted` under the admitted capability, `None` when it was
/// refused: the admitted set, or every failure in 015's order.
pub(crate) fn admit(
    submitted: SubmittedFileEntry<'_>,
    capability: Option<&ImplementationCapability>,
) -> Result<FileEntryInputs, Vec<InputFailure>> {
    use SafeOriginKind as Kind;

    let SubmittedFileEntry {
        file,
        selector: report,
    } = submitted;
    let mut failures = Vec::new();
    let admitted = slot(
        &mut failures,
        Kind::ConfigurationFile,
        file.origin,
        capability.map(|capability| {
            let value = &file.value;
            source::admit(value.identity, value.revision, &value.bytes, capability)
                .map(|pair| (pair, locator::file_locator(value.locator, capability)))
                .map_err(Refusal::Source)
        }),
    );
    let selected = slot(
        &mut failures,
        Kind::ProfileSelectorCli,
        report.origin,
        capability
            .map(|capability| selector::admit(report.value, capability).map_err(Refusal::Selector)),
    );
    match (admitted, selected) {
        (Some((source, locator)), Some(selector)) if failures.is_empty() => Ok(FileEntryInputs {
            bytes: file.value.bytes,
            source,
            marked: file.value.marked,
            locator,
            selector,
        }),
        _ => Err(failures),
    }
}

/// The `configuration-programmatic` slot's value as submitted. Each part is
/// `None` when absent.
pub(crate) struct ProgrammaticValue<'a> {
    pub(crate) snapshot: Option<&'a SnapshotReport>,
    pub(crate) identity: Option<&'a str>,
    /// Whether the frontend marks the Programmatic Entry identity non-secret.
    pub(crate) marked: bool,
    pub(crate) revision: Option<&'a str>,
    pub(crate) locator: Option<LocatorCandidate<'a>>,
    pub(crate) call_site: Option<CallSiteReport<'a>>,
}

/// A programmatic Entry Admission Input Set as submitted, its slots in rank
/// order.
pub(crate) struct SubmittedProgrammaticEntry<'a> {
    pub(crate) programmatic: Component<'a, ProgrammaticValue<'a>>,
    pub(crate) selector: Component<'a, SelectorReport<'a>>,
}

/// An admitted programmatic Entry Admission Input Set (D15:3168). Each origin
/// is the Safe Origin Kind its slot fixes, so the set does not repeat it. No
/// `Debug`: the Snapshot carries configuration.
pub(crate) struct ProgrammaticEntryInputs {
    pub(crate) snapshot: AdmittedSnapshot,
    /// The Programmatic Entry identity and its revision, the Snapshot's
    /// digest.
    pub(crate) source: SourcePair,
    /// Whether the frontend marked the Programmatic Entry identity non-secret.
    pub(crate) marked: bool,
    /// The Programmatic Entry locator, never the sidecar's (D15:748).
    pub(crate) locator: Option<Locator>,
    pub(crate) call_site: Option<AdmittedCallSite>,
    pub(crate) selector: SelectorInput,
}

/// Two sets are equal by value, as invocation staleness compares them
/// (D15:2584): the Snapshot and its verified digest, the pair, the mark, the
/// locator, the call-site sidecar or its absence, and the selector. The
/// pattern names every field, so a part added to the set fails to compile
/// here.
impl PartialEq for ProgrammaticEntryInputs {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            snapshot,
            source,
            marked,
            locator,
            call_site,
            selector,
        } = self;
        *snapshot == other.snapshot
            && *source == other.source
            && *marked == other.marked
            && *locator == other.locator
            && *call_site == other.call_site
            && *selector == other.selector
    }
}

/// Admit a programmatic entry under the admitted capability, `None` when it
/// was refused: the admitted set, or every failure in 015's order.
pub(crate) fn admit_programmatic(
    submitted: SubmittedProgrammaticEntry<'_>,
    capability: Option<&ImplementationCapability>,
) -> Result<ProgrammaticEntryInputs, Vec<InputFailure>> {
    use SafeOriginKind as Kind;

    let SubmittedProgrammaticEntry {
        programmatic,
        selector: report,
    } = submitted;
    let value = programmatic.value;
    // The Snapshot is admitted before the pair compared with its digest, but
    // the pair's failure is recorded first, in the boundary table's order.
    let (pair, admitted, sidecar) = match capability {
        None => (None, None, None),
        Some(capability) => {
            let admitted = snapshot::admit(value.snapshot, capability);
            let pair = admitted.as_ref().ok().map(|snapshot| {
                source::admit_entry(value.identity, value.revision, snapshot, capability)
                    .map(|pair| {
                        (
                            pair,
                            locator::programmatic_locator(value.locator, capability),
                        )
                    })
                    .map_err(Refusal::EntryIdentity)
            });
            (
                pair,
                Some(admitted.map_err(Refusal::Snapshot)),
                Some(call_site::admit(value.call_site, capability).map_err(Refusal::CallSite)),
            )
        }
    };
    let mut failures = Vec::new();
    admit_origin(
        &mut failures,
        Kind::ConfigurationProgrammatic,
        programmatic.origin,
    );
    let pair = record(&mut failures, Kind::ConfigurationProgrammatic, pair);
    let admitted = record(&mut failures, Kind::ConfigurationProgrammatic, admitted);
    let sidecar = record(&mut failures, Kind::ConfigurationProgrammatic, sidecar);
    let selected = slot(
        &mut failures,
        Kind::ProfileSelectorProgrammatic,
        report.origin,
        capability
            .map(|capability| selector::admit(report.value, capability).map_err(Refusal::Selector)),
    );
    match (pair, admitted, sidecar, selected) {
        (Some((source, locator)), Some(snapshot), Some(call_site), Some(selector))
            if failures.is_empty() =>
        {
            Ok(ProgrammaticEntryInputs {
                snapshot,
                source,
                marked: value.marked,
                locator,
                call_site,
                selector,
            })
        }
        _ => Err(failures),
    }
}

// `invocation`'s tests submit the same entries.
#[cfg(test)]
pub(crate) mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::boundary::call_site::{
        CallSiteRefusal, SpanCandidate, SubmittedCallSite, SubmittedEdge, SubmittedMapping,
    };
    use crate::boundary::locator::LocatorVariant;
    use crate::boundary::origin::OriginRefusal;
    use crate::boundary::selector::SelectorRefusal;
    use crate::boundary::snapshot::{submitted as submitted_tree, SnapshotRefusal};
    use crate::boundary::source::{call_site_source_revision, file_source_revision, SourceRefusal};
    use crate::construction::capability::{
        IDENTITY as CAPABILITY, REVISION as CAPABILITY_REVISION,
    };
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::origin::SafeOriginKind as Kind;
    use crate::snapshot::{from_json, Node};

    pub(crate) const BYTES: &[u8] = b"{\"schemaVersion\":\"0\"}\n";
    pub(crate) const IDENTITY: &str = "intlify-file-source.v0:storefront/intlify.config.json";
    const LOCATOR: &str = "storefront/intlify.config.json";

    /// Set every `evidenceMetadata` and `snapshot` bound and
    /// `maxProfileIdBytes` of the capability `value` as large as a bound can
    /// be, so every entry here is within them.
    pub(crate) fn widen_entry_bounds(value: &mut Value) {
        let largest = json!((u64::MAX - 1).to_string());
        for group in ["evidenceMetadata", "snapshot"] {
            for (_, bound) in value["bootstrapOnly"][group].as_object_mut().unwrap() {
                *bound = largest.clone();
            }
        }
        value["policyComparable"]["configurationInput"]["maxProfileIdBytes"] = largest;
    }

    /// A capability value whose bounds `edit` sets, after
    /// [`widen_entry_bounds`].
    fn capability_with(edit: impl FnOnce(&mut Value)) -> ImplementationCapability {
        let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
        widen_entry_bounds(&mut value);
        edit(&mut value);
        let constructed = VersionedIdentity {
            identity: CAPABILITY,
            revision: CAPABILITY_REVISION,
        };
        crate::boundary::capability::admit(Some(&value), &constructed).unwrap()
    }

    fn roomy() -> ImplementationCapability {
        capability_with(|_| {})
    }

    fn failure(slot: Kind, refusal: Refusal) -> InputFailure {
        InputFailure { slot, refusal }
    }

    fn locator(payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant: LocatorVariant::RelativePath,
            payload,
            non_secret: true,
        }
    }

    /// A file entry every boundary admits, each value beside its slot's own
    /// Safe Origin Kind.
    pub(crate) fn submitted<'a>(revision: &'a str, selector: &'a str) -> SubmittedFileEntry<'a> {
        SubmittedFileEntry {
            file: Component {
                value: FileValue {
                    bytes: Arc::from(BYTES),
                    identity: Some(IDENTITY),
                    marked: false,
                    revision: Some(revision),
                    locator: Some(locator(LOCATOR)),
                },
                origin: Some(Kind::ConfigurationFile.id().into()),
            },
            selector: Component {
                value: SelectorReport::String(selector),
                origin: Some(Kind::ProfileSelectorCli.id().into()),
            },
        }
    }

    fn refused(
        submitted: SubmittedFileEntry<'_>,
        capability: Option<&ImplementationCapability>,
    ) -> Vec<InputFailure> {
        admit(submitted, capability)
            .err()
            .expect("the entry is refused")
    }

    #[test]
    fn a_complete_file_entry_is_admitted_slot_by_slot() {
        let revision = file_source_revision(BYTES);
        let capability = roomy();
        let Ok(admitted) = admit(submitted(&revision, "app"), Some(&capability)) else {
            panic!("the entry is admitted");
        };
        assert_eq!(&*admitted.bytes, BYTES);
        assert_eq!(admitted.source.identity().as_str(), IDENTITY);
        assert!(!admitted.marked);
        assert_eq!(admitted.source.revision(), revision);
        assert_eq!(
            admitted.locator.as_ref().map(Locator::payload),
            Some(LOCATOR)
        );
        let bound = capability
            .policy_comparable()
            .configuration_input
            .structural
            .max_profile_id_bytes;
        assert!(admitted
            .selector
            .benchmark_matches_fixture(&SelectorInput::string("app", bound)));
    }

    /// The admitted entry keeps the mark as the adapter supplied it, and the
    /// mark never changes whether, or why, the entry is refused.
    #[test]
    fn the_mark_is_carried_as_supplied() {
        let revision = file_source_revision(BYTES);
        let capability = roomy();
        for marked in [false, true] {
            let mut entry = submitted(&revision, "app");
            entry.file.value.marked = marked;
            let Ok(admitted) = admit(entry, Some(&capability)) else {
                panic!("the entry is admitted: {marked}");
            };
            assert_eq!(admitted.marked, marked);
            let mut entry = submitted("sha256:0", "app");
            entry.file.value.marked = marked;
            assert_eq!(
                refused(entry, Some(&capability)),
                [failure(
                    Kind::ConfigurationFile,
                    Refusal::Source(SourceRefusal::UnboundRevision)
                )],
                "{marked}"
            );
        }
    }

    /// Failures come in slot rank, an origin's before its value's, and an
    /// absent or refused value never skips its origin, nor the reverse.
    #[test]
    fn every_failure_comes_in_slot_rank_with_the_origin_first() {
        let revision = file_source_revision(BYTES);
        let mut entry = submitted(&revision, "app");
        entry.file.origin = None;
        entry.file.value.identity = None;
        entry.selector = Component {
            value: SelectorReport::Unsafe,
            origin: Some("profile-selector-programmatic".into()),
        };
        assert_eq!(
            refused(entry, Some(&roomy())),
            [
                failure(
                    Kind::ConfigurationFile,
                    Refusal::Origin(OriginRefusal::Missing)
                ),
                failure(
                    Kind::ConfigurationFile,
                    Refusal::Source(SourceRefusal::MissingIdentity)
                ),
                failure(
                    Kind::ProfileSelectorCli,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                ),
                failure(
                    Kind::ProfileSelectorCli,
                    Refusal::Selector(SelectorRefusal::UnsafeHostInspection)
                ),
            ]
        );
    }

    /// Without a capability, no value or locator is read: the step yields only
    /// its origin failures, which may be none.
    #[test]
    fn a_refused_capability_leaves_both_values_unevaluated() {
        let mut entry = submitted("not a revision", "app");
        entry.file.value.identity = None;
        entry.selector.value = SelectorReport::Unsafe;
        assert_eq!(refused(entry, None), []);
        let mut entry = submitted("not a revision", "app");
        entry.selector.origin = Some(" Profile-Selector-CLI".into());
        assert_eq!(
            refused(entry, None),
            [failure(
                Kind::ProfileSelectorCli,
                Refusal::Origin(OriginRefusal::NonCanonical)
            )]
        );
    }

    /// One refused origin refuses the entry, whose values were admitted.
    #[test]
    fn a_refused_origin_alone_refuses_the_entry() {
        let revision = file_source_revision(BYTES);
        let capability = roomy();
        for slot in [Kind::ConfigurationFile, Kind::ProfileSelectorCli] {
            let mut entry = submitted(&revision, "app");
            if slot == Kind::ConfigurationFile {
                entry.file.origin = Some("configuration-programmatic".into());
            } else {
                entry.selector.origin = Some("configuration-file".into());
            }
            assert_eq!(
                refused(entry, Some(&capability)),
                [failure(
                    slot,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                )],
                "{slot:?}"
            );
        }
    }

    /// A locator candidate that is not admitted is absence in the admitted
    /// entry, never a failure.
    #[test]
    fn a_locator_that_is_not_admitted_is_absence() {
        let revision = file_source_revision(BYTES);
        let capability = roomy();
        for candidate in [None, Some(locator("C:/intlify.json")), Some(locator(""))] {
            let mut entry = submitted(&revision, "app");
            entry.file.value.locator = candidate;
            let Ok(admitted) = admit(entry, Some(&capability)) else {
                panic!("the entry is admitted: {candidate:?}");
            };
            assert_eq!(admitted.locator, None, "{candidate:?}");
        }
    }

    /// A stable configuration-root identity plus a logical source slot
    /// establishes the file's identity with no locator, and the same pair
    /// under any locator: the locator takes no part in it (D15:742, 789,
    /// 3524).
    #[test]
    fn a_root_identity_and_slot_establish_the_file_identity_without_a_locator() {
        const ROOTED: &str = "intlify-file-source.v0:root-7f3a#intlify.config";
        let revision = file_source_revision(BYTES);
        let capability = roomy();
        let mut pairs = Vec::new();
        for candidate in [
            None,
            Some(locator("intlify.config.json")),
            Some(locator("packages/web/intlify.config.json")),
            Some(locator("C:/not/a/relative/path")),
        ] {
            let mut entry = submitted(&revision, "app");
            entry.file.value.identity = Some(ROOTED);
            entry.file.value.locator = candidate;
            let Ok(admitted) = admit(entry, Some(&capability)) else {
                panic!("the entry is admitted: {candidate:?}");
            };
            assert_eq!(admitted.source.identity().as_str(), ROOTED);
            pairs.push(admitted.source);
        }
        assert!(pairs.windows(2).all(|pair| pair[0] == pair[1]));
    }

    /// Each value is read under its own bounds of the capability.
    #[test]
    fn each_value_is_read_under_its_own_capability_bounds() {
        let revision = file_source_revision(BYTES);
        let identity = capability_with(|value| {
            value["bootstrapOnly"]["evidenceMetadata"]["maxSourceIdentityBytes"] =
                json!((IDENTITY.len() - 1).to_string());
        });
        assert_eq!(
            refused(submitted(&revision, "app"), Some(&identity)),
            [failure(
                Kind::ConfigurationFile,
                Refusal::Source(SourceRefusal::OverBoundIdentity)
            )]
        );
        let selector = capability_with(|value| {
            value["policyComparable"]["configurationInput"]["maxProfileIdBytes"] = json!("2");
        });
        assert_eq!(
            refused(submitted(&revision, "app"), Some(&selector)),
            [failure(
                Kind::ProfileSelectorCli,
                Refusal::Selector(SelectorRefusal::MalformedNormalized)
            )]
        );
        let tight = capability_with(|value| {
            value["bootstrapOnly"]["evidenceMetadata"]["maxLocatorBytes"] =
                json!((LOCATOR.len() - 1).to_string());
        });
        let Ok(admitted) = admit(submitted(&revision, "app"), Some(&tight)) else {
            panic!("a locator over its bound is absence, not a failure");
        };
        assert_eq!(admitted.locator, None);
    }

    pub(crate) const ENTRY: &str = "intlify-programmatic-entry.v0:storefront";
    const ENTRY_LABEL: &str = "storefront";
    const CALL_SITE: &str = "intlify-call-site.v0:storefront/app.ts";
    const CALL_SITE_LABEL: &str = "app.ts";
    /// The call site whose `'0'`, bytes 45 to 48, the sidecar maps the
    /// Snapshot's one member's value to.
    const SOURCE: &str = "export default defineConfig({ schemaVersion: '0' })\n";

    /// The Snapshot every programmatic entry here submits, its digest, the
    /// binding's report of it, and the revision of the sidecar's source.
    pub(crate) struct Programmatic {
        tree: Node,
        pub(crate) digest: String,
        report: SnapshotReport,
        revision: String,
    }

    impl Programmatic {
        pub(crate) fn new() -> Self {
            let tree = from_json(&json!({ "schemaVersion": "0" }));
            let digest = tree.digest();
            Self {
                report: SnapshotReport::Formal {
                    root: submitted_tree(&tree),
                    digest: digest.clone(),
                },
                tree,
                digest,
                revision: call_site_source_revision(SOURCE.as_bytes()),
            }
        }

        /// A programmatic entry every boundary admits, each value beside its
        /// slot's own Safe Origin Kind.
        pub(crate) fn entry<'a>(&'a self, selector: &'a str) -> SubmittedProgrammaticEntry<'a> {
            SubmittedProgrammaticEntry {
                programmatic: Component {
                    value: ProgrammaticValue {
                        snapshot: Some(&self.report),
                        identity: Some(ENTRY),
                        marked: false,
                        revision: Some(&self.digest),
                        locator: Some(safe_label(ENTRY_LABEL)),
                        call_site: Some(self.call_site()),
                    },
                    origin: Some(Kind::ConfigurationProgrammatic.id().into()),
                },
                selector: Component {
                    value: SelectorReport::String(selector),
                    origin: Some(Kind::ProfileSelectorProgrammatic.id().into()),
                },
            }
        }

        /// A sidecar the call-site boundary admits, marked non-secret.
        fn call_site(&self) -> CallSiteReport<'_> {
            CallSiteReport::Formal(SubmittedCallSite {
                identity: Some(CALL_SITE),
                marked: true,
                revision: Some(&self.revision),
                locator: Some(safe_label(CALL_SITE_LABEL)),
                source: SOURCE.as_bytes().to_vec(),
                mappings: vec![SubmittedMapping {
                    path: vec![SubmittedEdge::Key(b"schemaVersion".to_vec())],
                    role: "member-value".to_owned(),
                    span: SpanCandidate {
                        start: 45.0,
                        end: 48.0,
                    },
                }],
            })
        }
    }

    fn safe_label(payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant: LocatorVariant::SafeLabel,
            payload,
            non_secret: true,
        }
    }

    fn refused_programmatic(
        submitted: SubmittedProgrammaticEntry<'_>,
        capability: Option<&ImplementationCapability>,
    ) -> Vec<InputFailure> {
        admit_programmatic(submitted, capability)
            .err()
            .expect("the entry is refused")
    }

    /// The admitted set holds what D15:3168 lists for a programmatic entry.
    #[test]
    fn a_complete_programmatic_entry_is_admitted_slot_by_slot() {
        let programmatic = Programmatic::new();
        let capability = roomy();
        let Ok(admitted) = admit_programmatic(programmatic.entry("app"), Some(&capability)) else {
            panic!("the entry is admitted");
        };
        assert_eq!(
            admitted.snapshot.root().framed(),
            programmatic.tree.framed()
        );
        assert_eq!(admitted.snapshot.digest(), programmatic.digest);
        assert_eq!(admitted.source.identity().as_str(), ENTRY);
        assert!(!admitted.marked);
        assert_eq!(admitted.source.revision(), programmatic.digest);
        assert_eq!(
            admitted.locator.as_ref().map(Locator::payload),
            Some(ENTRY_LABEL)
        );
        let call_site = admitted
            .call_site
            .as_ref()
            .expect("the sidecar is admitted");
        assert_eq!(call_site.pair().identity().as_str(), CALL_SITE);
        assert_eq!(call_site.pair().revision(), programmatic.revision);
        assert!(call_site.marked());
        assert_eq!(
            call_site.locator().map(Locator::payload),
            Some(CALL_SITE_LABEL)
        );
        assert_eq!(call_site.source(), SOURCE);
        assert_eq!(
            call_site
                .mappings()
                .map(|(_, _, span)| span)
                .collect::<Vec<_>>(),
            [SpanCandidate {
                start: 45.0,
                end: 48.0
            }]
        );
        let bound = capability
            .policy_comparable()
            .configuration_input
            .structural
            .max_profile_id_bytes;
        assert!(admitted
            .selector
            .benchmark_matches_fixture(&SelectorInput::string("app", bound)));
    }

    /// The step borrows the binding's Snapshot and selector reports, so the
    /// same submission is admitted alike again (D15:141, 2810).
    #[test]
    fn the_reports_stay_the_bindings_and_admit_alike_again() {
        let programmatic = Programmatic::new();
        let capability = roomy();
        let first = admit_programmatic(programmatic.entry("app"), Some(&capability));
        let second = admit_programmatic(programmatic.entry("app"), Some(&capability));
        assert!(first.is_ok() && first == second);
        let SnapshotReport::Formal { digest, .. } = &programmatic.report else {
            unreachable!()
        };
        assert_eq!(*digest, programmatic.digest);
    }

    /// The admitted entry keeps the Programmatic Entry identity's mark as the
    /// frontend supplied it, and the mark never changes whether, or why, the
    /// entry is refused.
    #[test]
    fn the_programmatic_mark_is_carried_as_supplied() {
        let programmatic = Programmatic::new();
        let capability = roomy();
        for marked in [false, true] {
            let mut entry = programmatic.entry("app");
            entry.programmatic.value.marked = marked;
            let Ok(admitted) = admit_programmatic(entry, Some(&capability)) else {
                panic!("the entry is admitted: {marked}");
            };
            assert_eq!(admitted.marked, marked);
            let mut entry = programmatic.entry("app");
            entry.programmatic.value.marked = marked;
            entry.programmatic.value.revision = Some("sha256:0");
            assert_eq!(
                refused_programmatic(entry, Some(&capability)),
                [failure(
                    Kind::ConfigurationProgrammatic,
                    Refusal::EntryIdentity(SourceRefusal::UnboundRevision)
                )],
                "{marked}"
            );
        }
    }

    /// Failures come in slot rank, an origin's before its values', and the
    /// `configuration-programmatic` values' in the boundary table's order:
    /// the pair, or the Snapshot when it is refused, and then the sidecar.
    #[test]
    fn every_programmatic_failure_comes_in_slot_rank_and_table_order() {
        let programmatic = Programmatic::new();
        let capability = roomy();
        let selector = [
            failure(
                Kind::ProfileSelectorProgrammatic,
                Refusal::Origin(OriginRefusal::EvidenceKindIncompatible),
            ),
            failure(
                Kind::ProfileSelectorProgrammatic,
                Refusal::Selector(SelectorRefusal::UnsafeHostInspection),
            ),
        ];
        let failing = |edit: &dyn Fn(&mut ProgrammaticValue<'_>)| {
            let mut entry = programmatic.entry("app");
            edit(&mut entry.programmatic.value);
            entry.programmatic.value.call_site = Some(CallSiteReport::Unsafe);
            entry.selector = Component {
                value: SelectorReport::Unsafe,
                origin: Some("profile-selector-cli".into()),
            };
            entry
        };
        let mut entry = failing(&|value| value.identity = None);
        entry.programmatic.origin = None;
        let mut expected = vec![
            failure(
                Kind::ConfigurationProgrammatic,
                Refusal::Origin(OriginRefusal::Missing),
            ),
            failure(
                Kind::ConfigurationProgrammatic,
                Refusal::EntryIdentity(SourceRefusal::MissingIdentity),
            ),
            failure(
                Kind::ConfigurationProgrammatic,
                Refusal::CallSite(CallSiteRefusal::UnsafeHostInspection),
            ),
        ];
        expected.extend(selector);
        assert_eq!(refused_programmatic(entry, Some(&capability)), expected);
        let mut entry = failing(&|value| value.snapshot = Some(&SnapshotReport::Unsafe));
        entry.programmatic.origin = Some(" configuration-programmatic".into());
        let mut expected = vec![
            failure(
                Kind::ConfigurationProgrammatic,
                Refusal::Origin(OriginRefusal::NonCanonical),
            ),
            failure(
                Kind::ConfigurationProgrammatic,
                Refusal::Snapshot(SnapshotRefusal::UnsafeHostInspection),
            ),
            failure(
                Kind::ConfigurationProgrammatic,
                Refusal::CallSite(CallSiteRefusal::UnsafeHostInspection),
            ),
        ];
        expected.extend(selector);
        assert_eq!(refused_programmatic(entry, Some(&capability)), expected);
    }

    /// The pair is compared with the admitted Snapshot's digest, so a refused
    /// Snapshot leaves it without a result whatever it holds, while the
    /// sidecar, which reads no Snapshot, is still admitted or refused
    /// (015-201).
    #[test]
    fn a_refused_snapshot_leaves_the_pair_without_a_result() {
        let programmatic = Programmatic::new();
        let capability = roomy();
        let other = SnapshotReport::Formal {
            root: submitted_tree(&programmatic.tree),
            digest: Node::Null.digest(),
        };
        for (report, refusal) in [
            (None, SnapshotRefusal::Missing),
            (Some(&other), SnapshotRefusal::ContentDigestMismatch),
        ] {
            let mut entry = programmatic.entry("app");
            entry.programmatic.value.snapshot = report;
            entry.programmatic.value.identity = None;
            entry.programmatic.value.revision = Some("not a revision");
            assert_eq!(
                refused_programmatic(entry, Some(&capability)),
                [failure(
                    Kind::ConfigurationProgrammatic,
                    Refusal::Snapshot(refusal)
                )],
                "{refusal:?}"
            );
        }
        let mut entry = programmatic.entry("app");
        entry.programmatic.value.snapshot = None;
        entry.programmatic.value.identity = None;
        let Some(CallSiteReport::Formal(sidecar)) = &mut entry.programmatic.value.call_site else {
            unreachable!("the entry submits a formal sidecar");
        };
        sidecar.source = vec![0xff];
        assert_eq!(
            refused_programmatic(entry, Some(&capability)),
            [
                failure(
                    Kind::ConfigurationProgrammatic,
                    Refusal::Snapshot(SnapshotRefusal::Missing)
                ),
                failure(
                    Kind::ConfigurationProgrammatic,
                    Refusal::CallSite(CallSiteRefusal::InvalidUtf8Source)
                ),
            ]
        );
    }

    /// Without a capability, no value or locator is read: the step yields only
    /// its origin failures, which may be none.
    #[test]
    fn a_refused_capability_leaves_only_the_programmatic_origins() {
        let programmatic = Programmatic::new();
        let mut entry = programmatic.entry("app");
        entry.programmatic.value.snapshot = Some(&SnapshotReport::Unsafe);
        entry.programmatic.value.identity = None;
        entry.programmatic.value.call_site = Some(CallSiteReport::Unsafe);
        entry.selector.value = SelectorReport::Unsafe;
        assert_eq!(refused_programmatic(entry, None), []);
        let mut entry = programmatic.entry("app");
        entry.programmatic.origin = Some("configuration-file".into());
        entry.selector.origin = None;
        assert_eq!(
            refused_programmatic(entry, None),
            [
                failure(
                    Kind::ConfigurationProgrammatic,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                ),
                failure(
                    Kind::ProfileSelectorProgrammatic,
                    Refusal::Origin(OriginRefusal::Missing)
                ),
            ]
        );
    }

    /// One refused origin refuses the entry, whose values were admitted.
    #[test]
    fn a_refused_programmatic_origin_alone_refuses_the_entry() {
        let programmatic = Programmatic::new();
        let capability = roomy();
        for slot in [
            Kind::ConfigurationProgrammatic,
            Kind::ProfileSelectorProgrammatic,
        ] {
            let mut entry = programmatic.entry("app");
            if slot == Kind::ConfigurationProgrammatic {
                entry.programmatic.origin = Some("configuration-file".into());
            } else {
                entry.selector.origin = Some("profile-selector-cli".into());
            }
            assert_eq!(
                refused_programmatic(entry, Some(&capability)),
                [failure(
                    slot,
                    Refusal::Origin(OriginRefusal::EvidenceKindIncompatible)
                )],
                "{slot:?}"
            );
        }
    }

    /// A Programmatic Entry locator candidate that is not admitted is absence,
    /// and each locator stays with its own source: neither fills the other's
    /// absence (D15:748, 3539).
    #[test]
    fn a_programmatic_locator_that_is_not_admitted_is_absence() {
        let programmatic = Programmatic::new();
        let capability = roomy();
        let sidecar_locator = |admitted: &ProgrammaticEntryInputs| {
            admitted
                .call_site
                .as_ref()
                .and_then(AdmittedCallSite::locator)
                .map(|locator| locator.payload().to_owned())
        };
        for candidate in [
            None,
            Some(LocatorCandidate {
                variant: LocatorVariant::RelativePath,
                ..safe_label(ENTRY_LABEL)
            }),
            Some(safe_label("")),
            Some(LocatorCandidate {
                non_secret: false,
                ..safe_label(ENTRY_LABEL)
            }),
        ] {
            let mut entry = programmatic.entry("app");
            entry.programmatic.value.locator = candidate;
            let Ok(admitted) = admit_programmatic(entry, Some(&capability)) else {
                panic!("the entry is admitted: {candidate:?}");
            };
            assert_eq!(admitted.locator, None, "{candidate:?}");
            assert_eq!(sidecar_locator(&admitted), Some(CALL_SITE_LABEL.to_owned()));
        }
        let mut entry = programmatic.entry("app");
        let Some(CallSiteReport::Formal(sidecar)) = &mut entry.programmatic.value.call_site else {
            unreachable!("the entry submits a formal sidecar");
        };
        sidecar.locator = None;
        let Ok(admitted) = admit_programmatic(entry, Some(&capability)) else {
            panic!("the entry is admitted");
        };
        assert_eq!(
            admitted.locator.as_ref().map(Locator::payload),
            Some(ENTRY_LABEL)
        );
        assert_eq!(sidecar_locator(&admitted), None);
    }

    /// No sidecar is absence in the admitted entry, never a failure.
    #[test]
    fn an_absent_sidecar_is_admitted_as_absence() {
        let programmatic = Programmatic::new();
        let mut entry = programmatic.entry("app");
        entry.programmatic.value.call_site = None;
        let Ok(admitted) = admit_programmatic(entry, Some(&roomy())) else {
            panic!("the entry is admitted");
        };
        assert!(admitted.call_site.is_none());
    }

    /// Each programmatic value is read under its own bounds of the capability.
    #[test]
    fn each_programmatic_value_is_read_under_its_own_capability_bounds() {
        let programmatic = Programmatic::new();
        let bounded = |group: &'static str, name: &'static str, value: usize| {
            capability_with(move |raw| {
                raw["bootstrapOnly"][group][name] = json!(value.to_string());
            })
        };
        for (capability, refusal) in [
            (
                bounded("snapshot", "maxNodes", 1),
                Refusal::Snapshot(SnapshotRefusal::EnvelopeOverBound),
            ),
            (
                bounded(
                    "evidenceMetadata",
                    "maxSourceIdentityBytes",
                    ENTRY.len() - 1,
                ),
                Refusal::EntryIdentity(SourceRefusal::OverBoundIdentity),
            ),
            (
                bounded(
                    "evidenceMetadata",
                    "maxCallSiteSourceBytes",
                    SOURCE.len() - 1,
                ),
                Refusal::CallSite(CallSiteRefusal::OverBoundInput),
            ),
        ] {
            assert_eq!(
                refused_programmatic(programmatic.entry("app"), Some(&capability)),
                [failure(Kind::ConfigurationProgrammatic, refusal)],
                "{refusal:?}"
            );
        }
        let selector = capability_with(|value| {
            value["policyComparable"]["configurationInput"]["maxProfileIdBytes"] = json!("2");
        });
        assert_eq!(
            refused_programmatic(programmatic.entry("app"), Some(&selector)),
            [failure(
                Kind::ProfileSelectorProgrammatic,
                Refusal::Selector(SelectorRefusal::MalformedNormalized)
            )]
        );
        let tight = bounded("evidenceMetadata", "maxLocatorBytes", ENTRY_LABEL.len() - 1);
        let Ok(admitted) = admit_programmatic(programmatic.entry("app"), Some(&tight)) else {
            panic!("a locator over its bound is absence, not a failure");
        };
        assert_eq!(admitted.locator, None);
        // The sidecar's shorter locator is within the same bound.
        assert_eq!(
            admitted
                .call_site
                .as_ref()
                .and_then(AdmittedCallSite::locator)
                .map(Locator::payload),
            Some(CALL_SITE_LABEL)
        );
    }
}
