// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's invocation staleness: how a later retained invocation compares with
//! an earlier one, each the construction its resolver was built from and the
//! Resolver Invocation Input Set admitted on it (D15:194, 2529; Decision
//! 015-129).
//!
//! It is one function of the two retained sets, never of a stored key, and
//! compares the constructions first (D15:2582, 2590): another Resolver
//! Construction Identity is `construction-stale`, and an equal one over
//! authorities that differ in what it commits is `corrupt`
//! ([`construction::compare`]). Under one construction, another Resolver
//! Construction Identity reference is construction staleness too (D15:2586).
//!
//! Then the two sets are compared by value, every logical field and
//! occurrence, whether or not entry materialization succeeded (D15:2584): the
//! sets' own equality, whose patterns name every field, the collection as the
//! multiset of its members' `C(member)` bytes and a sidecar's span candidates
//! as numbers. The sets retain no Safe Origin Kind, since every admitted
//! origin is the one its slot fixes, so equal sets of one entry kind have
//! equal origins and a file and a programmatic entry differ. A value
//! represented otherwise, such as a collection, Specification Input members,
//! or sidecar mappings in another order, or a manifest written with other
//! bytes, is the same value, while another multiplicity is not (D15:2586).
//!
//! Equal sets are `fresh`. A difference is `corrupt`, which forbids reuse,
//! when one exact identity names two contents or carries two marks: an equal
//! identity/revision pair over unequal raw bytes, Snapshot, or call-site
//! source, or one exact identity, compared as its string in whichever role
//! either set supplies it, with unequal non-secret marks (D15:735, 2586,
//! 2590). Any other difference is `invocation-stale`. A difference within one
//! set, such as an entry and its sidecar marking one identity unequally, is
//! the boundary's to judge, so a set is always `fresh` against an equal one.

use crate::boundary::invocation::{EntryInputs, InvocationInputs};
use crate::boundary::source::ExactIdentity;
use crate::construction::root::ConstructionRoot;
use crate::construction::staleness::{
    self as construction, ConstructionComparison, ConstructionContext,
};

/// How a later invocation compares with an earlier one, by 015's names
/// (D15:2590; plan 025 decision #124).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Staleness {
    /// One construction, and every logical field and occurrence equal.
    Fresh,
    /// Another construction, or another Resolver Construction Identity
    /// reference: the resolver is constructed again.
    ConstructionStale,
    /// One construction, and a logical field or occurrence differs: the
    /// invocation runs again.
    InvocationStale,
    /// Construction-authority or input corruption, which forbids reuse.
    Corrupt,
}

impl Staleness {
    /// The name as 015 spells it.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Fresh => "fresh",
            Self::ConstructionStale => "construction-stale",
            Self::InvocationStale => "invocation-stale",
            Self::Corrupt => "corrupt",
        }
    }
}

/// One retained invocation as staleness compares it: the construction of the
/// resolver its set was admitted on, and the set.
pub(crate) struct Retained<'a, D> {
    pub(crate) construction: ConstructionContext<'a>,
    pub(crate) invocation: &'a InvocationInputs<D>,
}

/// Compare two retained invocations, both constructed under `root`.
pub(crate) fn compare<D: Eq>(
    root: &ConstructionRoot,
    earlier: &Retained<'_, D>,
    later: &Retained<'_, D>,
) -> Staleness {
    match construction::compare(root, earlier.construction, later.construction) {
        ConstructionComparison::Stale => return Staleness::ConstructionStale,
        ConstructionComparison::Corrupt => return Staleness::Corrupt,
        ConstructionComparison::Same => {}
    }
    let (earlier, later) = (earlier.invocation, later.invocation);
    if earlier.construction_identity != later.construction_identity {
        return Staleness::ConstructionStale;
    }
    if earlier == later {
        Staleness::Fresh
    } else if corrupt(&earlier.entry, &later.entry) {
        Staleness::Corrupt
    } else {
        Staleness::InvocationStale
    }
}

/// Whether one exact identity names two contents, or carries two marks,
/// across the two entries.
fn corrupt(earlier: &EntryInputs, later: &EntryInputs) -> bool {
    other_content(earlier, later) || other_marks(earlier, later)
}

/// Whether an equal identity/revision pair names unequal content: a file
/// source's raw bytes, a Programmatic Entry's Snapshot, or a call-site
/// source's UTF-8 bytes. Each revision is derived from its own content under
/// its own domain, so a pair is compared with the other set's pair of the
/// same role (D15:2586).
fn other_content(earlier: &EntryInputs, later: &EntryInputs) -> bool {
    match (earlier, later) {
        (EntryInputs::File(left), EntryInputs::File(right)) => {
            left.source == right.source && left.bytes != right.bytes
        }
        (EntryInputs::Programmatic(left), EntryInputs::Programmatic(right)) => {
            let call_site = match (&left.call_site, &right.call_site) {
                (Some(left), Some(right)) => {
                    left.pair() == right.pair() && left.source() != right.source()
                }
                _ => false,
            };
            (left.source == right.source && left.snapshot != right.snapshot) || call_site
        }
        _ => false,
    }
}

/// Whether an exact identity that both entries supply, in any role, carries
/// unequal marks.
fn other_marks(earlier: &EntryInputs, later: &EntryInputs) -> bool {
    marks(earlier).any(|(identity, marked)| {
        marks(later).any(|(other, mark)| identity.as_str() == other.as_str() && marked != mark)
    })
}

/// Every exact identity `entry` supplies, with whether it is marked
/// non-secret: a file's source, or a Programmatic Entry and its call-site
/// source when it has a sidecar.
fn marks(entry: &EntryInputs) -> impl Iterator<Item = (&ExactIdentity, bool)> {
    let (first, call_site) = match entry {
        EntryInputs::File(file) => ((file.source.identity(), file.marked), None),
        EntryInputs::Programmatic(programmatic) => (
            (programmatic.source.identity(), programmatic.marked),
            programmatic
                .call_site
                .as_ref()
                .map(|sidecar| (sidecar.pair().identity(), sidecar.marked())),
        ),
    };
    std::iter::once(first).chain(call_site)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value};

    use super::*;
    use crate::boundary::call_site::tests::resourced;
    use crate::boundary::call_site::{AdmittedCallSite, SpanCandidate, SubmittedMapping};
    use crate::boundary::entry::{FileEntryInputs, ProgrammaticEntryInputs};
    use crate::boundary::snapshot::AdmittedSnapshot;
    use crate::boundary::source::{self, file_source_revision};
    use crate::construction::component::Artifact;
    use crate::construction::flow::construct;
    use crate::construction::identity::derive;
    use crate::construction::resolver::committed;
    use crate::resolver::call_site_span::fixtures::{identified, mirrored, CALL_SITE};
    use crate::resolver::entry::fixtures::FILE;
    use crate::resolver::invocation::tests::{
        example, materialized, resolved, resolver, seal, PinnedFixture, Raw, ROOM,
    };
    use crate::resolver::invocation::Resolver;
    use crate::resolver::snapshot_entry::fixtures::ENTRY;
    use crate::snapshot::{from_json, Marker, MarkerReason, Node};
    use Staleness::{ConstructionStale, Corrupt, Fresh, InvocationStale};

    type Inputs = InvocationInputs<PinnedFixture>;

    /// Another identity in each role.
    const OTHER_FILE: &str = "intlify-file-source.v0:storefront/other.config.json";
    const OTHER_ENTRY: &str = "intlify-programmatic-entry.v0:admin";
    const OTHER_CALL_SITE: &str = "intlify-call-site.v0:storefront/admin.ts";

    /// A raw invocation's file, its identity marked, and a Programmatic Entry
    /// of its value beside a mirroring sidecar.
    const ENTRIES: [fn(&Raw, &Resolver<'_>) -> Inputs; 2] = [
        |raw, resolver| raw.file(resolver, true),
        |raw, resolver| raw.programmatic(resolver, true, true),
    ];

    fn root() -> &'static ConstructionRoot {
        ConstructionRoot::built_in().unwrap()
    }

    /// How `later` under the second of `contexts` compares with `earlier`
    /// under the first.
    fn across<D: Eq>(
        [first, second]: [ConstructionContext<'_>; 2],
        earlier: &InvocationInputs<D>,
        later: &InvocationInputs<D>,
    ) -> Staleness {
        let retained = |construction, invocation| Retained {
            construction,
            invocation,
        };
        compare(root(), &retained(first, earlier), &retained(second, later))
    }

    /// How `later` compares with `earlier`, both on the committed resolver.
    fn staleness<D: Eq>(earlier: &InvocationInputs<D>, later: &InvocationInputs<D>) -> Staleness {
        let context = committed().context();
        across([context, context], earlier, later)
    }

    /// `raw` after `edit`, admitted as `entry` on `resolver`.
    fn edited(entry: fn(&Raw, &Resolver<'_>) -> Inputs, edit: fn(&mut Raw)) -> Inputs {
        let mut raw = example(|inputs| inputs);
        edit(&mut raw);
        entry(&raw, &resolver())
    }

    fn file(inputs: &mut Inputs) -> &mut FileEntryInputs {
        let EntryInputs::File(file) = &mut inputs.entry else {
            panic!("a file entry");
        };
        file
    }

    fn programmatic(inputs: &mut Inputs) -> &mut ProgrammaticEntryInputs {
        let EntryInputs::Programmatic(programmatic) = &mut inputs.entry else {
            panic!("a programmatic entry");
        };
        programmatic
    }

    /// The JSON value of `bytes` after `edit`, written back with other
    /// whitespace when `pretty`.
    fn rewrite(bytes: &mut Vec<u8>, pretty: bool, edit: impl FnOnce(&mut Value)) {
        let mut value: Value = serde_json::from_slice(bytes).unwrap();
        edit(&mut value);
        let written = if pretty {
            serde_json::to_vec_pretty(&value)
        } else {
            serde_json::to_vec(&value)
        };
        *bytes = written.unwrap();
    }

    /// The configuration's text, which the example's sidecar mirrors.
    fn text(raw: &Raw) -> String {
        serde_json::to_string(&raw.config).unwrap()
    }

    /// A sidecar under `identity`, marked when `marked`, over `text` with the
    /// locator `label` and its mirroring mappings after `edit`.
    fn sidecar(
        identity: &str,
        marked: bool,
        text: &str,
        label: Option<&str>,
        edit: impl FnOnce(&mut Vec<SubmittedMapping>),
    ) -> AdmittedCallSite {
        let mut mappings = mirrored(text);
        edit(&mut mappings);
        identified(identity, marked, text, label, mappings)
    }

    /// `raw`'s Programmatic Entry, its identity marked, beside `call_site`.
    fn beside(raw: &Raw, resolver: &Resolver<'_>, call_site: Option<AdmittedCallSite>) -> Inputs {
        let mut inputs = raw.programmatic(resolver, true, true);
        programmatic(&mut inputs).call_site = call_site;
        inputs
    }

    /// D15:2586, 2590: two admissions of one invocation are `fresh`, and so
    /// is one represented otherwise: its collection in another order, a
    /// member, the vocabulary, or the Verification Input written with other
    /// bytes, the Specification Input's members in another order, or a
    /// sidecar admitted under other bounds with its mappings in another order.
    #[test]
    fn equal_sets_and_other_representations_are_fresh() {
        let edits: [fn(&mut Raw); 6] = [
            |_| {},
            |raw| raw.members.reverse(),
            |raw| rewrite(&mut raw.members[0], true, |_| {}),
            |raw| rewrite(&mut raw.vocabulary, true, |_| {}),
            |raw| rewrite(&mut raw.verification, true, |_| {}),
            |raw| {
                rewrite(&mut raw.specification, false, |input| {
                    *input = input
                        .as_object()
                        .unwrap()
                        .clone()
                        .into_iter()
                        .rev()
                        .collect();
                });
            },
        ];
        for entry in ENTRIES {
            let earlier = edited(entry, |_| {});
            for (index, edit) in edits.into_iter().enumerate() {
                assert_eq!(staleness(&earlier, &edited(entry, edit)), Fresh, "{index}");
            }
        }
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let earlier = raw.programmatic(&resolver, true, true);
        let reversed = sidecar(CALL_SITE, true, &text(&raw), None, |mappings| {
            mappings.reverse();
        });
        assert_eq!(
            staleness(&earlier, &beside(&raw, &resolver, Some(reversed))),
            Fresh
        );
    }

    /// D15:1147, 1664, 1670, 2584: another value of any common component,
    /// the collection's multiplicity included, or another configuration
    /// value or selector makes either entry's invocation stale.
    #[test]
    fn each_value_both_entries_hold_is_compared() {
        let edits: [fn(&mut Raw); 8] = [
            |raw| raw.specification = b"{}".to_vec(),
            |raw| {
                rewrite(&mut raw.vocabulary, false, |value| {
                    value["members"] = json!([]);
                });
            },
            |raw| raw.provider.binding.admission.artifact.push('x'),
            |raw| raw.members.push(raw.members[1].clone()),
            |raw| {
                let mut input: Value = serde_json::from_slice(&raw.verification).unwrap();
                input["revision"] = json!("2");
                (raw.verification, raw.assertion) = seal(input);
            },
            |raw| raw.config["profiles"]["web"]["defaultSourceLocale"] = json!("ja"),
            |raw| "app".clone_into(&mut raw.selected),
            |raw| raw.members.clear(),
        ];
        for entry in ENTRIES {
            let earlier = edited(entry, |_| {});
            for (index, edit) in edits.into_iter().enumerate() {
                let later = edited(entry, edit);
                assert_eq!(staleness(&earlier, &later), InvocationStale, "{index}");
            }
            let bounded = example(|inputs| inputs).bounded("rawInput.maxFileBytes", ROOM - 1);
            let later = entry(&bounded, &resolver());
            assert_eq!(staleness(&earlier, &later), InvocationStale);
        }
    }

    /// D15:2584: a file entry's identity, alone or with its mark, and its
    /// locator are compared, and a file and a Snapshot of its value differ,
    /// at least in their fixed Safe Origin Kinds.
    #[test]
    fn each_file_field_and_the_entry_kind_are_compared() {
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let earlier = raw.file(&resolver, true);
        let capability = &earlier.common.capability;
        for (index, (identity, marked)) in [(OTHER_FILE, true), (OTHER_FILE, false), (FILE, true)]
            .into_iter()
            .enumerate()
        {
            let mut later = raw.file(&resolver, marked);
            let file = file(&mut later);
            let revision = file_source_revision(&file.bytes);
            file.source =
                source::admit(Some(identity), Some(&revision), &file.bytes, capability).unwrap();
            if identity == FILE {
                file.locator = None;
            }
            assert_eq!(staleness(&earlier, &later), InvocationStale, "{index}");
        }
        let snapshot = raw.programmatic(&resolver, true, false);
        assert_eq!(staleness(&earlier, &snapshot), InvocationStale);
        assert_eq!(staleness(&snapshot, &earlier), InvocationStale);
    }

    /// D15:2584, 3574: a Programmatic Entry's identity, alone or with its
    /// mark, and its locator are compared, and so are its sidecar's presence,
    /// identity alone or with its mark, locator, source, mappings, and
    /// candidates.
    #[test]
    fn each_programmatic_field_is_compared() {
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let earlier = raw.programmatic(&resolver, true, true);
        let capability = &earlier.common.capability;
        for (index, (identity, marked)) in
            [(OTHER_ENTRY, true), (OTHER_ENTRY, false), (ENTRY, true)]
                .into_iter()
                .enumerate()
        {
            let mut later = raw.programmatic(&resolver, marked, true);
            let entry = programmatic(&mut later);
            let digest = entry.snapshot.digest();
            entry.source =
                source::admit_entry(Some(identity), Some(digest), &entry.snapshot, capability)
                    .unwrap();
            if identity == ENTRY {
                entry.locator = None;
            }
            assert_eq!(staleness(&earlier, &later), InvocationStale, "{index}");
        }
        let (text, pretty) = (
            text(&raw),
            serde_json::to_string_pretty(&raw.config).unwrap(),
        );
        let mirroring =
            |identity, marked, text, label| sidecar(identity, marked, text, label, |_| {});
        let edited =
            |edit: fn(&mut Vec<SubmittedMapping>)| sidecar(CALL_SITE, true, &text, None, edit);
        let sidecars = [
            None,
            Some(mirroring(OTHER_CALL_SITE, true, &text, None)),
            Some(mirroring(OTHER_CALL_SITE, false, &text, None)),
            Some(mirroring(CALL_SITE, true, &text, Some("app.ts"))),
            Some(mirroring(CALL_SITE, true, &pretty, None)),
            Some(edited(|mappings| mappings.truncate(1))),
            Some(edited(|mappings| {
                mappings[0].role.replace_range(.., "member-key");
            })),
            Some(edited(|mappings| mappings[0].span.start += 1.0)),
            Some(edited(|mappings| mappings[0].span.end += 1.0)),
        ];
        for (index, call_site) in sidecars.into_iter().enumerate() {
            let later = beside(&raw, &resolver, call_site);
            assert_eq!(staleness(&earlier, &later), InvocationStale, "{index}");
        }
    }

    /// Plan 025 decision #306: span candidates compare as numbers, so a
    /// sidecar with NaN candidates is `fresh` against itself, whatever NaN it
    /// holds, `-0` is `+0`, and NaN is no other number.
    #[test]
    fn span_candidates_compare_as_numbers() {
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let text = text(&raw);
        let with = |start, end| {
            let span = SpanCandidate { start, end };
            let edit = |mappings: &mut Vec<SubmittedMapping>| mappings[0].span = span;
            beside(
                &raw,
                &resolver,
                Some(sidecar(CALL_SITE, true, &text, None, edit)),
            )
        };
        let nan = with(f64::NAN, f64::NAN);
        assert_eq!(staleness(&nan, &nan), Fresh);
        assert_eq!(staleness(&nan, &with(f64::NAN, -f64::NAN)), Fresh);
        assert_eq!(staleness(&with(-0.0, 1.0), &with(0.0, 1.0)), Fresh);
        assert_eq!(staleness(&with(1.0, 0.0), &with(1.0, -0.0)), Fresh);
        assert_eq!(staleness(&nan, &with(0.0, f64::NAN)), InvocationStale);
    }

    /// D15:2582, 2586, 2590: the constructions are compared before the
    /// invocations, whether those are equal, differ, or are corrupt: another
    /// Resolver Construction Identity is `construction-stale`, and other
    /// committed bytes under one identity are `corrupt`. Another reference
    /// under one construction is `construction-stale` too. Revision `"0"`
    /// admits one set of authorities, so the other constructions are made
    /// directly.
    #[test]
    fn the_constructions_are_compared_first() {
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let own = committed().context();
        let mut admitted = construct(root(), committed().input()).unwrap();
        admitted.registry_package.declared.revision = "1".to_owned();
        let other = ConstructionContext {
            input: own.input,
            admitted: &admitted,
        };
        let mut input = own.input.clone();
        let schema = input.profile_specification_schema.as_mut().unwrap();
        let mut bytes = schema.artifact.bytes().to_vec();
        bytes.push(b' ');
        schema.artifact = Artifact::read(bytes);
        let corrupted = ConstructionContext {
            input: &input,
            admitted: own.admitted,
        };
        let earlier = raw.file(&resolver, true);
        let mut stale = raw.file(&resolver, true);
        file(&mut stale).locator = None;
        for later in [raw.file(&resolver, true), stale, raw.file(&resolver, false)] {
            assert_eq!(across([own, other], &earlier, &later), ConstructionStale);
            assert_eq!(across([other, own], &later, &earlier), ConstructionStale);
            assert_eq!(across([own, corrupted], &earlier, &later), Corrupt);
        }
        let mut referenced = raw.file(&resolver, true);
        referenced.construction_identity = derive(root(), &admitted);
        assert!(referenced != earlier);
        assert_eq!(staleness(&earlier, &referenced), ConstructionStale);
    }

    /// D15:2586, 2590: an equal pair over other raw bytes, another Snapshot,
    /// or another call-site source is `corrupt`. Each boundary derives its
    /// revision from that content, so only a revision collision gives such a
    /// set, and these are built directly.
    #[test]
    fn an_equal_pair_over_other_content_is_corrupt() {
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let earlier = raw.file(&resolver, true);
        let mut later = raw.file(&resolver, true);
        file(&mut later).bytes = Arc::from(serde_json::to_vec_pretty(&raw.config).unwrap());
        assert_eq!(staleness(&earlier, &later), Corrupt);
        let earlier = raw.programmatic(&resolver, true, true);
        let mut value = raw.config.clone();
        value["profiles"]["web"]["defaultSourceLocale"] = json!("ja");
        let mut later = raw.programmatic(&resolver, true, true);
        programmatic(&mut later).snapshot = AdmittedSnapshot::of(from_json(&value));
        assert_eq!(staleness(&earlier, &later), Corrupt);
        let mut later = raw.programmatic(&resolver, true, true);
        let entry = programmatic(&mut later);
        entry.call_site = entry
            .call_site
            .take()
            .map(|sidecar| resourced(sidecar, "{}"));
        assert_eq!(staleness(&earlier, &later), Corrupt);
    }

    /// D15:735, 2586, 2590: one exact identity marked otherwise is
    /// `corrupt`, as a file source, a Programmatic Entry, or a call-site
    /// source, and across those roles, before any other difference.
    #[test]
    fn one_identity_marked_otherwise_is_corrupt() {
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let text = text(&raw);
        let file_entry = raw.file(&resolver, true);
        assert_eq!(staleness(&file_entry, &raw.file(&resolver, false)), Corrupt);
        let snapshot = raw.programmatic(&resolver, true, true);
        let unmarked = raw.programmatic(&resolver, false, true);
        assert_eq!(staleness(&snapshot, &unmarked), Corrupt);
        let call_site = |identity, marked| {
            let sidecar = sidecar(identity, marked, &text, None, |_| {});
            beside(&raw, &resolver, Some(sidecar))
        };
        assert_eq!(staleness(&snapshot, &call_site(CALL_SITE, false)), Corrupt);
        assert_eq!(staleness(&file_entry, &call_site(FILE, false)), Corrupt);
        assert_eq!(staleness(&call_site(FILE, false), &file_entry), Corrupt);
        assert_eq!(
            staleness(&file_entry, &call_site(FILE, true)),
            InvocationStale
        );
        let bounded = example(|inputs| inputs).bounded("rawInput.maxFileBytes", ROOM - 1);
        let mut later = bounded.file(&resolver, false);
        file(&mut later).locator = None;
        assert_eq!(staleness(&file_entry, &later), Corrupt);
        let mut later = raw.programmatic(&resolver, false, true);
        let shorter = |mappings: &mut Vec<SubmittedMapping>| mappings.truncate(1);
        programmatic(&mut later).call_site = Some(sidecar(CALL_SITE, true, &text, None, shorter));
        assert_eq!(staleness(&snapshot, &later), Corrupt);
    }

    /// D15:2584, 3574: a set whose Phase 1 blocked before a value existed
    /// compares as any other, its sidecar's presence included.
    #[test]
    fn a_set_whose_phase_1_blocked_still_compares() {
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let bounded = example(|inputs| inputs).bounded("rawInput.maxFileBytes", ROOM - 1);
        let no_json =
            |raw: &Raw, bytes: &[u8], marked| raw.file_of(&resolver, Arc::from(bytes), marked);
        let earlier = no_json(&raw, b"{", true);
        assert!(materialized(&resolver, &earlier).is_none());
        assert_eq!(staleness(&earlier, &no_json(&raw, b"{", true)), Fresh);
        assert_eq!(
            staleness(&earlier, &no_json(&raw, b"[", true)),
            InvocationStale
        );
        assert_eq!(
            staleness(&earlier, &no_json(&bounded, b"{", true)),
            InvocationStale
        );
        assert_eq!(staleness(&earlier, &no_json(&raw, b"{", false)), Corrupt);
        let mut root = from_json(&raw.config);
        let Node::Object(members) = &mut root else {
            panic!("the root is an object");
        };
        let marker = Marker::Rejected(MarkerReason::Cycle);
        members.insert("~".to_owned(), Node::Marker(marker));
        let earlier = raw.snapshot(&resolver, &root, true, true);
        assert!(materialized(&resolver, &earlier).is_none());
        let later = raw.snapshot(&resolver, &root, true, true);
        assert_eq!(staleness(&earlier, &later), Fresh);
        let later = raw.snapshot(&resolver, &root, true, false);
        assert_eq!(staleness(&earlier, &later), InvocationStale);
    }

    /// D15:2588, 3569: editing a declaration the selector does not select
    /// makes the file invocation stale, though the checked profile stays
    /// equal.
    #[test]
    fn an_unselected_declarations_edit_is_stale_under_an_equal_profile() {
        let resolver = resolver();
        let with_admin = |locale: &str| {
            let mut raw = example(|inputs| inputs);
            let mut admin = raw.config["profiles"]["web"].clone();
            admin["defaultSourceLocale"] = json!(locale);
            raw.config["profiles"]["admin"] = admin;
            raw.file(&resolver, true)
        };
        let (earlier, later) = (with_admin("en"), with_admin("ja"));
        assert_eq!(staleness(&earlier, &later), InvocationStale);
        let (before, after) = (resolved(&resolver, &earlier), resolved(&resolver, &later));
        assert!(before.checked.is_some());
        assert_eq!(before.checked, after.checked);
    }

    /// Plan 025 decision #124: the four names are 015's.
    #[test]
    fn the_names_are_015s() {
        let names = [Fresh, ConstructionStale, InvocationStale, Corrupt].map(Staleness::id);
        assert_eq!(
            names,
            ["fresh", "construction-stale", "invocation-stale", "corrupt"]
        );
    }

    /// D15:2586: canonicalization data whose manifest is written with other
    /// bytes is the same value, and another manifest value is not.
    #[cfg(feature = "icu4x")]
    #[test]
    fn a_manifest_written_otherwise_is_fresh() {
        use std::collections::BTreeMap;

        use crate::boundary::common::CommonInputs;
        use crate::input_limits::Bound;
        use crate::locale::icu4x::Icu4xData;

        const MANIFEST: &[u8] = include_bytes!("../../../intlify_locale_icu4x/data/manifest.json");
        let (resolver, raw) = (resolver(), example(|inputs| inputs));
        let with = |manifest: &[u8]| {
            let bound = Bound::new(1 << 20).unwrap();
            let data = Icu4xData::read(Some(manifest), &BTreeMap::new(), bound, bound).unwrap();
            let inputs = raw.file(&resolver, true);
            let common = Arc::into_inner(inputs.common).unwrap();
            let common = CommonInputs {
                specification: common.specification,
                vocabulary: common.vocabulary,
                data,
                collection: common.collection,
                verification: common.verification,
                capability: common.capability,
            };
            InvocationInputs {
                construction_identity: inputs.construction_identity,
                common: Arc::new(common),
                entry: inputs.entry,
            }
        };
        let mut manifest = MANIFEST.to_vec();
        let earlier = with(&manifest);
        rewrite(&mut manifest, true, |_| {});
        assert_ne!(manifest, MANIFEST);
        assert_eq!(staleness(&earlier, &with(&manifest)), Fresh);
        rewrite(&mut manifest, false, |value| {
            value["dataset"]["identity"] = json!("other");
        });
        assert_eq!(staleness(&earlier, &with(&manifest)), InvocationStale);
    }
}
