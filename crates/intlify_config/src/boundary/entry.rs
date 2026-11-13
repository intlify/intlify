// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's admission of a file Entry Admission Input Set in a step of its own
//! (015-172, D15:3159).
//!
//! A file entry has two typed input slots. The `configuration-file` slot's
//! value is the raw configuration bytes with their exact logical source
//! identity, whether the adapter marks that identity non-secret, the
//! Source-content revision, and an optional locator candidate; the
//! `profile-selector-cli` slot's value is the binding's selector report. A
//! binding submits each beside its candidate for the Safe Origin Kind the slot
//! fixes. The mark is carried as supplied and never decides admission; an
//! adapter that supplies none leaves the identity unmarked (Decision 015-185).
//!
//! The step works like the Common Invocation Input Set's. The source pair, the
//! locator, and the selector are read under the admitted capability's bounds,
//! so without a capability none of them is evaluated, while both origins are;
//! the step then yields no admitted set and only its origin failures, which
//! may be none. Failures come in the rank of their slots, `configuration-file`
//! and then `profile-selector-cli`, an origin's before its value's. A locator
//! candidate never fails: one that is not admitted is absence in the admitted
//! set. The construction-identity reference is admitted outside this step,
//! and a programmatic entry has no step yet (015-172).

use std::sync::Arc;

use crate::resolver::origin::SafeOriginKind;
use crate::structural::selection::SelectorInput;

use super::capability::ImplementationCapability;
use super::locator::{self, Locator, LocatorCandidate};
use super::selector::{self, SelectorReport};
use super::source::{self, SourcePair};
use super::step::{slot, Component, InputFailure, Refusal};

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
    pub(crate) selector: Component<'a, SelectorReport>,
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

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::boundary::locator::LocatorVariant;
    use crate::boundary::origin::OriginRefusal;
    use crate::boundary::selector::SelectorRefusal;
    use crate::boundary::source::{file_source_revision, SourceRefusal};
    use crate::construction::capability::{
        IDENTITY as CAPABILITY, REVISION as CAPABILITY_REVISION,
    };
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::origin::SafeOriginKind as Kind;

    const BYTES: &[u8] = b"{\"schemaVersion\":\"0\"}\n";
    const IDENTITY: &str = "intlify-file-source.v0:storefront/intlify.config.json";
    const LOCATOR: &str = "storefront/intlify.config.json";

    /// A capability value whose bounds `edit` sets, every `evidenceMetadata`
    /// bound and `maxProfileIdBytes` first as large as a bound can be.
    fn capability_with(edit: impl FnOnce(&mut Value)) -> ImplementationCapability {
        let mut value = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
        let largest = json!((u64::MAX - 1).to_string());
        for (_, bound) in value["bootstrapOnly"]["evidenceMetadata"]
            .as_object_mut()
            .unwrap()
        {
            *bound = largest.clone();
        }
        value["policyComparable"]["configurationInput"]["maxProfileIdBytes"] = largest;
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
    fn submitted<'a>(revision: &'a str, selector: &str) -> SubmittedFileEntry<'a> {
        SubmittedFileEntry {
            file: Component {
                value: FileValue {
                    bytes: Arc::from(BYTES),
                    identity: Some(IDENTITY),
                    marked: false,
                    revision: Some(revision),
                    locator: Some(locator(LOCATOR)),
                },
                origin: Some(Kind::ConfigurationFile.id()),
            },
            selector: Component {
                value: SelectorReport::String(selector.to_owned()),
                origin: Some(Kind::ProfileSelectorCli.id()),
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
            origin: Some("profile-selector-programmatic"),
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
        entry.selector.origin = Some(" Profile-Selector-CLI");
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
                entry.file.origin = Some("configuration-programmatic");
            } else {
                entry.selector.origin = Some("configuration-file");
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
}
