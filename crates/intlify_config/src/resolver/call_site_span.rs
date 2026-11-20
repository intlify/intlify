// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's call-site spans: the Portable Source Span that a programmatic
//! entry's Evidence takes from its admitted Programmatic Call-Site Evidence
//! Input, and the source that span selects (D15:745-751, 795-802).
//!
//! Evidence selects one location role where it points: `member-key` when it
//! locates a key, and when it locates a value, `member-value` where the
//! value's path ends at an object member and `node` at the root or an array
//! element. A missing member is observed at its owning object and a marker at
//! its position, so each takes the role of the value there (D15:747; Decision
//! 015-203). The sidecar's candidate for the path and that role is a span
//! only when both coordinates are integers with `start <= end <= length` over
//! the call-site source bytes, byte coordinates that need not fall on a
//! character boundary (D15:745). Any other candidate, and a path or role the
//! sidecar does not map, is absence, and no other role is consulted (D15:747,
//! 749, 751).
//!
//! A span selects the call-site source's identity and locator together;
//! without one, Evidence names the Programmatic Entry's, so neither locator is
//! borrowed for the other identity (D15:795, 796, 800).
//!
//! Phase 1 looks a position's mapping up by the position's path. The phases
//! after it name a value or a key by its locus in the materialized document,
//! so the spans are resolved once, each mapping's path walked from the root:
//! a mapping has a locus only when its path names an established node and the
//! rule selects its role there, and the walk is bounded by the mappings and
//! the document's depth (D15:747, 1833).

use std::collections::BTreeMap;

use crate::boundary::call_site::{AdmittedCallSite, LocationRole, SpanCandidate, StructuralEdge};
use crate::materialize::{Locus, MaterializedDocument, NodeKind};
use crate::structural::Edge;

use super::evidence::Source;
use super::evidence_source::SourceSpan;

/// What Evidence locates where it points: the value there, or the key of the
/// member there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Located {
    Value,
    Key,
}

impl Located {
    /// The role Evidence that locates this selects where its path ends at an
    /// object member when `member`, and otherwise at the root or an array
    /// element, which have no key (D15:747).
    pub(crate) const fn role(self, member: bool) -> Option<LocationRole> {
        match (self, member) {
            (Self::Key, true) => Some(LocationRole::MemberKey),
            (Self::Value, true) => Some(LocationRole::MemberValue),
            (Self::Value, false) => Some(LocationRole::Node),
            (Self::Key, false) => None,
        }
    }
}

/// The call-site source and the span of Evidence that locates `located` at
/// the end of the edges `path` from the root, when `call_site` maps the path
/// and the role the rule selects there to a candidate that is a span
/// (D15:747, 798, 802).
pub(crate) fn mapped<'a>(
    call_site: &'a AdmittedCallSite,
    path: &[Edge<'_>],
    located: Located,
) -> Option<(Source<'a>, SourceSpan)> {
    let role = located.role(matches!(path.last(), Some(Edge::Member(_))))?;
    let path = path
        .iter()
        .map(|edge| match *edge {
            Edge::Member(key) => StructuralEdge::Key(key.to_owned()),
            Edge::Index(index) => StructuralEdge::Index(index),
        })
        .collect();
    let span = admit(call_site.candidate(path, role)?, call_site)?;
    Some((source(call_site), span))
}

/// The source and span Evidence names: the call-site source and its span when
/// one is `mapped`, and otherwise `entry`, the Programmatic Entry's source,
/// with none (D15:795, 796, 800).
pub(crate) fn named<'a>(
    entry: Source<'a>,
    mapped: Option<(Source<'a>, SourceSpan)>,
) -> (Source<'a>, Option<SourceSpan>) {
    mapped.map_or((entry, None), |(source, span)| (source, Some(span)))
}

/// The spans an admitted sidecar gives the values and keys of a materialized
/// document, by their loci, with the call-site source they select.
pub(crate) struct CallSiteSpans<'a> {
    source: Source<'a>,
    spans: BTreeMap<Locus, SourceSpan>,
}

impl<'a> CallSiteSpans<'a> {
    /// The spans `call_site` gives `document`: each candidate that is a span,
    /// at the locus of what Evidence selecting its mapping's role locates,
    /// when the mapping's path names an established node and the rule selects
    /// the role there (D15:747, 749).
    pub(crate) fn new(call_site: &'a AdmittedCallSite, document: &MaterializedDocument) -> Self {
        let spans = call_site
            .mappings()
            .filter_map(|(path, role, candidate)| {
                Some((locus(document, path, role)?, admit(candidate, call_site)?))
            })
            .collect();
        Self {
            source: source(call_site),
            spans,
        }
    }

    /// The call-site source and the span of the value or key at `locus`, when
    /// a mapping gives it one (D15:800).
    pub(crate) fn at(&self, locus: Locus) -> Option<(Source<'a>, SourceSpan)> {
        self.spans.get(&locus).map(|&span| (self.source, span))
    }
}

/// The locus of what Evidence selecting `role` at the end of `path` locates
/// in `document`: none when the path names no established node, or when the
/// rule selects another role there, whether or not `role` applies (D15:747).
fn locus(
    document: &MaterializedDocument,
    path: &[StructuralEdge],
    role: LocationRole,
) -> Option<Locus> {
    let mut node = document.root();
    // The key of the member the path ends at, when it ends at one.
    let mut key = None;
    for edge in path {
        (node, key) = match (edge, document.node(node).kind()) {
            (StructuralEdge::Key(name), NodeKind::Object(members)) => {
                let member = members.get(name)?;
                (member.value(), Some(member.key()))
            }
            (StructuralEdge::Index(index), NodeKind::Array(elements)) => {
                (*elements.get(usize::try_from(*index).ok()?)?, None)
            }
            _ => return None,
        };
    }
    let located = if role == LocationRole::MemberKey {
        Located::Key
    } else {
        Located::Value
    };
    if located.role(key.is_some()) != Some(role) {
        return None;
    }
    match located {
        Located::Key => key,
        Located::Value => Some(document.node(node).locus()),
    }
}

/// `candidate` as a span over the bytes of `call_site`'s source: both
/// coordinates integers with `start <= end <= length`, which need not fall
/// on a character boundary; absence otherwise (D15:745, 749).
fn admit(candidate: SpanCandidate, call_site: &AdmittedCallSite) -> Option<SourceSpan> {
    let (start, end) = candidate.coordinates()?;
    SourceSpan::admit(start, end, call_site.source().as_bytes())
}

/// The call-site source: its identity, whether its frontend marked that
/// identity non-secret, and its own locator, never the Programmatic Entry's
/// (D15:747, 796).
pub(crate) fn source(call_site: &AdmittedCallSite) -> Source<'_> {
    Source {
        identity: call_site.pair().identity(),
        marked: call_site.marked(),
        locator: call_site.locator(),
    }
}

#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::Arc;

    use serde_json::Value;

    use crate::boundary::call_site::{
        read_report, AdmittedCallSite, CallSiteLimits, CallSiteReport, SpanCandidate,
        SubmittedCallSite, SubmittedEdge, SubmittedMapping,
    };
    use crate::boundary::locator::{LocatorCandidate, LocatorVariant};
    use crate::boundary::source::{call_site_source_revision, SourceLimits};
    use crate::input_limits::Bound;
    use crate::materialize::{materialize_file, Locus, MaterializedDocument, NodeId, NodeKind};
    use crate::materialize_tests::limits;

    /// The identity of the test call-site sources, which their frontend marks
    /// non-secret.
    pub(crate) const CALL_SITE: &str = "intlify-call-site.v0:storefront/app.ts";

    /// The mapping from `path`, an array of keys and indices, and `role` to
    /// `[start, end)`.
    pub(crate) fn mapping(path: &Value, role: &str, start: f64, end: f64) -> SubmittedMapping {
        let path = path
            .as_array()
            .expect("a path is an array")
            .iter()
            .map(|edge| match edge {
                Value::String(key) => SubmittedEdge::Key(key.as_bytes().to_vec()),
                index => SubmittedEdge::Index(index.as_f64().expect("an index is a number")),
            })
            .collect();
        SubmittedMapping {
            path,
            role: role.to_owned(),
            span: SpanCandidate { start, end },
        }
    }

    /// The sidecar over `source` holding `mappings`, under the identity
    /// [`CALL_SITE`] and, as its locator, the label `label` if any.
    pub(crate) fn sidecar(
        source: &str,
        label: Option<&str>,
        mappings: Vec<SubmittedMapping>,
    ) -> AdmittedCallSite {
        marked_as(true, source, label, mappings)
    }

    /// The same, its identity marked non-secret only when `marked`.
    pub(crate) fn marked_as(
        marked: bool,
        source: &str,
        label: Option<&str>,
        mappings: Vec<SubmittedMapping>,
    ) -> AdmittedCallSite {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        let limits = CallSiteLimits {
            pair: SourceLimits {
                identity_bytes: largest,
                revision_bytes: largest,
            },
            source_bytes: largest,
            mappings: largest,
            locator_bytes: largest,
        };
        let revision = call_site_source_revision(source.as_bytes());
        let submitted = SubmittedCallSite {
            identity: Some(CALL_SITE),
            marked,
            revision: Some(&revision),
            locator: label.map(|payload| LocatorCandidate {
                variant: LocatorVariant::SafeLabel,
                payload,
                non_secret: true,
            }),
            source: source.as_bytes().to_vec(),
            mappings,
        };
        let Ok(Some(admitted)) = read_report(Some(CallSiteReport::Formal(submitted)), limits)
        else {
            panic!("the sidecar is admitted");
        };
        admitted
    }

    /// The mappings that give every value and key of the file document of
    /// `text` its token's span: the root and each element under `node`, a
    /// member's value under `member-value`, and its key under `member-key`.
    pub(crate) fn mirrored(text: &str) -> Vec<SubmittedMapping> {
        let document = materialize_file(Arc::from(text.as_bytes()), limits()).unwrap();
        let mut mappings = Vec::new();
        mirror(
            &document,
            document.root(),
            &mut Vec::new(),
            "node",
            &mut mappings,
        );
        mappings
    }

    /// The mappings of `node` at `path` under `role`, and of what it holds.
    fn mirror(
        document: &MaterializedDocument,
        node: NodeId,
        path: &mut Vec<SubmittedEdge>,
        role: &str,
        mappings: &mut Vec<SubmittedMapping>,
    ) {
        let located = document.node(node);
        mappings.push(spanned(path, role, located.locus()));
        match located.kind() {
            NodeKind::Object(members) => {
                for (name, member) in members {
                    path.push(SubmittedEdge::Key(name.as_bytes().to_vec()));
                    mappings.push(spanned(path, "member-key", member.key()));
                    mirror(document, member.value(), path, "member-value", mappings);
                    path.pop();
                }
            }
            NodeKind::Array(elements) => {
                for (index, &element) in (0_u32..).zip(elements) {
                    path.push(SubmittedEdge::Index(f64::from(index)));
                    mirror(document, element, path, "node", mappings);
                    path.pop();
                }
            }
            NodeKind::Null | NodeKind::Boolean(_) | NodeKind::Number(_) | NodeKind::String(_) => {}
        }
    }

    /// The mapping from `path` and `role` to the file token at `locus`.
    fn spanned(path: &[SubmittedEdge], role: &str, locus: Locus) -> SubmittedMapping {
        let span = locus.bytes().expect("a file locus has bytes");
        let coordinate = |byte: u64| f64::from(u32::try_from(byte).unwrap());
        SubmittedMapping {
            path: path.to_vec(),
            role: role.to_owned(),
            span: SpanCandidate {
                start: coordinate(span.start_byte()),
                end: coordinate(span.end_byte()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::{json, Value};

    use crate::boundary::call_site::{LocationRole, SpanCandidate};
    use crate::boundary::locator::{
        read_programmatic_locator, Locator, LocatorCandidate, LocatorVariant,
    };
    use crate::boundary::source::{read_entry_pair, SourceLimits};
    use crate::input_limits::Bound;
    use crate::materialize::{Locus, MaterializedDocument, NodeId, NodeKind};
    use crate::resolver::evidence::Source;
    use crate::resolver::evidence_source::SourceSpan;
    use crate::snapshot::{from_json, Node};
    use crate::structural::Edge;

    use super::fixtures::{mapping, sidecar, CALL_SITE};
    use super::{admit, mapped, named, CallSiteSpans, Located};

    /// A source longer than every test span, whose bytes outnumber its
    /// characters.
    const SOURCE: &str = "defineConfig({ a: 1, b: [true], c: 2, d: 3 }) // 設定\n";
    const ENTRY: &str = "intlify-programmatic-entry.v0:storefront";

    /// The view of the span `[start, end)`.
    fn view(start: u64, end: u64) -> Value {
        json!({ "start": start.to_string(), "end": end.to_string() })
    }

    /// A coordinate pair is a span only when both are integers and
    /// `start <= end <= length` over the source's bytes, which need not fall
    /// on a character boundary (D15:745, 749).
    #[test]
    fn a_candidate_is_a_span_only_with_integer_coordinates_within_the_source() {
        // 2^64, the first integer past `u64::MAX`.
        const PAST: f64 = 18_446_744_073_709_551_616.0;
        // Five bytes: `a`, the three of `設`, and `b`.
        let call_site = sidecar("a設b", None, Vec::new());
        let span =
            |start, end| admit(SpanCandidate { start, end }, &call_site).map(SourceSpan::view);
        for (start, end, admitted) in [
            (0.0, 5.0, Some((0, 5))),
            (0.0, 0.0, Some((0, 0))),
            (5.0, 5.0, Some((5, 5))),
            (2.0, 3.0, Some((2, 3))),
            (-0.0, 1.0, Some((0, 1))),
            (-1.0, 1.0, None),
            (3.0, 2.0, None),
            (0.0, 6.0, None),
            (6.0, 6.0, None),
            (0.5, 1.0, None),
            (0.0, 1.5, None),
            (f64::NAN, 1.0, None),
            (0.0, f64::INFINITY, None),
            (0.0, PAST, None),
        ] {
            assert_eq!(
                span(start, end),
                admitted.map(|(start, end)| view(start, end)),
                "[{start}, {end})"
            );
        }
    }

    /// D15:747 (Decision 015-203): a key selects `member-key`, a value
    /// `member-value` at an object member and `node` at the root or an
    /// element, where no key is.
    #[test]
    fn each_location_selects_one_role() {
        assert_eq!(Located::Key.role(true), Some(LocationRole::MemberKey));
        assert_eq!(Located::Value.role(true), Some(LocationRole::MemberValue));
        assert_eq!(Located::Value.role(false), Some(LocationRole::Node));
        assert_eq!(Located::Key.role(false), None);
    }

    /// D15:747, 749 (Decision 015-203): a position takes the span mapped for
    /// the role it selects, and none when that mapping is missing or no span,
    /// whatever another role maps there.
    #[test]
    fn a_position_takes_only_the_span_mapped_for_its_role() {
        let call_site = sidecar(
            SOURCE,
            None,
            vec![
                mapping(&json!([]), "node", 0.0, 45.0),
                mapping(&json!([]), "member-key", 0.0, 1.0),
                mapping(&json!(["a"]), "node", 1.0, 2.0),
                mapping(&json!(["a"]), "member-key", 15.0, 16.0),
                mapping(&json!(["a"]), "member-value", 18.0, 19.0),
                mapping(&json!(["b", 0]), "node", 25.0, 29.0),
                mapping(&json!(["b", 0]), "member-value", 2.0, 3.0),
                mapping(&json!(["c"]), "member-key", 32.0, 33.0),
                mapping(&json!(["d"]), "member-value", 41.0, 40.0),
            ],
        );
        let at = |path: &[Edge<'_>], located| {
            mapped(&call_site, path, located).map(|(source, span)| {
                assert_eq!(source.identity.as_str(), CALL_SITE);
                span.view()
            })
        };
        let member = |name| [Edge::Member(name)];
        assert_eq!(at(&[], Located::Value), Some(view(0, 45)));
        assert_eq!(at(&member("a"), Located::Key), Some(view(15, 16)));
        assert_eq!(at(&member("a"), Located::Value), Some(view(18, 19)));
        assert_eq!(
            at(&[Edge::Member("b"), Edge::Index(0)], Located::Value),
            Some(view(25, 29))
        );
        // `c`'s value has no `member-value` mapping, and takes neither its
        // key's span nor a `node` span.
        assert_eq!(at(&member("c"), Located::Value), None);
        // An inverted candidate is absence.
        assert_eq!(at(&member("d"), Located::Value), None);
        assert_eq!(at(&member("e"), Located::Value), None);
        // Nothing locates a key at the root, whatever the sidecar maps there.
        assert_eq!(at(&[], Located::Key), None);
    }

    /// D15:795, 796, 800: a span selects the call-site identity and only its
    /// own locator, none when it has none even though the Programmatic Entry
    /// has one; without a span, Evidence names the Programmatic Entry's
    /// identity and locator.
    #[test]
    fn a_span_selects_the_call_site_source_and_no_span_the_programmatic_entry() {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        let limits = SourceLimits {
            identity_bytes: largest,
            revision_bytes: largest,
        };
        let digest = Node::Null.digest();
        let pair = read_entry_pair(Some(ENTRY), Some(&digest), &digest, limits).unwrap();
        let candidate = LocatorCandidate {
            variant: LocatorVariant::SafeLabel,
            payload: "storefront entry",
            non_secret: true,
        };
        let locator = read_programmatic_locator(Some(candidate), largest).unwrap();
        let entry = Source {
            identity: pair.identity(),
            marked: false,
            locator: Some(&locator),
        };
        let root = || vec![mapping(&json!([]), "node", 0.0, 1.0)];
        for (call_site, label) in [
            (sidecar(SOURCE, None, root()), None),
            (sidecar(SOURCE, Some("app.ts"), root()), Some("app.ts")),
        ] {
            let (source, span) = named(entry, mapped(&call_site, &[], Located::Value));
            assert_eq!(source.identity.as_str(), CALL_SITE);
            assert!(source.marked);
            assert_eq!(source.locator.map(Locator::payload), label);
            assert_eq!(span.map(SourceSpan::view), Some(view(0, 1)));
            let unmapped = mapped(&call_site, &[Edge::Member("a")], Located::Value);
            let (source, span) = named(entry, unmapped);
            assert_eq!(source.identity.as_str(), ENTRY);
            assert!(!source.marked);
            assert_eq!(
                source.locator.map(Locator::payload),
                Some("storefront entry")
            );
            assert_eq!(span, None);
        }
    }

    /// Every locus of `document`, each named by its value's path, `/` and a
    /// key or an index for each edge, and a member's key by its member's path
    /// and ` key`.
    fn loci(document: &MaterializedDocument) -> Vec<(String, Locus)> {
        fn walk(
            document: &MaterializedDocument,
            node: NodeId,
            name: &str,
            found: &mut Vec<(String, Locus)>,
        ) {
            found.push((name.to_owned(), document.node(node).locus()));
            match document.node(node).kind() {
                NodeKind::Object(members) => {
                    for (key, member) in members {
                        let path = format!("{name}/{key}");
                        found.push((format!("{path} key"), member.key()));
                        walk(document, member.value(), &path, found);
                    }
                }
                NodeKind::Array(elements) => {
                    for (index, &element) in elements.iter().enumerate() {
                        walk(document, element, &format!("{name}/{index}"), found);
                    }
                }
                NodeKind::Null
                | NodeKind::Boolean(_)
                | NodeKind::Number(_)
                | NodeKind::String(_) => {}
            }
        }
        let mut found = Vec::new();
        walk(document, document.root(), "", &mut found);
        found
    }

    /// D15:747, 749 (Decision 015-203): a document takes the span of each
    /// mapping whose path names an established node and whose role the rule
    /// selects there, at the locus of the key or the value it locates, with
    /// the call-site source; a role the rule does not select there, a path
    /// that leaves the document, and a candidate that is no span give none.
    #[test]
    fn a_document_takes_each_span_whose_path_is_established_and_whose_role_is_selected() {
        let value = json!({ "a": { "b": [true, { "c": null }] }, "d": "x" });
        let document = MaterializedDocument::from_snapshot(&from_json(&value)).unwrap();
        let call_site = sidecar(
            SOURCE,
            None,
            vec![
                mapping(&json!([]), "node", 0.0, 1.0),
                mapping(&json!(["a"]), "member-key", 1.0, 2.0),
                mapping(&json!(["a"]), "member-value", 2.0, 3.0),
                mapping(&json!(["a", "b", 0]), "node", 3.0, 4.0),
                mapping(&json!(["a", "b", 1, "c"]), "member-key", 4.0, 5.0),
                // Roles the rule does not select where the paths end.
                mapping(&json!([]), "member-key", 5.0, 6.0),
                mapping(&json!([]), "member-value", 5.0, 6.0),
                mapping(&json!(["a"]), "node", 5.0, 6.0),
                mapping(&json!(["a", "b", 0]), "member-key", 5.0, 6.0),
                mapping(&json!(["a", "b", 0]), "member-value", 5.0, 6.0),
                // Paths that name no established node: a missing member, an
                // element past the end, an index into an object, and a key
                // into a string.
                mapping(&json!(["e"]), "member-value", 5.0, 6.0),
                mapping(&json!(["a", "b", 2]), "node", 5.0, 6.0),
                mapping(&json!(["a", 0]), "node", 5.0, 6.0),
                mapping(&json!(["d", "x"]), "member-key", 5.0, 6.0),
                // Candidates that are no span.
                mapping(&json!(["d"]), "member-key", 7.0, 6.0),
                mapping(&json!(["d"]), "member-value", 6.0, 6.5),
            ],
        );
        let spans = CallSiteSpans::new(&call_site, &document);
        let expected: BTreeMap<&str, Value> = [
            ("", view(0, 1)),
            ("/a key", view(1, 2)),
            ("/a", view(2, 3)),
            ("/a/b/0", view(3, 4)),
            ("/a/b/1/c key", view(4, 5)),
        ]
        .into_iter()
        .collect();
        let loci = loci(&document);
        assert_eq!(loci.len(), 11);
        for (name, locus) in loci {
            let found = spans.at(locus).map(|(source, span)| {
                assert_eq!(source.identity.as_str(), CALL_SITE);
                span.view()
            });
            assert_eq!(found.as_ref(), expected.get(name.as_str()), "{name}");
        }
    }
}
