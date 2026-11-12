// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's Evidence paths: the Logical input path that locates Entry
//! Source Evidence before a complete value exists, and the Portable Value
//! Locator that locates Materialized Value Evidence inside the materialized
//! value, with the order and view of each (D15:755-758, 770-776, 842,
//! 2497-2499).
//!
//! The resolver walks an internal path, the exact member names and array
//! indices from the root, which never leaves it. Projection receives each
//! member already classified: a non-secret member with its exact name, and
//! an unsafe one without it, so no unsafe name can be retained. A non-secret
//! member stays a `safe-member` and an array index an `array-index`; an
//! unsafe member becomes a `redacted-member`, the Redacted evidence token of
//! the input that supplied it under its path's token kind. A token keeps only
//! that the member is a string, so the unsafe members of one origin and path
//! kind share one token.
//!
//! A value locator whose members are all non-secret is the exact RFC 6901
//! JSON Pointer, `~` written `~0` and `/` written `~1`. Any unsafe member
//! makes it a redacted logical path instead, which is not a pointer and
//! cannot be dereferenced. An empty path is the input root: the empty pointer,
//! or the present empty Logical input path, which differs from its absence.
//!
//! Segment rank is `safe-member`, `array-index`, then `redacted-member`;
//! names compare by exact UTF-8 bytes, indices numerically, and tokens by
//! complete digest bytes and then the collision ordinal, absence first. Paths
//! compare segment by segment with a shorter equal prefix first, so the root
//! precedes every other path, and a Logical input path's absence precedes
//! them all. Value locators rank `exact-json-pointer` before
//! `redacted-logical-path`, and pointers compare by their UTF-8 bytes. The
//! derived orders here are those comparisons. Nothing builds Evidence yet.

use std::sync::Arc;

use serde_json::{json, Value};

use super::evidence_token::{EvidenceTokenInvariant, EvidenceTokens, Redacted};
use super::origin::SafeOriginKind;
use super::token::{RedactedToken, Specification};

/// One edge of an internal path as projection receives it: a member
/// classified non-secret, with its exact name; a member that is not, without
/// its name; or an array index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Step<'a> {
    Safe(&'a str),
    Unsafe,
    Index(u64),
}

/// A path segment as Evidence projects it, before the invocation's evidence
/// tokens are assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Segment {
    SafeMember(Arc<str>),
    ArrayIndex(u64),
    RedactedMember(Redacted),
}

/// A retained path segment, in 015's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PathSegment {
    SafeMember(Arc<str>),
    ArrayIndex(u64),
    RedactedMember(RedactedToken),
}

impl PathSegment {
    /// `{ "safe-member": name }`, `{ "array-index": decimal }`, or
    /// `{ "redacted-member": Token }` (D15:2499).
    pub(crate) fn view(&self) -> Value {
        match self {
            Self::SafeMember(name) => json!({ "safe-member": name.as_ref() }),
            Self::ArrayIndex(index) => json!({ "array-index": index.to_string() }),
            Self::RedactedMember(token) => json!({ "redacted-member": token.view() }),
        }
    }
}

/// The segments of `steps`, each unsafe member replaced by `component`.
fn segments(steps: &[Step<'_>], component: &Redacted) -> Vec<Segment> {
    steps
        .iter()
        .map(|step| match *step {
            Step::Safe(name) => Segment::SafeMember(Arc::from(name)),
            Step::Unsafe => Segment::RedactedMember(component.clone()),
            Step::Index(index) => Segment::ArrayIndex(index),
        })
        .collect()
}

/// The components among `segments` that tokens replace.
fn components(segments: &[Segment]) -> impl Iterator<Item = &Redacted> {
    segments.iter().filter_map(|segment| match segment {
        Segment::RedactedMember(component) => Some(component),
        Segment::SafeMember(_) | Segment::ArrayIndex(_) => None,
    })
}

/// The retained segments of `segments`, `None` when `tokens` was not
/// assigned a component among them.
fn retain(
    segments: &[Segment],
    specification: Specification<'_>,
    tokens: &EvidenceTokens,
) -> Option<Vec<PathSegment>> {
    segments
        .iter()
        .map(|segment| match segment {
            Segment::SafeMember(name) => Some(PathSegment::SafeMember(Arc::clone(name))),
            Segment::ArrayIndex(index) => Some(PathSegment::ArrayIndex(*index)),
            Segment::RedactedMember(component) => tokens
                .token(specification, component)
                .map(PathSegment::RedactedMember),
        })
        .collect()
}

fn view(segments: &[PathSegment]) -> Value {
    Value::Array(segments.iter().map(PathSegment::view).collect())
}

/// A Logical input path as Evidence projects it, before tokens are
/// assigned. The empty path is the input root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InputPath(Vec<Segment>);

impl InputPath {
    /// Project `steps`, the internal path from the root that an input of
    /// `origin` supplied (D15:770-776).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an origin that supplies no Logical input
    /// path, whether or not a member is unsafe.
    pub(crate) fn project(
        origin: SafeOriginKind,
        steps: &[Step<'_>],
    ) -> Result<Self, EvidenceTokenInvariant> {
        let component = Redacted::input_path_member(origin)?;
        Ok(Self(segments(steps, &component)))
    }

    /// The components that tokens replace, one per unsafe member.
    pub(crate) fn redacted(&self) -> impl Iterator<Item = &Redacted> {
        components(&self.0)
    }

    /// The Logical input path Evidence retains, `None` when `tokens` was not
    /// assigned a component of the path.
    pub(crate) fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<LogicalInputPath> {
        retain(&self.0, specification, tokens).map(LogicalInputPath)
    }
}

/// A retained Logical input path, in 015's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct LogicalInputPath(Vec<PathSegment>);

impl LogicalInputPath {
    /// The array of its segments' views; the empty array is the input root
    /// (D15:2497).
    pub(crate) fn view(&self) -> Value {
        view(&self.0)
    }
}

/// A Portable Value Locator as Evidence projects it, before tokens are
/// assigned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ValueLocation {
    ExactJsonPointer(Arc<str>),
    RedactedLogicalPath(Vec<Segment>),
}

impl ValueLocation {
    /// Project `steps`, the internal path from the root of the value that an
    /// input of `origin` supplied: the exact pointer when every member is
    /// non-secret, and otherwise the redacted logical path (D15:755-758).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an origin that supplies no materialized
    /// value, whether or not a member is unsafe.
    pub(crate) fn project(
        origin: SafeOriginKind,
        steps: &[Step<'_>],
    ) -> Result<Self, EvidenceTokenInvariant> {
        let component = Redacted::value_path_member(origin)?;
        Ok(pointer(steps).map_or_else(
            || Self::RedactedLogicalPath(segments(steps, &component)),
            |pointer| Self::ExactJsonPointer(pointer.into()),
        ))
    }

    /// The components that tokens replace, one per unsafe member.
    pub(crate) fn redacted(&self) -> impl Iterator<Item = &Redacted> {
        let segments: &[Segment] = match self {
            Self::ExactJsonPointer(_) => &[],
            Self::RedactedLogicalPath(segments) => segments,
        };
        components(segments)
    }

    /// The Portable Value Locator Evidence retains, `None` when `tokens` was
    /// not assigned a component of the path.
    pub(crate) fn retained(
        &self,
        specification: Specification<'_>,
        tokens: &EvidenceTokens,
    ) -> Option<PortableValueLocator> {
        match self {
            Self::ExactJsonPointer(pointer) => {
                Some(PortableValueLocator::ExactJsonPointer(Arc::clone(pointer)))
            }
            Self::RedactedLogicalPath(segments) => retain(segments, specification, tokens)
                .map(PortableValueLocator::RedactedLogicalPath),
        }
    }
}

/// A retained Portable Value Locator, in 015's order.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum PortableValueLocator {
    ExactJsonPointer(Arc<str>),
    RedactedLogicalPath(Vec<PathSegment>),
}

impl PortableValueLocator {
    /// `{ "exact-json-pointer": text }` or `{ "redacted-logical-path":
    /// segment[] }` (D15:2498).
    pub(crate) fn view(&self) -> Value {
        match self {
            Self::ExactJsonPointer(pointer) => json!({ "exact-json-pointer": pointer.as_ref() }),
            Self::RedactedLogicalPath(segments) => {
                json!({ "redacted-logical-path": view(segments) })
            }
        }
    }
}

/// The RFC 6901 JSON Pointer of `steps`, or `None` when a member is unsafe:
/// each member written after `/` with `~` as `~0` and then `/` as `~1`, and
/// each index as its decimal digits.
fn pointer(steps: &[Step<'_>]) -> Option<String> {
    let mut text = String::new();
    for step in steps {
        text.push('/');
        match *step {
            Step::Safe(name) => text.push_str(&name.replace('~', "~0").replace('/', "~1")),
            Step::Index(index) => text.push_str(&index.to_string()),
            Step::Unsafe => return None,
        }
    }
    Some(text)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::resolver::evidence_token::{EvidenceTokenInvariant, EvidenceTokens, TokenKind};
    use crate::resolver::origin::SafeOriginKind as O;
    use crate::resolver::token::{Digest, Sha256, Specification};

    use super::{
        InputPath, LogicalInputPath, PathSegment, PortableValueLocator, Step, ValueLocation,
    };

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };
    /// PR 5d-1's vectors, computed apart from this crate: the token of a
    /// `logical-input-path-member` of `configuration-programmatic`, and of a
    /// `value-path-member` of `configuration-file`.
    const INPUT_MEMBER: &str =
        "sha256:0856f1e9b4249ebb1eca3f368db3d3e9ecbc208aac95ec20f079981da86dc0a8";
    const VALUE_MEMBER: &str =
        "sha256:663eb6254e94a3289fce71ce1f181e96c1aa4152eaa2a82784966b55d5ce8b43";

    /// A digest under which every record collides.
    struct Constant;

    impl Digest for Constant {
        fn digest(&self, _: &[u8]) -> [u8; 32] {
            [7; 32]
        }
    }

    fn input(origin: O, steps: &[Step<'_>]) -> InputPath {
        InputPath::project(origin, steps).unwrap()
    }

    fn value(origin: O, steps: &[Step<'_>]) -> ValueLocation {
        ValueLocation::project(origin, steps).unwrap()
    }

    /// The retained Logical input paths of `paths`, their tokens assigned
    /// together under `digest`.
    fn retain_inputs(paths: &[InputPath], digest: &impl Digest) -> Vec<LogicalInputPath> {
        let tokens =
            EvidenceTokens::assign(PROFILE, paths.iter().flat_map(InputPath::redacted), digest);
        paths
            .iter()
            .map(|path| path.retained(PROFILE, &tokens).unwrap())
            .collect()
    }

    fn retain_values(
        locations: &[ValueLocation],
        digest: &impl Digest,
    ) -> Vec<PortableValueLocator> {
        let tokens = EvidenceTokens::assign(
            PROFILE,
            locations.iter().flat_map(ValueLocation::redacted),
            digest,
        );
        locations
            .iter()
            .map(|location| location.retained(PROFILE, &tokens).unwrap())
            .collect()
    }

    const DUPLICATE: [Step<'static>; 4] = [
        Step::Safe("profiles"),
        Step::Safe("app"),
        Step::Unsafe,
        Step::Index(3),
    ];

    #[test]
    fn a_logical_input_path_keeps_safe_members_and_indices() {
        let path = input(O::ConfigurationProgrammatic, &DUPLICATE);
        assert_eq!(path.redacted().count(), 1);
        assert_eq!(
            retain_inputs(&[path], &Sha256)[0].view(),
            json!([
                { "safe-member": "profiles" },
                { "safe-member": "app" },
                { "redacted-member": { "token": INPUT_MEMBER } },
                { "array-index": "3" },
            ])
        );
    }

    /// The root is the present empty path; the empty member name is a member.
    #[test]
    fn the_root_and_the_empty_member_name_are_kept() {
        let paths = [
            input(O::ConfigurationFile, &[]),
            input(O::ConfigurationFile, &[Step::Safe("")]),
        ];
        let views: Vec<_> = retain_inputs(&paths, &Sha256)
            .iter()
            .map(LogicalInputPath::view)
            .collect();
        assert_eq!(views, [json!([]), json!([{ "safe-member": "" }])]);
    }

    /// Every unsafe member of one origin and path kind shares one token.
    #[test]
    fn unsafe_members_of_one_origin_share_a_token() {
        let paths = [
            input(O::ConfigurationFile, &[Step::Unsafe, Step::Unsafe]),
            input(O::ConfigurationProgrammatic, &[Step::Unsafe]),
        ];
        let retained = retain_inputs(&paths, &Sha256);
        let tokens: Vec<_> = retained
            .iter()
            .flat_map(|path| path.view().as_array().unwrap().clone())
            .collect();
        assert_eq!(tokens[0], tokens[1]);
        assert_ne!(tokens[0], tokens[2]);
        assert_eq!(
            tokens[2],
            json!({ "redacted-member": { "token": INPUT_MEMBER } })
        );
    }

    #[test]
    fn a_value_locator_is_the_exact_pointer_when_every_member_is_safe() {
        let locations = [
            value(O::ConfigurationFile, &[]),
            value(
                O::ConfigurationFile,
                &[Step::Safe("profiles"), Step::Safe("app"), Step::Index(0)],
            ),
            value(
                O::ConfigurationFile,
                &[
                    Step::Safe("a/b"),
                    Step::Safe("m~n"),
                    Step::Safe("~1"),
                    Step::Safe(""),
                ],
            ),
        ];
        assert_eq!(locations[1].redacted().count(), 0);
        let views: Vec<_> = retain_values(&locations, &Sha256)
            .iter()
            .map(PortableValueLocator::view)
            .collect();
        assert_eq!(
            views,
            [
                json!({ "exact-json-pointer": "" }),
                json!({ "exact-json-pointer": "/profiles/app/0" }),
                json!({ "exact-json-pointer": "/a~1b/m~0n/~01/" }),
            ]
        );
    }

    /// One unsafe member makes the whole locator a redacted logical path, the
    /// safe members and indices kept beside the token.
    #[test]
    fn one_unsafe_member_makes_a_redacted_logical_path() {
        let location = value(O::ConfigurationFile, &DUPLICATE);
        assert_eq!(location.redacted().count(), 1);
        assert_eq!(
            retain_values(&[location], &Sha256)[0].view(),
            json!({ "redacted-logical-path": [
                { "safe-member": "profiles" },
                { "safe-member": "app" },
                { "redacted-member": { "token": VALUE_MEMBER } },
                { "array-index": "3" },
            ] })
        );
    }

    /// The origin is checked whether or not a member is unsafe.
    #[test]
    fn only_a_configuration_origin_supplies_a_path() {
        for steps in [&[][..], &[Step::Safe("a")][..], &[Step::Unsafe][..]] {
            assert_eq!(
                InputPath::project(O::ProfileSelectorCli, steps),
                Err(EvidenceTokenInvariant::OriginOutsideKind(
                    TokenKind::InputPathMember,
                    O::ProfileSelectorCli
                )),
                "{steps:?}"
            );
            assert_eq!(
                ValueLocation::project(O::LocaleCanonicalizationData, steps),
                Err(EvidenceTokenInvariant::OriginOutsideKind(
                    TokenKind::ValuePathMember,
                    O::LocaleCanonicalizationData
                )),
                "{steps:?}"
            );
        }
    }

    #[test]
    fn a_path_without_its_token_is_not_retained() {
        let tokens = EvidenceTokens::assign(PROFILE, [], &Sha256);
        let unsafe_member = [Step::Unsafe];
        assert_eq!(
            input(O::ConfigurationFile, &unsafe_member).retained(PROFILE, &tokens),
            None
        );
        assert_eq!(
            value(O::ConfigurationFile, &unsafe_member).retained(PROFILE, &tokens),
            None
        );
        assert!(value(O::ConfigurationFile, &[Step::Safe("a")])
            .retained(PROFILE, &tokens)
            .is_some());
    }

    /// Segment rank, then names by bytes, indices numerically, and tokens by
    /// digest and then ordinal, absence first; a shorter equal prefix first,
    /// and absence before the root.
    #[test]
    fn logical_input_paths_compare_segment_by_segment() {
        let safe = |names: &[&str]| {
            let steps: Vec<_> = names.iter().map(|name| Step::Safe(name)).collect();
            retain_inputs(&[input(O::ConfigurationFile, &steps)], &Sha256).remove(0)
        };
        let index = |at| {
            retain_inputs(&[input(O::ConfigurationFile, &[Step::Index(at)])], &Sha256).remove(0)
        };
        let colliding = retain_inputs(
            &[
                input(O::ConfigurationFile, &[Step::Unsafe]),
                input(O::ConfigurationProgrammatic, &[Step::Unsafe]),
            ],
            &Constant,
        );
        let alone =
            retain_inputs(&[input(O::ConfigurationFile, &[Step::Unsafe])], &Constant).remove(0);
        let mut paths = vec![
            Some(colliding[1].clone()),
            Some(index(10)),
            Some(safe(&["b"])),
            None,
            Some(alone.clone()),
            Some(safe(&["a", "b"])),
            Some(index(9)),
            Some(safe(&[])),
            Some(colliding[0].clone()),
            Some(safe(&["a"])),
            Some(safe(&["B"])),
        ];
        paths.sort();
        assert_eq!(
            paths,
            [
                None,
                Some(safe(&[])),
                Some(safe(&["B"])),
                Some(safe(&["a"])),
                Some(safe(&["a", "b"])),
                Some(safe(&["b"])),
                Some(index(9)),
                Some(index(10)),
                Some(alone),
                Some(colliding[0].clone()),
                Some(colliding[1].clone()),
            ]
        );
    }

    /// `exact-json-pointer` first, compared by its UTF-8 bytes, then
    /// `redacted-logical-path`, compared segment by segment.
    #[test]
    fn value_locators_compare_by_variant_then_content() {
        let locations = [
            value(O::ConfigurationFile, &[Step::Safe("b")]),
            value(O::ConfigurationFile, &[Step::Unsafe]),
            value(O::ConfigurationFile, &[Step::Safe("a"), Step::Unsafe]),
            value(O::ConfigurationFile, &[Step::Safe("a~b")]),
            value(O::ConfigurationFile, &[]),
        ];
        let mut retained = retain_values(&locations, &Sha256);
        retained.sort();
        let views: Vec<_> = retained.iter().map(PortableValueLocator::view).collect();
        assert_eq!(
            views,
            [
                json!({ "exact-json-pointer": "" }),
                json!({ "exact-json-pointer": "/a~0b" }),
                json!({ "exact-json-pointer": "/b" }),
                json!({ "redacted-logical-path": [
                    { "safe-member": "a" },
                    { "redacted-member": { "token": VALUE_MEMBER } },
                ] }),
                json!({ "redacted-logical-path": [
                    { "redacted-member": { "token": VALUE_MEMBER } },
                ] }),
            ]
        );
    }

    #[test]
    fn a_segment_names_its_kind() {
        assert_eq!(
            PathSegment::ArrayIndex(u64::MAX).view(),
            json!({ "array-index": "18446744073709551615" })
        );
        assert_eq!(
            PathSegment::SafeMember("名前".into()).view(),
            json!({ "safe-member": "名前" })
        );
    }
}
