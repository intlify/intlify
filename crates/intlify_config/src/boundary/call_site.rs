// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `programmatic-call-site-evidence-input` boundary: the optional
//! sidecar a programmatic frontend submits beside its Snapshot, admitted with
//! its span candidates as submitted, or refused with one of the ten reasons of
//! its row (D15:144, 741, 747, 749, 3149).
//!
//! The sidecar holds one Call-Site Source identity and revision, whether the
//! frontend marks that identity non-secret, an optional locator candidate,
//! the exact source bytes, and mappings, each from a Snapshot Structural Path
//! and a location role to a half-open byte-span candidate. The binding
//! reports the sidecar, or that it could not inspect it without running host
//! behavior. The submitted sidecar can hold every form the boundary refuses,
//! so tests and conformance supply each without a live host: parts of any
//! length, mappings in any order or repeated, any role, a key that is not a
//! sequence of Unicode scalar values, an index that is not an integer in
//! `0..=u64::MAX`, and source bytes that are not UTF-8. No sidecar is absence,
//! not a failure.
//!
//! A sidecar is a host value with no text to parse, so no check sits at the
//! strict-syntax position of D15:3155, and the checks take its positions in
//! the order the row lists their reasons. The first that fails decides:
//!
//! 1. the binding inspected the sidecar safely, or it is
//!    `unsafe-host-inspection`;
//! 2. the identity and the revision, each when present, are within
//!    `maxSourceIdentityBytes` and `maxSourceRevisionBytes`, the source bytes
//!    within `maxCallSiteSourceBytes`, and the submitted mappings, repeated
//!    ones counted, within `maxCallSiteMappings`, or it is `over-bound-input`;
//! 3. each mapping's role is `node`, `member-key`, or `member-value`, and each
//!    edge of its path is a key that is a sequence of Unicode scalar values or
//!    an index in `0..=u64::MAX`, or it is `malformed-input`;
//! 4. the source bytes are UTF-8, or it is `invalid-utf8-source`;
//! 5. no path and role is mapped twice, or it is `duplicate-mapping`;
//! 6. the identity and the revision pass the file pair's checks other than its
//!    bounds, with the file row's reasons: `missing-source-identity`,
//!    `missing-source-revision`, `unstable-source-identity`,
//!    `unbound-source-revision`, and last `source-snapshot-mismatch` against
//!    the Call-Site Source revision of the source bytes (D15:743).
//!
//! The locator is admitted on its own as a programmatic locator: one that is
//! not admitted is absence and never refuses the sidecar (D15:749, 766).
//! Whether a path names an established node or edge, whether its role applies
//! there, and whether its span is valid are each candidate's own question
//! once the sidecar is admitted, and a candidate that fails is absence
//! (D15:749). The admitted sidecar therefore keeps every candidate as
//! submitted, since it is replay input even when none applies (D15:2657), and
//! only puts the mappings in canonical order, which is no change of value
//! (D15:2585).

use std::collections::BTreeMap;

use crate::artifacts::count;
use crate::input_limits::Bound;

use super::capability::ImplementationCapability;
use super::locator::{self, Locator, LocatorCandidate};
use super::source::{self, SourceLimits, SourcePair, SourceRefusal};

/// The binding's report about the sidecar. No `Debug`: a sidecar can carry
/// secrets.
pub(crate) enum CallSiteReport<'a> {
    /// The binding could not inspect the sidecar without running its behavior.
    Unsafe,
    /// The formal sidecar.
    Formal(SubmittedCallSite<'a>),
}

/// A formal sidecar as submitted. No `Debug`: its identity and its source can
/// carry secrets.
pub(crate) struct SubmittedCallSite<'a> {
    pub(crate) identity: Option<&'a str>,
    /// Whether the frontend marks the identity non-secret.
    pub(crate) marked: bool,
    pub(crate) revision: Option<&'a str>,
    pub(crate) locator: Option<LocatorCandidate<'a>>,
    /// The source's exact bytes, UTF-8 or not.
    pub(crate) source: Vec<u8>,
    /// The mappings in the order submitted, repeated ones included.
    pub(crate) mappings: Vec<SubmittedMapping>,
}

/// One mapping as submitted: a path and a role, and the span candidate they
/// map to. No `Debug`: a path's keys are the configuration's.
#[derive(Clone)]
pub(crate) struct SubmittedMapping {
    pub(crate) path: Vec<SubmittedEdge>,
    pub(crate) role: String,
    pub(crate) span: SpanCandidate,
}

/// One edge of a path as submitted. No `Debug`.
#[derive(Clone)]
pub(crate) enum SubmittedEdge {
    /// A key's bytes: its UTF-8 when it is a sequence of Unicode scalar
    /// values, and other bytes, such as the generalized UTF-8 of an isolated
    /// surrogate, when it is not.
    Key(Vec<u8>),
    /// An index, as the number the binding read.
    Index(f64),
}

/// A span candidate as submitted: `[start, end)` over the source bytes, each
/// coordinate as the number the binding read. Whether it is a Portable Source
/// Span is decided where Evidence selects it (D15:745, 749).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SpanCandidate {
    pub(crate) start: f64,
    pub(crate) end: f64,
}

impl SpanCandidate {
    /// Both coordinates, when each is an integer in `0..=u64::MAX`, which is
    /// all a byte coordinate can be (D15:745).
    pub(crate) fn coordinates(self) -> Option<(u64, u64)> {
        Some((unsigned(self.start)?, unsigned(self.end)?))
    }
}

/// A mapping's closed location role, in 015's order (D15:747).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum LocationRole {
    Node,
    MemberKey,
    MemberValue,
}

impl LocationRole {
    /// Every role, in 015's order.
    pub(crate) const ALL: [Self; 3] = [Self::Node, Self::MemberKey, Self::MemberValue];

    /// The role's ID.
    pub(crate) const fn id(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::MemberKey => "member-key",
            Self::MemberValue => "member-value",
        }
    }

    /// The role whose ID is `id`, if any.
    fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|role| role.id() == id)
    }
}

/// One edge of a Snapshot Structural Path: an object member's key, or an
/// array element's index (D15:141). No `Debug`: a key is the configuration's.
///
/// The derived order is 015's for mapping keys: paths compare edge by edge
/// with a shorter equal prefix first, and at one position a key precedes an
/// index, keys comparing by their unsigned UTF-8 bytes and indices
/// numerically (D15:747, Decision 015-204). A key and an index meet at one
/// position only when one of the paths names no established node.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum StructuralEdge {
    Key(String),
    Index(u64),
}

/// A mapping's key: its path and its role.
type MappingKey = (Vec<StructuralEdge>, LocationRole);

/// Why the boundary refuses a report: one of the ten reasons of its row, in
/// that order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CallSiteRefusal {
    UnsafeHostInspection,
    /// A part is over its `bootstrapOnly.evidenceMetadata` bound.
    OverBoundInput,
    /// A mapping's role or an edge of its path is outside the closed shape.
    MalformedInput,
    InvalidUtf8Source,
    /// Two mappings have one path and one role.
    DuplicateMapping,
    MissingIdentity,
    MissingRevision,
    /// The identity has no `:`, an invalid tag, or an empty payload.
    UnstableIdentity,
    /// The revision is not `sha256:` and 64 lowercase hexadecimal digits.
    UnboundRevision,
    /// The revision is not the Call-Site Source revision of the source bytes.
    SnapshotMismatch,
}

impl CallSiteRefusal {
    /// The reason as 015's boundary table spells it.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::UnsafeHostInspection => "unsafe-host-inspection",
            Self::OverBoundInput => "over-bound-input",
            Self::MalformedInput => "malformed-input",
            Self::InvalidUtf8Source => "invalid-utf8-source",
            Self::DuplicateMapping => "duplicate-mapping",
            Self::MissingIdentity => "missing-source-identity",
            Self::MissingRevision => "missing-source-revision",
            Self::UnstableIdentity => "unstable-source-identity",
            Self::UnboundRevision => "unbound-source-revision",
            Self::SnapshotMismatch => "source-snapshot-mismatch",
        }
    }

    /// The row's refusal for a failed check of the pair: its only over-bound
    /// reason for either bound, and the same reason for every other check.
    const fn of(refusal: SourceRefusal) -> Self {
        match refusal {
            SourceRefusal::MissingIdentity => Self::MissingIdentity,
            SourceRefusal::MissingRevision => Self::MissingRevision,
            SourceRefusal::OverBoundIdentity | SourceRefusal::OverBoundRevision => {
                Self::OverBoundInput
            }
            SourceRefusal::UnstableIdentity => Self::UnstableIdentity,
            SourceRefusal::UnboundRevision => Self::UnboundRevision,
            SourceRefusal::SnapshotMismatch => Self::SnapshotMismatch,
        }
    }
}

/// The bounds the boundary reads: the capability's, or a test's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CallSiteLimits {
    pub(crate) pair: SourceLimits,
    pub(crate) source_bytes: Bound,
    pub(crate) mappings: Bound,
    pub(crate) locator_bytes: Bound,
}

impl CallSiteLimits {
    const fn of(capability: &ImplementationCapability) -> Self {
        let bounds = &capability.bootstrap_only().evidence_metadata;
        Self {
            pair: SourceLimits::of(capability),
            source_bytes: bounds.max_call_site_source_bytes,
            mappings: bounds.max_call_site_mappings,
            locator_bytes: bounds.max_locator_bytes,
        }
    }
}

/// An admitted sidecar: its pair, its mark, its locator or none, its UTF-8
/// source, and every mapping in canonical order with its candidate as
/// submitted. No `Debug`: its identity and its source can carry secrets.
pub(crate) struct AdmittedCallSite {
    pair: SourcePair,
    marked: bool,
    locator: Option<Locator>,
    source: String,
    mappings: BTreeMap<MappingKey, SpanCandidate>,
}

impl AdmittedCallSite {
    /// The Call-Site Source identity and revision.
    pub(crate) const fn pair(&self) -> &SourcePair {
        &self.pair
    }

    /// Whether the frontend marked the identity non-secret.
    pub(crate) const fn marked(&self) -> bool {
        self.marked
    }

    /// The Call-Site Source locator, `None` for absence.
    pub(crate) const fn locator(&self) -> Option<&Locator> {
        self.locator.as_ref()
    }

    /// The exact source, whose bytes a span's coordinates count.
    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    /// Every mapping in canonical order, by path and then by role, with its
    /// candidate as submitted.
    pub(crate) fn mappings(
        &self,
    ) -> impl Iterator<Item = (&[StructuralEdge], LocationRole, SpanCandidate)> {
        self.mappings
            .iter()
            .map(|((path, role), span)| (path.as_slice(), *role, *span))
    }

    /// The candidate `path` and `role` are mapped to, if any.
    pub(crate) fn candidate(
        &self,
        path: Vec<StructuralEdge>,
        role: LocationRole,
    ) -> Option<SpanCandidate> {
        self.mappings.get(&(path, role)).copied()
    }
}

/// Two sidecars are equal by value, as invocation staleness compares them
/// (D15:2583): the pair, the mark, the locator, the source, and the same
/// mappings, in canonical order, to the same candidates. A candidate keeps
/// the numbers the binding read, so its coordinates compare as numbers: `-0`
/// equals `+0`, as Portable JSON Numbers have it, and NaN equals NaN, so a
/// sidecar equals itself (plan 025 decision #306). The pattern names every
/// field, so a part added to the sidecar fails to compile here.
impl PartialEq for AdmittedCallSite {
    fn eq(&self, other: &Self) -> bool {
        let Self {
            pair,
            marked,
            locator,
            source,
            mappings,
        } = self;
        *pair == other.pair
            && *marked == other.marked
            && *locator == other.locator
            && *source == other.source
            && mappings.len() == other.mappings.len()
            && mappings.iter().zip(&other.mappings).all(|(left, right)| {
                left.0 == right.0
                    && same_number(left.1.start, right.1.start)
                    && same_number(left.1.end, right.1.end)
            })
    }
}

impl Eq for AdmittedCallSite {}

/// Whether two coordinates are the same number: equal once `-0` is read as
/// `+0` and every NaN as one NaN.
fn same_number(left: f64, right: f64) -> bool {
    normalized(left).to_bits() == normalized(right).to_bits()
}

/// `value` with `-0` as `+0` and every NaN as [`f64::NAN`].
fn normalized(value: f64) -> f64 {
    if value.is_nan() {
        f64::NAN
    } else if value == 0.0 {
        0.0
    } else {
        value
    }
}

/// Admit a report, `None` when no sidecar was submitted, under the
/// capability's `bootstrapOnly.evidenceMetadata` bounds: the admitted
/// sidecar, or `None` for its absence.
pub(crate) fn admit(
    report: Option<CallSiteReport<'_>>,
    capability: &ImplementationCapability,
) -> Result<Option<AdmittedCallSite>, CallSiteRefusal> {
    read_report(report, CallSiteLimits::of(capability))
}

/// The boundary's checks under `limits`, which [`admit`] takes from the
/// capability.
pub(crate) fn read_report(
    report: Option<CallSiteReport<'_>>,
    limits: CallSiteLimits,
) -> Result<Option<AdmittedCallSite>, CallSiteRefusal> {
    use CallSiteRefusal::{
        DuplicateMapping, InvalidUtf8Source, MalformedInput, OverBoundInput, UnsafeHostInspection,
    };

    let SubmittedCallSite {
        identity,
        marked,
        revision,
        locator: candidate,
        source: bytes,
        mappings,
    } = match report {
        None => return Ok(None),
        Some(CallSiteReport::Unsafe) => return Err(UnsafeHostInspection),
        Some(CallSiteReport::Formal(sidecar)) => sidecar,
    };
    source::check_bounds(identity, revision, limits.pair).map_err(CallSiteRefusal::of)?;
    if count(bytes.len()) > limits.source_bytes.get()
        || count(mappings.len()) > limits.mappings.get()
    {
        return Err(OverBoundInput);
    }
    let keyed = mappings
        .iter()
        .map(|mapping| Some((mapping.key()?, mapping.span)))
        .collect::<Option<Vec<_>>>()
        .ok_or(MalformedInput)?;
    let text = String::from_utf8(bytes).map_err(|_| InvalidUtf8Source)?;
    let mut ordered = BTreeMap::new();
    for (key, span) in keyed {
        if ordered.insert(key, span).is_some() {
            return Err(DuplicateMapping);
        }
    }
    // The bounds held above, so only the pair's other checks can fail here.
    let pair = source::read_call_site_pair(identity, revision, text.as_bytes(), limits.pair)
        .map_err(CallSiteRefusal::of)?;
    Ok(Some(AdmittedCallSite {
        pair,
        marked,
        locator: locator::read_programmatic_locator(candidate, limits.locator_bytes),
        source: text,
        mappings: ordered,
    }))
}

impl SubmittedMapping {
    /// The mapping's path and role, when both are in the closed shape.
    fn key(&self) -> Option<MappingKey> {
        let role = LocationRole::from_id(&self.role)?;
        let path = self
            .path
            .iter()
            .map(SubmittedEdge::edge)
            .collect::<Option<_>>()?;
        Some((path, role))
    }
}

impl SubmittedEdge {
    /// The edge of a Snapshot Structural Path this one is: a key that is a
    /// sequence of Unicode scalar values, or an index in `0..=u64::MAX`
    /// (D15:141, 771).
    fn edge(&self) -> Option<StructuralEdge> {
        match self {
            Self::Key(bytes) => std::str::from_utf8(bytes)
                .ok()
                .map(|key| StructuralEdge::Key(key.to_owned())),
            Self::Index(value) => unsigned(*value).map(StructuralEdge::Index),
        }
    }
}

/// `value` as an unsigned integer when it is an integer in `0..=u64::MAX`,
/// which a binary64 is exactly when it is a non-negative integer below 2^64.
/// Negative zero is zero.
fn unsigned(value: f64) -> Option<u64> {
    // 2^64, the first integer past `u64::MAX`.
    const PAST: f64 = 18_446_744_073_709_551_616.0;
    ((0.0..PAST).contains(&value) && value.fract() == 0.0).then_some(value as u64)
}

// `resolver::staleness`'s tests build a sidecar no boundary admits.
#[cfg(test)]
pub(crate) mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::boundary::locator::LocatorVariant;
    use crate::boundary::source::{call_site_source_revision, file_source_revision};
    use crate::construction::capability::{
        IDENTITY as CAPABILITY, REVISION as CAPABILITY_REVISION,
    };
    use crate::locale::VersionedIdentity;
    use crate::resolver::limits::fixture_limits;
    use CallSiteRefusal::{
        DuplicateMapping, InvalidUtf8Source, MalformedInput, MissingIdentity, MissingRevision,
        OverBoundInput, SnapshotMismatch, UnboundRevision, UnstableIdentity,
    };

    /// A source whose bytes outnumber its characters.
    const SOURCE: &str = "defineConfig({ profiles: { app: {} } }) // 設定\n";
    const IDENTITY: &str = "intlify-call-site.v0:storefront/app.ts";
    const LABEL: &str = "app.ts";
    /// The greatest binary64 below 2^64: 2^64 - 2^11.
    const GREATEST: f64 = 18_446_744_073_709_549_568.0;

    fn largest() -> Bound {
        Bound::new(u64::MAX - 1).unwrap()
    }

    /// `sidecar` over `source` but under its own pair, which the boundary
    /// refuses: what only a revision collision could give.
    pub(crate) fn resourced(sidecar: AdmittedCallSite, source: &str) -> AdmittedCallSite {
        AdmittedCallSite {
            source: source.to_owned(),
            ..sidecar
        }
    }

    /// Limits no sidecar here is over.
    fn roomy() -> CallSiteLimits {
        CallSiteLimits {
            pair: SourceLimits {
                identity_bytes: largest(),
                revision_bytes: largest(),
            },
            source_bytes: largest(),
            mappings: largest(),
            locator_bytes: largest(),
        }
    }

    fn key(name: &str) -> SubmittedEdge {
        SubmittedEdge::Key(name.as_bytes().to_vec())
    }

    fn at(value: f64) -> SubmittedEdge {
        SubmittedEdge::Index(value)
    }

    fn mapping(path: Vec<SubmittedEdge>, role: &str, start: f64, end: f64) -> SubmittedMapping {
        SubmittedMapping {
            path,
            role: role.to_owned(),
            span: SpanCandidate { start, end },
        }
    }

    /// A mapping whose path and role are `path` and `node`.
    fn node(path: Vec<SubmittedEdge>) -> SubmittedMapping {
        mapping(path, "node", 0.0, 1.0)
    }

    fn label(payload: &str) -> LocatorCandidate<'_> {
        LocatorCandidate {
            variant: LocatorVariant::SafeLabel,
            payload,
            non_secret: true,
        }
    }

    /// A sidecar over `SOURCE` that every check admits when `revision` is the
    /// source's, holding `mappings`.
    fn sidecar(revision: &str, mappings: Vec<SubmittedMapping>) -> SubmittedCallSite<'_> {
        SubmittedCallSite {
            identity: Some(IDENTITY),
            marked: false,
            revision: Some(revision),
            locator: Some(label(LABEL)),
            source: SOURCE.as_bytes().to_vec(),
            mappings,
        }
    }

    fn revision() -> String {
        call_site_source_revision(SOURCE.as_bytes())
    }

    fn read(
        sidecar: SubmittedCallSite<'_>,
        limits: CallSiteLimits,
    ) -> Result<Option<AdmittedCallSite>, CallSiteRefusal> {
        read_report(Some(CallSiteReport::Formal(sidecar)), limits)
    }

    fn admitted(sidecar: SubmittedCallSite<'_>, limits: CallSiteLimits) -> AdmittedCallSite {
        let Ok(Some(admitted)) = read(sidecar, limits) else {
            panic!("the sidecar is admitted");
        };
        admitted
    }

    fn refused(sidecar: SubmittedCallSite<'_>, limits: CallSiteLimits) -> CallSiteRefusal {
        read(sidecar, limits).err().expect("the sidecar is refused")
    }

    /// A path as text, `/key` for a key and `[index]` for an index.
    fn spelled(path: &[StructuralEdge]) -> String {
        path.iter()
            .map(|edge| match edge {
                StructuralEdge::Key(key) => format!("/{key}"),
                StructuralEdge::Index(index) => format!("[{index}]"),
            })
            .collect()
    }

    /// The admitted mappings in their order: each path as text, its role, and
    /// its candidate.
    fn view(admitted: &AdmittedCallSite) -> Vec<(String, &'static str, (f64, f64))> {
        admitted
            .mappings()
            .map(|(path, role, span)| (spelled(path), role.id(), (span.start, span.end)))
            .collect()
    }

    /// Every candidate stays as submitted: one the resolver will admit, and
    /// ones it will treat as absence, being negative, inverted, past the
    /// source, fractional, or not a number, or under a path no Snapshot needs
    /// to hold, or with a role that cannot apply at its path.
    #[test]
    fn a_sidecar_is_admitted_with_every_candidate_as_submitted() {
        let revision = revision();
        let mappings = vec![
            mapping(vec![key("profiles")], "member-key", 15.0, 23.0),
            mapping(vec![key("profiles"), key("app")], "member-value", -1.0, 4.0),
            mapping(vec![key("profiles"), key("app")], "member-key", 9.0, 3.0),
            mapping(vec![key("missing"), at(7.0)], "node", 0.0, 1000.0),
            mapping(vec![], "member-key", 0.5, 1.0),
            mapping(vec![], "node", f64::NAN, f64::INFINITY),
        ];
        let kept = admitted(sidecar(&revision, mappings), roomy());
        assert_eq!(kept.pair().identity().as_str(), IDENTITY);
        assert_eq!(kept.pair().revision(), revision);
        assert!(!kept.marked());
        assert_eq!(kept.locator().map(Locator::payload), Some(LABEL));
        assert_eq!(kept.source(), SOURCE);
        let (path, role, span) = kept.mappings().next().unwrap();
        assert_eq!((spelled(path), role), (String::new(), LocationRole::Node));
        assert!(span.start.is_nan() && span.end.is_infinite());
        assert_eq!(
            view(&kept)[1..],
            [
                (String::new(), "member-key", (0.5, 1.0)),
                ("/missing[7]".to_owned(), "node", (0.0, 1000.0)),
                ("/profiles".to_owned(), "member-key", (15.0, 23.0)),
                ("/profiles/app".to_owned(), "member-key", (9.0, 3.0)),
                ("/profiles/app".to_owned(), "member-value", (-1.0, 4.0)),
            ]
        );
    }

    /// An admitted sidecar gives the candidate of one path and one role.
    #[test]
    fn a_candidate_is_looked_up_by_its_path_and_role() {
        let mappings = vec![mapping(vec![key("profiles"), at(0.0)], "node", 1.0, 2.0)];
        let kept = admitted(sidecar(&revision(), mappings), roomy());
        let path = |edges: &[StructuralEdge]| edges.to_vec();
        let element = [
            StructuralEdge::Key("profiles".to_owned()),
            StructuralEdge::Index(0),
        ];
        assert_eq!(
            kept.candidate(path(&element), LocationRole::Node),
            Some(SpanCandidate {
                start: 1.0,
                end: 2.0
            })
        );
        assert_eq!(
            kept.candidate(path(&element), LocationRole::MemberValue),
            None
        );
        assert_eq!(
            kept.candidate(path(&element[..1]), LocationRole::Node),
            None
        );
    }

    /// A candidate's coordinates are integers in `0..=u64::MAX`, inverted or
    /// not, or none; whether they make a span is Evidence's question
    /// (D15:745, 749).
    #[test]
    fn a_candidates_coordinates_are_unsigned_integers() {
        // 2^64, the first integer past `u64::MAX`.
        const PAST: f64 = 18_446_744_073_709_551_616.0;
        let coordinates = |start, end| SpanCandidate { start, end }.coordinates();
        assert_eq!(coordinates(3.0, 9.0), Some((3, 9)));
        assert_eq!(coordinates(9.0, 3.0), Some((9, 3)));
        assert_eq!(
            coordinates(-0.0, GREATEST),
            Some((0, 18_446_744_073_709_549_568))
        );
        for (start, end) in [
            (-1.0, 1.0),
            (1.0, -1.0),
            (0.5, 1.0),
            (0.0, 1.5),
            (f64::NAN, 1.0),
            (0.0, f64::INFINITY),
            (0.0, PAST),
        ] {
            assert_eq!(coordinates(start, end), None, "[{start}, {end})");
        }
    }

    /// No sidecar is absence under any bound, never a failure (D15:144).
    #[test]
    fn no_sidecar_is_absence() {
        let one = Bound::new(1).unwrap();
        let tight = CallSiteLimits {
            pair: SourceLimits {
                identity_bytes: one,
                revision_bytes: one,
            },
            source_bytes: one,
            mappings: one,
            locator_bytes: one,
        };
        assert!(matches!(read_report(None, tight), Ok(None)));
    }

    #[test]
    fn a_sidecar_the_binding_cannot_inspect_safely_is_refused() {
        assert_eq!(
            read_report(Some(CallSiteReport::Unsafe), roomy()).err(),
            Some(CallSiteRefusal::UnsafeHostInspection)
        );
    }

    /// Each bound admits its exact value and refuses the sidecar once it is
    /// one less, counting bytes rather than characters and repeated mappings
    /// as submitted. Only the locator's bound leaves the sidecar admitted.
    #[test]
    fn each_bound_admits_its_exact_value_and_refuses_one_more() {
        let revision = revision();
        let mappings = || vec![node(vec![]), node(vec![key("a")])];
        let bound = |value: usize| Bound::new(count(value)).unwrap();
        let exact = CallSiteLimits {
            pair: SourceLimits {
                identity_bytes: bound(IDENTITY.len()),
                revision_bytes: bound(revision.len()),
            },
            source_bytes: bound(SOURCE.len()),
            mappings: bound(2),
            locator_bytes: bound(LABEL.len()),
        };
        let within = admitted(sidecar(&revision, mappings()), exact);
        assert_eq!(within.locator().map(Locator::payload), Some(LABEL));
        for short in [
            CallSiteLimits {
                pair: SourceLimits {
                    identity_bytes: bound(IDENTITY.len() - 1),
                    ..exact.pair
                },
                ..exact
            },
            CallSiteLimits {
                pair: SourceLimits {
                    revision_bytes: bound(revision.len() - 1),
                    ..exact.pair
                },
                ..exact
            },
            CallSiteLimits {
                source_bytes: bound(SOURCE.len() - 1),
                ..exact
            },
            CallSiteLimits {
                mappings: bound(1),
                ..exact
            },
        ] {
            assert_eq!(
                refused(sidecar(&revision, mappings()), short),
                OverBoundInput
            );
        }
        // A repeated mapping counts toward the bound before it is a repeat.
        let twice = || vec![node(vec![]), node(vec![])];
        assert_eq!(
            refused(
                sidecar(&revision, twice()),
                CallSiteLimits {
                    mappings: bound(1),
                    ..exact
                }
            ),
            OverBoundInput
        );
        assert_eq!(
            refused(sidecar(&revision, twice()), exact),
            DuplicateMapping
        );
        let short = CallSiteLimits {
            locator_bytes: bound(LABEL.len() - 1),
            ..exact
        };
        assert!(admitted(sidecar(&revision, mappings()), short)
            .locator()
            .is_none());
    }

    #[test]
    fn a_role_outside_the_closed_vocabulary_is_malformed() {
        let revision = revision();
        let within = admitted(
            sidecar(
                &revision,
                LocationRole::ALL
                    .map(|role| mapping(vec![key("a")], role.id(), 0.0, 1.0))
                    .to_vec(),
            ),
            roomy(),
        );
        assert_eq!(
            within
                .mappings()
                .map(|(_, role, _)| role)
                .collect::<Vec<_>>(),
            LocationRole::ALL
        );
        for role in [
            "",
            "Node",
            " node",
            "member",
            "member_key",
            "memberValue",
            "value",
        ] {
            let mappings = vec![node(vec![]), mapping(vec![key("a")], role, 0.0, 1.0)];
            assert_eq!(
                refused(sidecar(&revision, mappings), roomy()),
                MalformedInput,
                "{role:?}"
            );
        }
    }

    /// A key must be a sequence of Unicode scalar values and an index an
    /// integer in `0..=u64::MAX`, which as a binary64 is below 2^64; an empty
    /// key is a key, and negative zero is index zero (D15:141, 771).
    #[test]
    fn an_edge_that_is_neither_a_key_nor_an_index_is_malformed() {
        let revision = revision();
        let within = admitted(
            sidecar(
                &revision,
                vec![
                    node(vec![key("")]),
                    node(vec![key("設定"), at(0.0)]),
                    node(vec![at(-0.0), at(1.0)]),
                    node(vec![at(GREATEST)]),
                ],
            ),
            roomy(),
        );
        assert_eq!(
            within
                .mappings()
                .map(|(path, _, _)| spelled(path))
                .collect::<Vec<_>>(),
            ["/", "/設定[0]", "[0][1]", "[18446744073709549568]"]
        );
        for edge in [
            SubmittedEdge::Key(vec![0xff]),
            // The generalized UTF-8 of the isolated surrogate U+D800.
            SubmittedEdge::Key(vec![0xed, 0xa0, 0x80]),
            at(-1.0),
            at(0.5),
            at(f64::NAN),
            at(f64::INFINITY),
            at(f64::NEG_INFINITY),
            at(18_446_744_073_709_551_616.0),
        ] {
            let mappings = vec![node(vec![]), node(vec![key("a"), edge])];
            assert_eq!(
                refused(sidecar(&revision, mappings), roomy()),
                MalformedInput
            );
        }
    }

    /// The source's bytes must be UTF-8, even under the revision computed over
    /// those same bytes.
    #[test]
    fn a_source_that_is_not_utf8_is_refused() {
        for bytes in [
            vec![0xff],
            vec![0xed, 0xa0, 0x80],
            b"defineConfig(\xc3)".to_vec(),
        ] {
            let revision = call_site_source_revision(&bytes);
            let mut submitted = sidecar(&revision, vec![node(vec![])]);
            submitted.source = bytes;
            assert_eq!(refused(submitted, roomy()), InvalidUtf8Source);
        }
    }

    /// A path and role mapped twice is a repeat wherever the two stand and
    /// whatever their candidates, while one path under several roles, and a
    /// key and an index with the same digits, are distinct keys.
    #[test]
    fn a_path_and_role_mapped_twice_is_a_duplicate() {
        let revision = revision();
        let distinct = vec![
            mapping(vec![key("a")], "member-key", 0.0, 1.0),
            mapping(vec![key("a")], "member-value", 0.0, 1.0),
            node(vec![key("0")]),
            node(vec![at(0.0)]),
        ];
        assert_eq!(
            view(&admitted(sidecar(&revision, distinct.clone()), roomy())).len(),
            4
        );
        for repeated in [
            mapping(vec![key("a")], "member-key", 2.0, 3.0),
            node(vec![at(-0.0)]),
            node(vec![key("0")]),
        ] {
            let mut mappings = distinct.clone();
            mappings.insert(1, repeated);
            assert_eq!(
                refused(sidecar(&revision, mappings), roomy()),
                DuplicateMapping
            );
        }
    }

    /// The pair runs the file pair's checks other than its bounds, with the
    /// file row's reasons, against the Call-Site Source revision: the file
    /// domain's revision of the same bytes is a mismatch (D15:743, 3389).
    #[test]
    fn the_pair_runs_the_file_pairs_other_checks() {
        let own = revision();
        let file = file_source_revision(SOURCE.as_bytes());
        let other = call_site_source_revision(b"{}");
        let upper = own.to_ascii_uppercase();
        for (identity, revision, refusal) in [
            (None, Some(&*own), MissingIdentity),
            (None, None, MissingIdentity),
            (Some(IDENTITY), None, MissingRevision),
            (Some("app.ts"), Some(&*own), UnstableIdentity),
            (Some("app.ts"), None, MissingRevision),
            (Some(IDENTITY), Some(&*upper), UnboundRevision),
            (Some(IDENTITY), Some("sha256:0"), UnboundRevision),
            (Some(IDENTITY), Some(&*file), SnapshotMismatch),
            (Some(IDENTITY), Some(&*other), SnapshotMismatch),
        ] {
            let mut submitted = sidecar("", vec![]);
            submitted.identity = identity;
            submitted.revision = revision;
            assert_eq!(refused(submitted, roomy()), refusal, "{refusal:?}");
        }
    }

    /// Each position decides before every later one: the bounds before the
    /// closed shape, the closed shape of every mapping before the source's
    /// UTF-8, the UTF-8 before a repeat, and a repeat before the pair. An
    /// absent part is held by no bound, and the identity's form decides
    /// before the revision's presentation (D15:3155).
    #[test]
    fn the_first_failing_position_decides() {
        fn invalid(mut submitted: SubmittedCallSite<'_>) -> SubmittedCallSite<'_> {
            submitted.source = vec![0xff];
            submitted
        }

        let revision = revision();
        let one = Bound::new(1).unwrap();
        let unknown = || mapping(vec![], "value", 0.0, 1.0);
        let few = CallSiteLimits {
            mappings: one,
            ..roomy()
        };
        let short = CallSiteLimits {
            source_bytes: one,
            ..roomy()
        };
        let small = CallSiteLimits {
            pair: SourceLimits {
                identity_bytes: one,
                revision_bytes: one,
            },
            ..roomy()
        };
        let narrow = CallSiteLimits {
            pair: SourceLimits {
                identity_bytes: one,
                ..roomy().pair
            },
            ..roomy()
        };
        let twice = || vec![node(vec![]), node(vec![])];
        let mut missing = sidecar(&revision, twice());
        missing.identity = None;
        let mut absent = sidecar(&revision, vec![unknown()]);
        absent.identity = None;
        absent.revision = None;
        let mut unbounded = sidecar(&revision, vec![]);
        unbounded.identity = None;
        let mut unstable = sidecar("sha256:0", vec![]);
        unstable.identity = Some("app.ts");
        for (submitted, limits, refusal) in [
            (
                invalid(sidecar(&revision, vec![node(vec![]), node(vec![key("a")])])),
                few,
                OverBoundInput,
            ),
            (sidecar(&revision, vec![unknown()]), short, OverBoundInput),
            (sidecar(&revision, twice()), small, OverBoundInput),
            (absent, small, MalformedInput),
            (
                invalid(sidecar(&revision, vec![unknown()])),
                roomy(),
                MalformedInput,
            ),
            (
                sidecar(&revision, vec![node(vec![]), node(vec![]), unknown()]),
                roomy(),
                MalformedInput,
            ),
            (
                invalid(sidecar(&revision, twice())),
                roomy(),
                InvalidUtf8Source,
            ),
            (missing, roomy(), DuplicateMapping),
            (unbounded, narrow, MissingIdentity),
            (unstable, roomy(), UnstableIdentity),
        ] {
            assert_eq!(refused(submitted, limits), refusal, "{refusal:?}");
        }
    }

    /// The submitted order is no part of the value: every order admits the
    /// same mappings in one canonical order, by path and then by role. Keys
    /// compare by their unsigned UTF-8 bytes, indices numerically, a shorter
    /// equal prefix first, and a key before an index at one position
    /// (Decision 015-204).
    #[test]
    fn mappings_in_any_order_are_admitted_in_one_canonical_order() {
        let revision = revision();
        let mappings = vec![
            node(vec![key("b")]),
            node(vec![key("a"), at(10.0)]),
            mapping(vec![key("a")], "member-value", 0.0, 1.0),
            node(vec![key("é")]),
            node(vec![key("a"), at(2.0)]),
            mapping(vec![key("a"), key("z")], "member-key", 0.0, 1.0),
            node(vec![key("a")]),
            node(vec![]),
            mapping(vec![key("a")], "member-key", 0.0, 1.0),
            node(vec![key("B")]),
        ];
        let canonical = [
            ("", "node"),
            ("/B", "node"),
            ("/a", "node"),
            ("/a", "member-key"),
            ("/a", "member-value"),
            ("/a/z", "member-key"),
            ("/a[2]", "node"),
            ("/a[10]", "node"),
            ("/b", "node"),
            ("/é", "node"),
        ];
        let mut orders = vec![mappings.clone(), mappings.iter().rev().cloned().collect()];
        for turn in 1..mappings.len() {
            let mut turned = mappings.clone();
            turned.rotate_left(turn);
            orders.push(turned);
        }
        for order in orders {
            let kept = admitted(sidecar(&revision, order), roomy());
            let order = view(&kept)
                .into_iter()
                .map(|(path, role, _)| (path, role))
                .collect::<Vec<_>>();
            assert_eq!(order, canonical.map(|(path, role)| (path.to_owned(), role)));
        }
    }

    /// The locator is admitted on its own as a programmatic locator, and one
    /// that is not admitted is absence in an admitted sidecar (D15:749).
    #[test]
    fn a_locator_is_admitted_on_its_own() {
        let revision = revision();
        let uri = "https://example.com/app.ts";
        for (candidate, expected) in [
            (Some(label(LABEL)), Some(LABEL)),
            (
                Some(LocatorCandidate {
                    variant: LocatorVariant::SafeUri,
                    payload: uri,
                    non_secret: true,
                }),
                Some(uri),
            ),
            (None, None),
            (
                Some(LocatorCandidate {
                    variant: LocatorVariant::RelativePath,
                    ..label(LABEL)
                }),
                None,
            ),
            (Some(label("")), None),
            (
                Some(LocatorCandidate {
                    non_secret: false,
                    ..label(LABEL)
                }),
                None,
            ),
        ] {
            let mut submitted = sidecar(&revision, vec![]);
            submitted.locator = candidate;
            assert_eq!(
                admitted(submitted, roomy()).locator().map(Locator::payload),
                expected,
                "{candidate:?}"
            );
        }
    }

    /// The admitted sidecar keeps the mark as the frontend supplied it, and
    /// the mark never changes whether, or why, the sidecar is refused.
    #[test]
    fn the_mark_is_carried_as_supplied() {
        let revision = revision();
        for marked in [false, true] {
            let mut submitted = sidecar(&revision, vec![]);
            submitted.marked = marked;
            assert_eq!(admitted(submitted, roomy()).marked(), marked);
            let mut submitted = sidecar("sha256:0", vec![]);
            submitted.marked = marked;
            assert_eq!(refused(submitted, roomy()), UnboundRevision, "{marked}");
        }
    }

    /// `admit` reads the five bounds of the capability's
    /// `bootstrapOnly.evidenceMetadata` that the boundary applies.
    #[test]
    fn admit_reads_the_capability_bounds() {
        let revision = revision();
        let capability = |name: &str, value: usize| {
            let mut raw = ImplementationCapability::fixture_owned(fixture_limits(1)).normalized();
            let bounds = &mut raw["bootstrapOnly"]["evidenceMetadata"];
            for (_, bound) in bounds.as_object_mut().unwrap() {
                *bound = json!((u64::MAX - 1).to_string());
            }
            bounds[name] = Value::String(value.to_string());
            let constructed = VersionedIdentity {
                identity: CAPABILITY,
                revision: CAPABILITY_REVISION,
            };
            crate::boundary::capability::admit(Some(&raw), &constructed).unwrap()
        };
        let report = || {
            Some(CallSiteReport::Formal(sidecar(
                &revision,
                vec![node(vec![]), node(vec![key("a")])],
            )))
        };
        for (name, value) in [
            ("maxSourceIdentityBytes", IDENTITY.len()),
            ("maxSourceRevisionBytes", revision.len()),
            ("maxCallSiteSourceBytes", SOURCE.len()),
            ("maxCallSiteMappings", 2),
        ] {
            assert!(
                matches!(admit(report(), &capability(name, value)), Ok(Some(_))),
                "{name}"
            );
            assert_eq!(
                admit(report(), &capability(name, value - 1)).err(),
                Some(OverBoundInput),
                "{name}"
            );
        }
        let locator = |value| {
            let Ok(Some(admitted)) = admit(report(), &capability("maxLocatorBytes", value)) else {
                panic!("the sidecar is admitted");
            };
            admitted
                .locator()
                .map(|locator| locator.payload().to_owned())
        };
        assert_eq!(locator(LABEL.len()), Some(LABEL.to_owned()));
        assert_eq!(locator(LABEL.len() - 1), None);
    }

    #[test]
    fn each_refusal_has_its_reason() {
        use CallSiteRefusal as R;
        assert_eq!(
            [
                R::UnsafeHostInspection,
                R::OverBoundInput,
                R::MalformedInput,
                R::InvalidUtf8Source,
                R::DuplicateMapping,
                R::MissingIdentity,
                R::MissingRevision,
                R::UnstableIdentity,
                R::UnboundRevision,
                R::SnapshotMismatch,
            ]
            .map(R::reason),
            [
                "unsafe-host-inspection",
                "over-bound-input",
                "malformed-input",
                "invalid-utf8-source",
                "duplicate-mapping",
                "missing-source-identity",
                "missing-source-revision",
                "unstable-source-identity",
                "unbound-source-revision",
                "source-snapshot-mismatch",
            ]
        );
    }
}
