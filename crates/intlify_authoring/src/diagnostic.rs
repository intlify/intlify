// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Structured authoring diagnostics with deterministic ordering.
//!
//! These are component-owned reason families, not a frozen global registry and
//! not 019's common envelope. MF2 parser and semantic diagnostics keep their
//! owning codes rather than being relabelled into an authoring family, so a
//! reader can still tell a malformed message from a rejected authoring form.
//!
//! A diagnostic is distinct from the overall outcome. Retaining diagnostics
//! never turns a blocked result into a checked one, and any truncation is
//! explicit.

use intlify_shared_json::token::{valid_identity, IdentityFailure};

use crate::limits::LimitKind;
use crate::primitives::{ByteRange, Occurrence, PrimitiveError, SourceSnapshot};

/// Reason families owned by design 016.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ReasonFamily {
    /// Source, profile, inventory, or binding inputs are malformed,
    /// incompatible, or inconsistently attached.
    AuthoringInputInvalid,
    /// A recognized intrinsic or known UI surface uses an unsupported form.
    AuthoringFormUnsupported,
    /// Message source or reference identity cannot be enumerated finitely.
    AuthoringSourceDynamic,
    /// Metadata is malformed, multiply attached, ambiguously placed, or
    /// attempts to redefine an existing declaration.
    AuthoringMetadataInvalid,
    /// No permitted source-locale basis exists.
    AuthoringSourceLocaleMissing,
    /// The supplied locale fails the admitted canonicalization rules.
    AuthoringSourceLocaleInvalid,
    /// A required surface-class assignment is missing or outside the exact
    /// admitted vocabulary.
    AuthoringSurfaceClassInvalid,
    /// Missing, extra, or duplicate parameter names, or incompatible
    /// finite-alternative requirements.
    AuthoringParameterMismatch,
    /// New or moved source needs accepted identity associations.
    AuthoringIdentityUpdateRequired,
    /// Several lineages compete, or plan and base inputs conflict.
    AuthoringIdentityConflict,
    /// A named limit was exceeded.
    AuthoringResourceLimit,
}

impl ReasonFamily {
    /// Return the exact wire spelling used for reporting and ordering.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AuthoringInputInvalid => "authoring-input-invalid",
            Self::AuthoringFormUnsupported => "authoring-form-unsupported",
            Self::AuthoringSourceDynamic => "authoring-source-dynamic",
            Self::AuthoringMetadataInvalid => "authoring-metadata-invalid",
            Self::AuthoringSourceLocaleMissing => "authoring-source-locale-missing",
            Self::AuthoringSourceLocaleInvalid => "authoring-source-locale-invalid",
            Self::AuthoringSurfaceClassInvalid => "authoring-surface-class-invalid",
            Self::AuthoringParameterMismatch => "authoring-parameter-mismatch",
            Self::AuthoringIdentityUpdateRequired => "authoring-identity-update-required",
            Self::AuthoringIdentityConflict => "authoring-identity-conflict",
            Self::AuthoringResourceLimit => "authoring-resource-limit",
        }
    }
}

/// The 016 operation that produced a diagnostic.
///
/// Phase 1 emits the three middle stages. Host discovery belongs to a Producer
/// and identity resolution to the registry operation; both are listed so the
/// ordering rule stays stable as those phases are implemented.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Stage {
    HostDiscovery,
    MessageAnalysis,
    ContextResolution,
    IdentityResolution,
    ResultConstruction,
}

impl Stage {
    /// Return the exact wire spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HostDiscovery => "host-discovery",
            Self::MessageAnalysis => "message-analysis",
            Self::ContextResolution => "context-resolution",
            Self::IdentityResolution => "identity-resolution",
            Self::ResultConstruction => "result-construction",
        }
    }
}

/// Severity of one structured record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Error,
    Warning,
    Information,
}

/// Which owner defined the reported reason.
///
/// Parser syntax and semantic failures keep their owning code. They are not
/// rewritten into an authoring family, because "this message is malformed" and
/// "this authoring form is unsupported" are different facts with different
/// remediation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DiagnosticOrigin {
    /// A reason family owned by 016.
    Authoring(ReasonFamily),
    /// One MF2 syntax diagnostic, by its parser-owned stable code.
    Mf2Syntax(&'static str),
    /// One MF2 semantic diagnostic, by its parser-owned stable code.
    Mf2Semantic(&'static str),
}

impl DiagnosticOrigin {
    /// Return the reason spelling used for reporting and ordering.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Authoring(family) => family.as_str(),
            Self::Mf2Syntax(code) | Self::Mf2Semantic(code) => code,
        }
    }

    // Ordering groups authoring reasons before parser-owned ones so a reader
    // sees the authoring decision before the message-level detail it caused.
    const fn rank(self) -> u8 {
        match self {
            Self::Authoring(_) => 0,
            Self::Mf2Syntax(_) => 1,
            Self::Mf2Semantic(_) => 2,
        }
    }
}

/// Which cause inside a reason family one record reports.
///
/// A reason family says what kind of mistake this is and what a reader should
/// do about it, and that is the part other components consume. Several
/// distinct causes share one family, so a test asserting only the family also
/// passes when a different cause fires. The detail exists so a fixture can
/// name the cause it actually means.
///
/// This is not a stable public code registry. Spellings are chosen by the
/// component that reports them and change with its implementation; nothing
/// outside this workspace should branch on one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Detail(&'static str);

impl Detail {
    /// Validate and retain one detail spelling.
    pub fn new(value: &'static str) -> Result<Self, IdentityFailure> {
        valid_identity(value)
            .then_some(Self(value))
            .ok_or(IdentityFailure::InvalidToken)
    }

    /// Retain one detail that the reporting component writes as a literal.
    ///
    /// # Panics
    ///
    /// Panics when the literal is outside the exact token grammar. Details are
    /// implementation constants, so an invalid one is a defect.
    #[must_use]
    pub fn literal(value: &'static str) -> Self {
        Self::new(value).expect("registered literal diagnostic detail")
    }

    /// Borrow the exact retained spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

/// The details this crate reports.
///
/// They are gathered here so one reader can see every cause this component
/// distinguishes, rather than finding them spread across reporting sites.
pub mod detail {
    use super::Detail;

    /// The message requires a parameter the use site did not supply.
    #[must_use]
    pub fn parameter_missing() -> Detail {
        Detail::literal("parameter-missing")
    }

    /// The use site supplied a name the message does not require.
    #[must_use]
    pub fn parameter_extra() -> Detail {
        Detail::literal("parameter-extra")
    }

    /// The use site supplied one name more than once.
    #[must_use]
    pub fn parameter_duplicate() -> Detail {
        Detail::literal("parameter-duplicate")
    }

    /// Displayed text contains a scalar MF2 pattern text cannot carry.
    #[must_use]
    pub fn unrepresentable_scalar() -> Detail {
        Detail::literal("unrepresentable-scalar")
    }

    /// No source-locale basis exists for this declaration.
    #[must_use]
    pub fn source_locale_absent() -> Detail {
        Detail::literal("source-locale-absent")
    }

    /// The authored locale failed the admitted canonicalization rules.
    #[must_use]
    pub fn source_locale_rejected() -> Detail {
        Detail::literal("source-locale-rejected")
    }

    /// Neither an explicit class nor an invocation default was present.
    #[must_use]
    pub fn surface_class_absent() -> Detail {
        Detail::literal("surface-class-absent")
    }

    /// The applicable class is not a member of the pinned vocabulary.
    #[must_use]
    pub fn surface_class_unknown() -> Detail {
        Detail::literal("surface-class-unknown")
    }

    /// A metadata value is empty or exceeds its bound.
    #[must_use]
    pub fn metadata_value_invalid() -> Detail {
        Detail::literal("metadata-value-invalid")
    }

    /// Semantic usage was supplied without a registered profile.
    #[must_use]
    pub fn usage_profile_unregistered() -> Detail {
        Detail::literal("usage-profile-unregistered")
    }

    /// The occurrence does not carry a declaration role.
    #[must_use]
    pub fn occurrence_role_invalid() -> Detail {
        Detail::literal("occurrence-role-invalid")
    }

    /// The occurrence belongs to an owner outside this invocation.
    #[must_use]
    pub fn occurrence_owner_foreign() -> Detail {
        Detail::literal("occurrence-owner-foreign")
    }
}

/// Which coordinate space a message range is expressed in.
///
/// A range inside a message can name the MF2 the parser saw or the text the
/// host decoded, and the two differ wherever encoding inserted or removed
/// bytes. Carrying the space with the range is what lets a reader pick the
/// right map instead of guessing which one the producer meant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum MessageRange {
    /// A range in the emitted MF2; map it back through the extraction map.
    Emitted(ByteRange),
    /// A range in the supplied text; map it through the host's input map.
    Supplied(ByteRange),
}

impl MessageRange {
    /// Return the addressed range, whichever space it names.
    #[must_use]
    pub const fn range(self) -> ByteRange {
        match self {
            Self::Emitted(range) | Self::Supplied(range) => range,
        }
    }
}

/// Where one record points.
///
/// Many authoring mistakes are not at a classified occurrence: an annotation
/// the compiler read and rejected, an import that bound an authoring intrinsic
/// in an unsupported way, or a unit whose bytes could not be parsed at all.
/// Giving those an inventory role would put syntax that produced no
/// declaration into the same vocabulary as syntax that did, and that
/// vocabulary is what an inventory admits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// One classified occurrence, in an inventory-admissible role.
    Occurrence(Occurrence),
    /// A region of one unit that carries no inventory role.
    Region(Region),
    /// One whole source unit, when no range inside it is meaningful.
    Unit(SourceSnapshot),
}

/// A region of one source unit that carries no inventory role.
///
/// The range is checked against the unit for the same reason
/// [`Occurrence::new`] checks its own: a consumer that resolves the position
/// against actual bytes would otherwise slice past the end of them. Carrying
/// the check on one of the two and not the other would make the guarantee
/// depend on which variant a reporter happened to pick.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    source: SourceSnapshot,
    range: ByteRange,
}

impl Region {
    /// Validate and retain one region against its own unit.
    pub fn new(source: SourceSnapshot, range: ByteRange) -> Result<Self, PrimitiveError> {
        if range.end() > source.byte_length() {
            return Err(PrimitiveError::RangeOutsideSource);
        }
        Ok(Self { source, range })
    }

    /// Borrow the unit this region belongs to.
    #[must_use]
    pub const fn source(&self) -> &SourceSnapshot {
        &self.source
    }

    /// Return the addressed half-open range.
    #[must_use]
    pub const fn range(&self) -> ByteRange {
        self.range
    }
}

impl From<Region> for Location {
    fn from(region: Region) -> Self {
        Self::Region(region)
    }
}

impl From<Occurrence> for Location {
    fn from(occurrence: Occurrence) -> Self {
        Self::Occurrence(occurrence)
    }
}

impl Location {
    /// Borrow the unit this record points into.
    #[must_use]
    pub const fn source(&self) -> &SourceSnapshot {
        match self {
            Self::Occurrence(occurrence) => occurrence.source(),
            Self::Region(region) => region.source(),
            Self::Unit(source) => source,
        }
    }

    /// Return the addressed range, when the record names one.
    #[must_use]
    pub const fn range(&self) -> Option<ByteRange> {
        match self {
            Self::Occurrence(occurrence) => Some(occurrence.range()),
            Self::Region(region) => Some(region.range()),
            Self::Unit(_) => None,
        }
    }

    /// Borrow the classified occurrence, when the record points at one.
    #[must_use]
    pub const fn occurrence(&self) -> Option<&Occurrence> {
        match self {
            Self::Occurrence(occurrence) => Some(occurrence),
            Self::Region(_) | Self::Unit(_) => None,
        }
    }

    // A whole-unit record sorts before every position inside that unit, which
    // is also how it reads: the unit could not be understood, so the positions
    // that follow are what was understood in spite of that.
    const fn rank(&self) -> u8 {
        match self {
            Self::Unit(_) => 0,
            Self::Region(_) => 1,
            Self::Occurrence(_) => 2,
        }
    }

    const fn role_spelling(&self) -> &'static str {
        match self {
            Self::Occurrence(occurrence) => occurrence.role().as_str(),
            Self::Region(_) | Self::Unit(_) => "",
        }
    }

    /// Compare two locations in 016's canonical reporting order.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> std::cmp::Ordering {
        let span = |location: &Self| location.range().map_or((0, 0), |r| (r.start(), r.end()));
        let (left_start, left_end) = span(self);
        let (right_start, right_end) = span(other);
        self.source()
            .canonical_cmp(other.source())
            .then_with(|| left_start.cmp(&right_start))
            .then_with(|| left_end.cmp(&right_end))
            .then_with(|| self.rank().cmp(&other.rank()))
            .then_with(|| {
                self.role_spelling()
                    .as_bytes()
                    .cmp(other.role_spelling().as_bytes())
            })
    }
}

/// One structured authoring record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    stage: Stage,
    origin: DiagnosticOrigin,
    detail: Option<Detail>,
    severity: Severity,
    location: Location,
    message_range: Option<MessageRange>,
    limit: Option<LimitKind>,
    related: Box<[Occurrence]>,
}

impl Diagnostic {
    /// Record one diagnostic at one location.
    #[must_use]
    pub fn new(
        stage: Stage,
        origin: DiagnosticOrigin,
        severity: Severity,
        location: impl Into<Location>,
    ) -> Self {
        Self {
            stage,
            origin,
            detail: None,
            severity,
            location: location.into(),
            message_range: None,
            limit: None,
            related: Box::new([]),
        }
    }

    /// Name the cause within the reason family.
    #[must_use]
    pub const fn with_detail(mut self, detail: Detail) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Attach the range inside the message that this record concerns.
    ///
    /// The range carries the space it is expressed in, because a reader has to
    /// choose between the extraction map and the host's input map to resolve
    /// it back to source.
    #[must_use]
    pub const fn with_message_range(mut self, range: MessageRange) -> Self {
        self.message_range = Some(range);
        self
    }

    /// Name the exhausted bound for a resource-limit reason.
    #[must_use]
    pub fn with_limit(mut self, limit: LimitKind) -> Self {
        self.limit = Some(limit);
        self
    }

    /// Attach related occurrences in the same admitted source domain.
    #[must_use]
    pub fn with_related(mut self, related: impl Into<Box<[Occurrence]>>) -> Self {
        self.related = related.into();
        self
    }

    /// Return the cause within the reason family, when one was named.
    #[must_use]
    pub const fn detail(&self) -> Option<Detail> {
        self.detail
    }

    /// Return the producing stage.
    #[must_use]
    pub const fn stage(&self) -> Stage {
        self.stage
    }

    /// Return the reason and its owner.
    #[must_use]
    pub const fn origin(&self) -> DiagnosticOrigin {
        self.origin
    }

    /// Return the severity.
    #[must_use]
    pub const fn severity(&self) -> Severity {
        self.severity
    }

    /// Borrow where this record points.
    #[must_use]
    pub const fn location(&self) -> &Location {
        &self.location
    }

    /// Borrow the reported occurrence, when this record points at one.
    ///
    /// A record about an annotation, an import, or a whole unit has no
    /// classified occurrence, so this returns `None` rather than inventing an
    /// inventory role for syntax that produced no declaration.
    #[must_use]
    pub const fn occurrence(&self) -> Option<&Occurrence> {
        self.location.occurrence()
    }

    /// Return the range inside the message this record concerns.
    ///
    /// The range names its own coordinate space. It is never an offset into
    /// the location's source unit: resolving it needs the extraction map, the
    /// host's input map, or both.
    #[must_use]
    pub const fn message_range(&self) -> Option<MessageRange> {
        self.message_range
    }

    /// Borrow the related occurrences, in the same admitted source domain.
    #[must_use]
    pub fn related(&self) -> &[Occurrence] {
        &self.related
    }

    /// Return the exhausted bound, when this is a resource-limit reason.
    #[must_use]
    pub const fn limit(&self) -> Option<LimitKind> {
        self.limit
    }

    /// Return whether this record blocks a checked result.
    #[must_use]
    pub const fn is_blocking(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }

    /// Compare two records in 016's deterministic reporting order.
    ///
    /// The order is stage, admitted source-unit identity and range, component
    /// reason and its detail, then a stable related-occurrence discriminator.
    /// It never depends on worker scheduling or hash-map iteration.
    #[must_use]
    pub fn reporting_cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.stage
            .cmp(&other.stage)
            .then_with(|| self.location.canonical_cmp(&other.location))
            .then_with(|| self.origin.rank().cmp(&other.origin.rank()))
            .then_with(|| {
                self.origin
                    .code()
                    .as_bytes()
                    .cmp(other.origin.code().as_bytes())
            })
            .then_with(|| {
                self.detail
                    .map(Detail::as_str)
                    .cmp(&other.detail.map(Detail::as_str))
            })
            .then_with(|| self.message_range.cmp(&other.message_range))
            .then_with(|| self.severity.cmp(&other.severity))
            .then_with(|| self.limit.cmp(&other.limit))
            .then_with(|| self.related.len().cmp(&other.related.len()))
            .then_with(|| {
                for (left, right) in self.related.iter().zip(other.related.iter()) {
                    let ordering = left.canonical_cmp(right);
                    if ordering != std::cmp::Ordering::Equal {
                        return ordering;
                    }
                }
                std::cmp::Ordering::Equal
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::{OccurrenceRole, OwnerIdentity, OwnerKind, SourceSnapshot};
    use intlify_shared_json::token::VersionedIdentity;

    fn occurrence(start: u64, end: u64, role: OccurrenceRole) -> Occurrence {
        let source = SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            "checkout",
            "1",
            VersionedIdentity::literal("intlify-js-grammar", "0"),
            512,
            &format!("sha256:{}", "0".repeat(64)),
        )
        .unwrap();
        Occurrence::new(source, ByteRange::new(start, end).unwrap(), role).unwrap()
    }

    #[test]
    fn reason_and_stage_spellings_are_distinct() {
        let families = [
            ReasonFamily::AuthoringInputInvalid,
            ReasonFamily::AuthoringFormUnsupported,
            ReasonFamily::AuthoringSourceDynamic,
            ReasonFamily::AuthoringMetadataInvalid,
            ReasonFamily::AuthoringSourceLocaleMissing,
            ReasonFamily::AuthoringSourceLocaleInvalid,
            ReasonFamily::AuthoringSurfaceClassInvalid,
            ReasonFamily::AuthoringParameterMismatch,
            ReasonFamily::AuthoringIdentityUpdateRequired,
            ReasonFamily::AuthoringIdentityConflict,
            ReasonFamily::AuthoringResourceLimit,
        ];
        let mut spellings: Vec<&str> = families.iter().map(|family| family.as_str()).collect();
        let total = spellings.len();
        spellings.sort_unstable();
        spellings.dedup();
        assert_eq!(spellings.len(), total);
        assert_eq!(Stage::MessageAnalysis.as_str(), "message-analysis");
    }

    #[test]
    fn parser_owned_codes_are_preserved_rather_than_relabelled() {
        let syntax = DiagnosticOrigin::Mf2Syntax("unclosed-expression");
        let semantic = DiagnosticOrigin::Mf2Semantic("duplicate-declaration");
        let authoring = DiagnosticOrigin::Authoring(ReasonFamily::AuthoringFormUnsupported);
        assert_eq!(syntax.code(), "unclosed-expression");
        assert_eq!(semantic.code(), "duplicate-declaration");
        assert_eq!(authoring.code(), "authoring-form-unsupported");
        assert_ne!(syntax, semantic);
    }

    #[test]
    fn reporting_order_is_deterministic_and_independent_of_insertion() {
        let build = |stage, origin, start, end| {
            Diagnostic::new(
                stage,
                origin,
                Severity::Error,
                occurrence(start, end, OccurrenceRole::UiLiteral),
            )
        };
        let mut records = vec![
            build(
                Stage::ContextResolution,
                DiagnosticOrigin::Authoring(ReasonFamily::AuthoringSourceLocaleMissing),
                0,
                4,
            ),
            build(
                Stage::MessageAnalysis,
                DiagnosticOrigin::Mf2Syntax("unclosed-expression"),
                10,
                14,
            ),
            build(
                Stage::MessageAnalysis,
                DiagnosticOrigin::Authoring(ReasonFamily::AuthoringParameterMismatch),
                10,
                14,
            ),
            build(
                Stage::MessageAnalysis,
                DiagnosticOrigin::Mf2Syntax("unclosed-expression"),
                2,
                6,
            ),
        ];
        let mut reversed = records.clone();
        reversed.reverse();
        records.sort_by(Diagnostic::reporting_cmp);
        reversed.sort_by(Diagnostic::reporting_cmp);
        assert_eq!(records, reversed);
        let order: Vec<(&str, &str, u64)> = records
            .iter()
            .map(|record| {
                (
                    record.stage().as_str(),
                    record.origin().code(),
                    record
                        .location()
                        .range()
                        .expect("every record here names a range")
                        .start(),
                )
            })
            .collect();
        assert_eq!(
            order,
            [
                ("message-analysis", "unclosed-expression", 2),
                ("message-analysis", "authoring-parameter-mismatch", 10),
                ("message-analysis", "unclosed-expression", 10),
                ("context-resolution", "authoring-source-locale-missing", 0),
            ]
        );
    }

    #[test]
    fn retained_evidence_is_readable_by_a_caller() {
        let subject = occurrence(0, 4, OccurrenceRole::UiLiteral);
        let sibling = occurrence(10, 14, OccurrenceRole::Reference);
        let plain = Diagnostic::new(
            Stage::MessageAnalysis,
            DiagnosticOrigin::Mf2Syntax("unclosed-expression"),
            Severity::Error,
            subject.clone(),
        );
        assert_eq!(plain.message_range(), None);
        assert!(plain.related().is_empty());

        assert_eq!(plain.detail(), None);
        assert_eq!(plain.occurrence(), Some(&subject));

        let range = ByteRange::new(6, 12).unwrap();
        let detailed = Diagnostic::new(
            Stage::ContextResolution,
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringParameterMismatch),
            Severity::Error,
            subject,
        )
        .with_detail(detail::parameter_extra())
        .with_message_range(MessageRange::Emitted(range))
        .with_related(vec![sibling.clone()]);
        assert_eq!(detailed.message_range(), Some(MessageRange::Emitted(range)));
        assert_eq!(
            detailed.detail().map(Detail::as_str),
            Some("parameter-extra")
        );
        assert_eq!(detailed.related(), [sibling]);
    }

    #[test]
    fn a_location_without_an_inventory_role_is_still_reportable_and_ordered() {
        let classified = occurrence(4, 8, OccurrenceRole::UiLiteral);
        let source = classified.source().clone();
        let annotation =
            Location::Region(Region::new(source.clone(), ByteRange::new(4, 8).unwrap()).unwrap());
        let whole = Location::Unit(source);

        // A region and a unit carry no classified occurrence, so a consumer
        // reading one cannot mistake the syntax for a declaration.
        assert_eq!(annotation.occurrence(), None);
        assert_eq!(whole.occurrence(), None);
        assert_eq!(whole.range(), None);
        assert_eq!(annotation.range(), Some(ByteRange::new(4, 8).unwrap()));

        // The unit sorts before every position inside it: what could not be
        // read at all comes before what was read in spite of it.
        let mut locations = [
            Location::Occurrence(classified),
            annotation.clone(),
            whole.clone(),
        ];
        locations.sort_by(Location::canonical_cmp);
        assert_eq!(locations[0], whole);
        assert_eq!(locations[1], annotation);
    }

    #[test]
    fn a_region_is_checked_against_its_own_unit() {
        // The same guarantee as a classified occurrence: a consumer resolving
        // the position against real bytes must not be handed a range that
        // runs past them. Which variant a reporter picked cannot decide
        // whether that holds.
        let source = occurrence(0, 4, OccurrenceRole::UiLiteral).source().clone();
        let length = source.byte_length();
        assert!(Region::new(source.clone(), ByteRange::new(0, length).unwrap()).is_ok());
        assert_eq!(
            Region::new(source, ByteRange::new(0, length + 1).unwrap()),
            Err(PrimitiveError::RangeOutsideSource),
            "one byte past the unit is refused, exactly as Occurrence::new refuses it"
        );
    }

    #[test]
    fn a_detail_uses_the_exact_token_grammar() {
        assert_eq!(
            Detail::literal("parameter-missing").as_str(),
            "parameter-missing"
        );
        for invalid in ["", "Parameter-Missing", "parameter missing", "-a", "a-"] {
            assert_eq!(Detail::new(invalid), Err(IdentityFailure::InvalidToken));
        }
    }

    #[test]
    fn only_errors_block_a_checked_result() {
        let base = occurrence(0, 4, OccurrenceRole::UiLiteral);
        let error = Diagnostic::new(
            Stage::MessageAnalysis,
            DiagnosticOrigin::Authoring(ReasonFamily::AuthoringResourceLimit),
            Severity::Error,
            base.clone(),
        )
        .with_limit(LimitKind::EmittedMf2Bytes);
        let warning = Diagnostic::new(
            Stage::MessageAnalysis,
            DiagnosticOrigin::Mf2Syntax("ambiguous-message-mode"),
            Severity::Warning,
            base,
        );
        assert!(error.is_blocking());
        assert_eq!(error.limit(), Some(LimitKind::EmittedMf2Bytes));
        assert!(!warning.is_blocking());
        assert_eq!(warning.limit(), None);
    }
}
