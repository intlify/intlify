// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Evidence record: one origin-bearing location that a Finding
//! Occurrence or Coverage Decision Evidence names, built from the parts plan
//! 025 PR 5d defines, with its kind, order, and view (D15:840-842, 2481,
//! 2492; Decision 015-187).
//!
//! Every record has one Safe Origin Kind, and its constructor projects every
//! part under that origin, so no part can name another. A configuration
//! origin gives either Entry Source Evidence, a source identity with an
//! optional locator, Logical input path, and span, or Materialized Value
//! Evidence, a source identity with a required Portable Value Locator and an
//! optional locator and span. A selector origin gives profile-selector
//! Evidence with one projection, and a resolver input gives Evidence with one
//! Resolver Input Component Path. The evidence kind follows from the origin
//! and, for configuration, from which of the two records it is.
//!
//! Evidence compares by Safe Origin Kind rank first. Configuration Evidence
//! then compares its source identity, its locator after absence, Entry Source
//! Evidence with its path before Materialized Value Evidence with its value
//! locator (Decision 015-187), and its span after absence. Selector Evidence
//! compares its projection and resolver-input Evidence its component path.
//! The derived order is that comparison. Nothing builds Evidence yet.

use serde_json::{Map, Value};

use crate::boundary::locator::Locator;
use crate::boundary::source::ExactIdentity;
use crate::input_limits::Bound;
use crate::model::ProfileId;
use crate::structural::selection::SelectorInput;

use super::evidence_component::{ComponentPath, ResolverInputComponentPath};
use super::evidence_path::{
    InputPath, LogicalInputPath, PortableValueLocator, Step, ValueLocation,
};
use super::evidence_selector::{SelectorEvidence, SelectorEvidenceInvariant, SelectorProjection};
use super::evidence_source::{PortableSourceIdentity, SourceIdentity, SourceSpan};
use super::evidence_token::{EvidenceTokenInvariant, EvidenceTokens, Redacted};
use super::origin::{EvidenceKind, SafeOriginKind};
use super::token::Specification;

/// A resolver bug that leaves a location without Evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EvidenceInvariant {
    /// The origin cannot supply a part.
    Token(EvidenceTokenInvariant),
    /// The selector has no projection.
    Selector(SelectorEvidenceInvariant),
}

/// The configuration source one record names: its admitted exact identity,
/// whether its owner marked that identity non-secret, and only the locator
/// associated with that same identity (D15:840).
#[derive(Debug, Clone, Copy)]
pub(crate) struct Source<'a> {
    pub(crate) identity: &'a ExactIdentity,
    pub(crate) marked: bool,
    pub(crate) locator: Option<&'a Locator>,
}

/// Where a configuration record points inside its source.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Location {
    Entry(Option<InputPath>),
    Value(ValueLocation),
}

/// A record's parts as Evidence projects them, before tokens are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Parts {
    Configuration {
        source: SourceIdentity,
        locator: Option<Locator>,
        location: Location,
        span: Option<SourceSpan>,
    },
    Selector(SelectorProjection),
    ResolverInput(ComponentPath),
}

/// One Evidence record as Evidence projects it, before the invocation's
/// evidence tokens are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceProjection {
    origin: SafeOriginKind,
    parts: Parts,
}

impl EvidenceProjection {
    /// Entry Source Evidence: `source`, the Logical input path `path` when
    /// one was established, and the admitted span `span` (D15:780-786).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an origin that supplies no configuration.
    pub(crate) fn entry_source(
        origin: SafeOriginKind,
        source: Source<'_>,
        path: Option<&[Step<'_>]>,
        span: Option<SourceSpan>,
    ) -> Result<Self, EvidenceInvariant> {
        let path = path
            .map(|steps| InputPath::project(origin, steps))
            .transpose()
            .map_err(EvidenceInvariant::Token)?;
        Self::configuration(origin, source, |_| Ok(Location::Entry(path)), span)
    }

    /// Materialized Value Evidence: `source`, the required value locator of
    /// `value`, and the admitted span `span` (D15:790).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an origin that supplies no configuration.
    pub(crate) fn configuration_value(
        origin: SafeOriginKind,
        source: Source<'_>,
        value: &[Step<'_>],
        span: Option<SourceSpan>,
    ) -> Result<Self, EvidenceInvariant> {
        Self::configuration(
            origin,
            source,
            |origin| ValueLocation::project(origin, value).map(Location::Value),
            span,
        )
    }

    fn configuration(
        origin: SafeOriginKind,
        source: Source<'_>,
        location: impl FnOnce(SafeOriginKind) -> Result<Location, EvidenceTokenInvariant>,
        span: Option<SourceSpan>,
    ) -> Result<Self, EvidenceInvariant> {
        let identity = SourceIdentity::project(origin, source.identity, source.marked)
            .map_err(EvidenceInvariant::Token)?;
        let location = location(origin).map_err(EvidenceInvariant::Token)?;
        Ok(Self {
            origin,
            parts: Parts::Configuration {
                source: identity,
                locator: source.locator.cloned(),
                location,
                span,
            },
        })
    }

    /// Profile-selector Evidence: the projection of `input` under the final
    /// authority and the admitted declared Profile IDs (D15:804).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug that leaves the selector without a projection.
    pub(crate) fn selector<'d>(
        origin: SafeOriginKind,
        input: &SelectorInput,
        authority: Bound,
        declared: impl IntoIterator<Item = &'d ProfileId>,
    ) -> Result<Self, EvidenceInvariant> {
        let selector = SelectorProjection::project(origin, input, authority, declared)
            .map_err(EvidenceInvariant::Selector)?;
        Ok(Self {
            origin,
            parts: Parts::Selector(selector),
        })
    }

    /// Resolver-input Evidence: `path` inside the input its origin names
    /// (D15:821).
    pub(crate) fn resolver_input(path: ComponentPath) -> Self {
        Self {
            origin: path.origin(),
            parts: Parts::ResolverInput(path),
        }
    }

    /// Every component of the record that a token replaces.
    pub(crate) fn redacted(&self) -> Vec<&Redacted> {
        match &self.parts {
            Parts::Configuration {
                source, location, ..
            } => {
                let path: Vec<&Redacted> = match location {
                    Location::Entry(Some(path)) => path.redacted().collect(),
                    Location::Entry(None) => Vec::new(),
                    Location::Value(value) => value.redacted().collect(),
                };
                source.redacted().into_iter().chain(path).collect()
            }
            Parts::Selector(selector) => selector.redacted().into_iter().collect(),
            Parts::ResolverInput(path) => path.redacted().collect(),
        }
    }

    /// The Evidence the record retains, `None` when `tokens` was not
    /// assigned one of its components.
    pub(crate) fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<Evidence> {
        let body = match &self.parts {
            Parts::Configuration {
                source,
                locator,
                location,
                span,
            } => Body::Configuration {
                source: source.retained(specification, tokens)?,
                locator: locator.clone(),
                location: match location {
                    Location::Entry(None) => RetainedLocation::Entry(None),
                    Location::Entry(Some(path)) => {
                        RetainedLocation::Entry(Some(path.retained(specification, tokens)?))
                    }
                    Location::Value(value) => {
                        RetainedLocation::Value(value.retained(specification, tokens)?)
                    }
                },
                span: *span,
            },
            Parts::Selector(selector) => Body::Selector(selector.retained(specification, tokens)?),
            Parts::ResolverInput(path) => {
                Body::ResolverInput(path.retained(specification, tokens)?)
            }
        };
        Some(Evidence {
            origin: self.origin,
            body,
        })
    }
}

/// Where a retained configuration record points: Entry Source Evidence and
/// its optional Logical input path before Materialized Value Evidence and its
/// value locator (Decision 015-187).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum RetainedLocation {
    Entry(Option<LogicalInputPath>),
    Value(PortableValueLocator),
}

/// A retained record's parts, in 015's order after the origin.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Body {
    Configuration {
        source: PortableSourceIdentity,
        locator: Option<Locator>,
        location: RetainedLocation,
        span: Option<SourceSpan>,
    },
    Selector(SelectorEvidence),
    ResolverInput(ResolverInputComponentPath),
}

/// A retained Evidence record, in 015's order: its Safe Origin Kind rank,
/// then its parts.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Evidence {
    origin: SafeOriginKind,
    body: Body,
}

impl Evidence {
    /// The record's Safe Origin Kind.
    pub(crate) const fn origin(&self) -> SafeOriginKind {
        self.origin
    }

    /// The record's evidence kind: the one its origin permits, or for a
    /// configuration origin whether it is Entry Source or Materialized Value
    /// Evidence.
    pub(crate) fn kind(&self) -> EvidenceKind {
        match &self.body {
            Body::Configuration {
                location: RetainedLocation::Entry(_),
                ..
            } => EvidenceKind::EntrySource,
            Body::Configuration { .. } => EvidenceKind::ConfigurationValue,
            Body::Selector(_) | Body::ResolverInput(_) => self.origin.permitted_evidence_kinds()[0],
        }
    }

    /// `{ kind, origin, source?, locator?, path?, value?, span?, selector?,
    /// component? }` with only the members the record's kind has (D15:2481,
    /// 2492).
    pub(crate) fn view(&self) -> Value {
        let mut view = Map::new();
        view.insert("kind".to_owned(), Value::from(self.kind().id()));
        view.insert("origin".to_owned(), Value::from(self.origin.id()));
        match &self.body {
            Body::Configuration {
                source,
                locator,
                location,
                span,
            } => {
                view.insert("source".to_owned(), source.view());
                if let Some(locator) = locator {
                    view.insert("locator".to_owned(), locator.view());
                }
                match location {
                    RetainedLocation::Entry(Some(path)) => {
                        view.insert("path".to_owned(), path.view());
                    }
                    RetainedLocation::Entry(None) => {}
                    RetainedLocation::Value(value) => {
                        view.insert("value".to_owned(), value.view());
                    }
                }
                if let Some(span) = span {
                    view.insert("span".to_owned(), span.view());
                }
            }
            Body::Selector(selector) => {
                view.insert("selector".to_owned(), selector.view());
            }
            Body::ResolverInput(path) => {
                view.insert("component".to_owned(), path.view());
            }
        }
        Value::Object(view)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::boundary::locator::{read_file_locator, LocatorCandidate, LocatorVariant};
    use crate::boundary::source::{file_source_revision, read_pair, ExactIdentity, SourceLimits};
    use crate::input_limits::Bound;
    use crate::model::ProfileId;
    use crate::resolver::evidence_component::{ComponentPath, VerificationRole};
    use crate::resolver::evidence_path::Step;
    use crate::resolver::evidence_source::SourceSpan;
    use crate::resolver::evidence_token::{EvidenceTokenInvariant, EvidenceTokens, TokenKind};
    use crate::resolver::origin::{EvidenceKind, SafeOriginKind as O};
    use crate::resolver::specification::Slot;
    use crate::resolver::token::{Sha256, Specification};
    use crate::structural::selection::SelectorInput;

    use super::{Evidence, EvidenceInvariant, EvidenceProjection, Source};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };
    const FILE: &str = "intlify-file-source.v0:storefront/intlify.config.json";
    /// PR 5d-1's vector, computed apart from this crate: the token of an
    /// unmarked `intlify-file-source.v0` identity under `configuration-file`.
    const SOURCE_TOKEN: &str =
        "sha256:0b14760705ef603246b0f47ea23829ebfcccc8a47da7bc7fc19fc4b0d291612f";
    /// The same identity's token under `configuration-programmatic`, computed
    /// by `.plans/drafts/025-pr5d6/vectors.py`, and PR 5d-1's tokens of a
    /// `logical-input-path-member` of `configuration-programmatic` and of a
    /// `value-path-member` of `configuration-file`.
    const PROGRAMMATIC_SOURCE_TOKEN: &str =
        "sha256:96ce94ae827575cb02287dd649fe9302ecbd4376acf8549882ef9e272d3577d7";
    const INPUT_MEMBER: &str =
        "sha256:0856f1e9b4249ebb1eca3f368db3d3e9ecbc208aac95ec20f079981da86dc0a8";
    const VALUE_MEMBER: &str =
        "sha256:663eb6254e94a3289fce71ce1f181e96c1aa4152eaa2a82784966b55d5ce8b43";

    fn exact(text: &str) -> ExactIdentity {
        let largest = Bound::new(u64::MAX - 1).unwrap();
        let limits = SourceLimits {
            identity_bytes: largest,
            revision_bytes: largest,
        };
        read_pair(Some(text), Some(&file_source_revision(b"")), b"", limits)
            .unwrap()
            .identity()
            .clone()
    }

    fn locator(payload: &str) -> crate::boundary::locator::Locator {
        read_file_locator(
            Some(LocatorCandidate {
                variant: LocatorVariant::RelativePath,
                payload,
                non_secret: true,
            }),
            Bound::new(u64::MAX - 1).unwrap(),
        )
        .unwrap()
    }

    /// The retained records of `records`, their tokens assigned together.
    fn retain(records: &[EvidenceProjection]) -> Vec<Evidence> {
        let tokens = EvidenceTokens::assign(
            PROFILE,
            records.iter().flat_map(EvidenceProjection::redacted),
            &Sha256,
        );
        records
            .iter()
            .map(|record| record.retained(PROFILE, &tokens).unwrap())
            .collect()
    }

    fn views(records: &[EvidenceProjection]) -> Vec<serde_json::Value> {
        retain(records).iter().map(Evidence::view).collect()
    }

    #[test]
    fn entry_source_evidence_keeps_only_what_it_was_given() {
        let identity = exact(FILE);
        let at = locator("storefront/intlify.config.json");
        let snapshot = [0_u8; 40];
        let full = EvidenceProjection::entry_source(
            O::ConfigurationFile,
            Source {
                identity: &identity,
                marked: true,
                locator: Some(&at),
            },
            Some(&[Step::Safe("profiles"), Step::Index(0)]),
            SourceSpan::admit(3, 9, &snapshot),
        )
        .unwrap();
        let bare = EvidenceProjection::entry_source(
            O::ConfigurationFile,
            Source {
                identity: &identity,
                marked: false,
                locator: None,
            },
            None,
            None,
        )
        .unwrap();
        assert_eq!(
            views(&[full, bare]),
            [
                json!({
                    "kind": "entry-source",
                    "origin": "configuration-file",
                    "source": { "safe-opaque": "696e746c6966792d66696c652d736f757263652e76303a73746f726566726f6e742f696e746c6966792e636f6e6669672e6a736f6e" },
                    "locator": { "relative-path": "storefront/intlify.config.json" },
                    "path": [{ "safe-member": "profiles" }, { "array-index": "0" }],
                    "span": { "start": "3", "end": "9" },
                }),
                json!({
                    "kind": "entry-source",
                    "origin": "configuration-file",
                    "source": { "redacted": { "token": SOURCE_TOKEN } },
                }),
            ]
        );
    }

    #[test]
    fn materialized_value_evidence_always_has_its_value_locator() {
        let identity = exact(FILE);
        let record = EvidenceProjection::configuration_value(
            O::ConfigurationFile,
            Source {
                identity: &identity,
                marked: true,
                locator: None,
            },
            &[],
            None,
        )
        .unwrap();
        let retained = retain(&[record]);
        assert_eq!(retained[0].kind(), EvidenceKind::ConfigurationValue);
        assert_eq!(
            retained[0].view()["value"],
            json!({ "exact-json-pointer": "" })
        );
        assert!(retained[0].view().get("path").is_none());
    }

    #[test]
    fn selector_and_resolver_input_evidence_take_their_origins_kind() {
        let declared: Vec<ProfileId> = vec![serde_json::from_value(json!("app")).unwrap()];
        let records = [
            EvidenceProjection::selector(
                O::ProfileSelectorCli,
                &SelectorInput::string("app", Bound::new(64).unwrap()),
                Bound::new(64).unwrap(),
                &declared,
            )
            .unwrap(),
            EvidenceProjection::resolver_input(ComponentPath::specification(Slot::Coverage)),
            EvidenceProjection::resolver_input(
                ComponentPath::root(O::LocaleCanonicalizationData).unwrap(),
            ),
            EvidenceProjection::resolver_input(ComponentPath::verification(VerificationRole::Mode)),
        ];
        assert_eq!(
            views(&records),
            [
                json!({
                    "kind": "profile-selector",
                    "origin": "profile-selector-cli",
                    "selector": { "safe-profile-id": "app" },
                }),
                json!({
                    "kind": "specification-input",
                    "origin": "profile-resolution-specification-input",
                    "component": [{ "fixed-role": "coverage" }],
                }),
                json!({
                    "kind": "artifact-input",
                    "origin": "locale-canonicalization-data",
                    "component": [],
                }),
                json!({
                    "kind": "verification-input",
                    "origin": "resource-limit-policy-verification",
                    "component": [{ "fixed-role": "mode" }],
                }),
            ]
        );
    }

    /// Every part is projected under the record's one origin.
    #[test]
    fn a_part_cannot_take_another_origin() {
        let identity = exact(FILE);
        let source = Source {
            identity: &identity,
            marked: true,
            locator: None,
        };
        assert_eq!(
            EvidenceProjection::entry_source(O::ProfileSelectorCli, source, None, None),
            Err(EvidenceInvariant::Token(
                EvidenceTokenInvariant::OriginOutsideKind(
                    TokenKind::SourceIdentity,
                    O::ProfileSelectorCli
                )
            ))
        );
        assert!(matches!(
            EvidenceProjection::selector(
                O::ConfigurationFile,
                &SelectorInput::absent(Bound::new(64).unwrap()),
                Bound::new(64).unwrap(),
                &[],
            ),
            Err(EvidenceInvariant::Selector(_))
        ));
    }

    /// Every token of a record, its source's and its path members', is
    /// assigned with every other record's, under the record's origin.
    #[test]
    fn every_token_of_a_record_is_its_origins() {
        let identity = exact(FILE);
        let source = Source {
            identity: &identity,
            marked: false,
            locator: None,
        };
        let entry = EvidenceProjection::entry_source(
            O::ConfigurationProgrammatic,
            source,
            Some(&[Step::Unsafe]),
            None,
        )
        .unwrap();
        let value = EvidenceProjection::configuration_value(
            O::ConfigurationFile,
            Source {
                marked: true,
                ..source
            },
            &[Step::Safe("profiles"), Step::Unsafe],
            None,
        )
        .unwrap();
        assert_eq!(entry.redacted().len(), 2);
        let tokens = EvidenceTokens::assign(PROFILE, [], &Sha256);
        assert_eq!(entry.retained(PROFILE, &tokens), None);
        let views = views(&[entry, value]);
        assert_eq!(
            views[0]["source"],
            json!({ "redacted": { "token": PROGRAMMATIC_SOURCE_TOKEN } })
        );
        assert_eq!(
            views[0]["path"],
            json!([{ "redacted-member": { "token": INPUT_MEMBER } }])
        );
        assert_eq!(
            views[1]["value"],
            json!({ "redacted-logical-path": [
                { "safe-member": "profiles" },
                { "redacted-member": { "token": VALUE_MEMBER } },
            ] })
        );
    }

    /// Origin rank first; then source, locator after absence, Entry Source
    /// before Materialized Value Evidence, and span after absence.
    #[test]
    fn evidence_compares_by_origin_then_its_parts() {
        let identity = exact(FILE);
        let at = locator("a.json");
        let snapshot = [0_u8; 10];
        let entry = |marked, locator, path: Option<&[Step<'_>]>, span| {
            EvidenceProjection::entry_source(
                O::ConfigurationFile,
                Source {
                    identity: &identity,
                    marked,
                    locator,
                },
                path,
                span,
            )
            .unwrap()
        };
        let value = EvidenceProjection::configuration_value(
            O::ConfigurationFile,
            Source {
                identity: &identity,
                marked: true,
                locator: None,
            },
            &[],
            None,
        )
        .unwrap();
        let records = [
            EvidenceProjection::resolver_input(ComponentPath::specification(Slot::Coverage)),
            value,
            entry(false, None, None, None),
            entry(true, Some(&at), None, None),
            entry(true, None, Some(&[]), SourceSpan::admit(1, 2, &snapshot)),
            entry(true, None, Some(&[]), None),
            entry(true, None, None, None),
        ];
        let mut retained = retain(&records);
        retained.sort();
        let summary: Vec<_> = retained
            .iter()
            .map(|evidence| {
                let view = evidence.view();
                json!([
                    view["kind"],
                    view["source"]
                        .as_object()
                        .map(|source| source.keys().next().cloned()),
                    view.get("locator").is_some(),
                    view.get("path").cloned(),
                    view.get("span").is_some(),
                ])
            })
            .collect();
        assert_eq!(
            summary,
            [
                json!(["entry-source", "safe-opaque", false, null, false]),
                json!(["entry-source", "safe-opaque", false, [], false]),
                json!(["entry-source", "safe-opaque", false, [], true]),
                json!(["configuration-value", "safe-opaque", false, null, false]),
                json!(["entry-source", "safe-opaque", true, null, false]),
                json!(["entry-source", "redacted", false, null, false]),
                json!(["specification-input", null, false, null, false]),
            ]
        );

        // The origin decides before the parts: a programmatic source whose
        // bytes sort first still follows every file source.
        let later = exact("a.v0:a");
        let earlier = exact("b.v0:z");
        let record = |origin, identity: &ExactIdentity| {
            EvidenceProjection::entry_source(
                origin,
                Source {
                    identity,
                    marked: true,
                    locator: None,
                },
                None,
                None,
            )
            .unwrap()
        };
        let mut pair = retain(&[
            record(O::ConfigurationProgrammatic, &later),
            record(O::ConfigurationFile, &earlier),
        ]);
        pair.sort();
        assert_eq!(pair[0].view()["origin"], json!("configuration-file"));
    }
}
