// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where a check of the selected declaration looks: a value under the
//! declaration, or a member's key there, as Materialized Value Evidence at the
//! token in the file Phase 1 read (D15:790, 802), and the configuration around
//! it, the root, the declarations, a declared Profile ID's key, every string
//! past a bound, and the selector.
//! The phases after profile selection observe their Findings through it.

use crate::input_limits::Bound;
use crate::materialize::{ByteSpan, MaterializedDocument, NodeId, NodeKind, ObjectMember};
use crate::structural::selection::SelectorInput;
use crate::structural::Edge;

use super::entry::{EntryFile, SELECTOR_ORIGIN};
use super::evaluation::Observation;
use super::evidence::{EvidenceInvariant, EvidenceProjection};

/// The declaration profile selection selected, in the file Phase 1 read and
/// the document it materialized.
pub(crate) struct DeclarationObserver<'a> {
    file: &'a EntryFile<'a>,
    document: &'a MaterializedDocument,
    profiles: NodeId,
    id: &'a str,
    declaration: NodeId,
}

/// A resolver bug the observer finds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ObserverInvariant {
    /// A value a check looked at is not in the document.
    Missing,
    /// What a check looked at has no Evidence.
    Evidence(EvidenceInvariant),
}

impl<'a> DeclarationObserver<'a> {
    /// The declaration `id` of `profiles`, the root's `profiles` object in
    /// `document`, which `file` holds.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a declaration the document lacks.
    pub(crate) fn new(
        file: &'a EntryFile<'a>,
        (document, profiles): (&'a MaterializedDocument, NodeId),
        id: &'a str,
    ) -> Result<Self, ObserverInvariant> {
        let declaration = member(document, profiles, id).ok_or(ObserverInvariant::Missing)?;
        Ok(Self {
            file,
            document,
            profiles,
            id,
            declaration,
        })
    }

    /// The value at the member path `path` under the declaration, at its
    /// token (D15:790).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a value the document lacks, or that has no
    /// Evidence.
    pub(crate) fn value(&self, path: &[&str]) -> Result<EvidenceProjection, ObserverInvariant> {
        let node = self.node(path)?;
        self.at(&self.edges(path, None), self.document.node(node).span())
    }

    /// The key of the member `name` of the object at the member path `path`
    /// under the declaration, at the key's token, which is classified by facts
    /// fixed when the record is constructed, never by a later check (D15:776,
    /// 790).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a key the document lacks, or that has no
    /// Evidence.
    pub(crate) fn member_key(
        &self,
        path: &[&str],
        name: &str,
    ) -> Result<EvidenceProjection, ObserverInvariant> {
        let NodeKind::Object(members) = self.document.node(self.node(path)?).kind() else {
            return Err(ObserverInvariant::Missing);
        };
        let member = members.get(name).ok_or(ObserverInvariant::Missing)?;
        self.at(&self.edges(path, Some(name)), member.key_span())
    }

    /// The node at the member path `path` under the declaration.
    fn node(&self, path: &[&str]) -> Result<NodeId, ObserverInvariant> {
        let mut node = self.declaration;
        for name in path {
            node = member(self.document, node, name).ok_or(ObserverInvariant::Missing)?;
        }
        Ok(node)
    }

    /// The internal path from the root of the member path `path` under the
    /// declaration, and then of its member `name` when there is one.
    fn edges<'e>(&'e self, path: &[&'e str], name: Option<&'e str>) -> Vec<Edge<'e>> {
        [Edge::Member("profiles"), Edge::Member(self.id)]
            .into_iter()
            .chain(path.iter().map(|name| Edge::Member(name)))
            .chain(name.map(Edge::Member))
            .collect()
    }

    /// The root value, at its token (D15:790).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a root that has no Evidence.
    pub(crate) fn root(&self) -> Result<EvidenceProjection, ObserverInvariant> {
        self.at(&[], self.document.node(self.document.root()).span())
    }

    /// The root's `profiles` object, the declarations as one location, at its
    /// token (D15:790, 812).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of declarations that have no Evidence.
    pub(crate) fn declarations(&self) -> Result<EvidenceProjection, ObserverInvariant> {
        self.at(
            &[Edge::Member("profiles")],
            self.document.node(self.profiles).span(),
        )
    }

    /// The key of the declared Profile ID `id` in the root's `profiles`
    /// object, at the key's token, which is classified by facts fixed when the
    /// record is constructed, never by a later check (D15:776, 790).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a key the document lacks, or that has no
    /// Evidence.
    pub(crate) fn key(&self, id: &str) -> Result<EvidenceProjection, ObserverInvariant> {
        let NodeKind::Object(members) = self.document.node(self.profiles).kind() else {
            return Err(ObserverInvariant::Missing);
        };
        let member = members.get(id).ok_or(ObserverInvariant::Missing)?;
        self.at(
            &[Edge::Member("profiles"), Edge::Member(id)],
            member.key_span(),
        )
    }

    /// Every string value and object key in the configuration longer than
    /// `limit` UTF-8 bytes, with its length, at its token: a key at its
    /// member, at the key's span (D15:790, 1884, 2203).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a string that has no Evidence.
    pub(crate) fn strings_past(
        &self,
        limit: u64,
    ) -> Result<Vec<(u64, EvidenceProjection)>, ObserverInvariant> {
        let mut found = Vec::new();
        self.walk(self.document.root(), &mut Vec::new(), limit, &mut found)?;
        Ok(found)
    }

    /// The strings past `limit` at `node`, reached through `path`, into
    /// `found`, in the document's order.
    fn walk(
        &self,
        node: NodeId,
        path: &mut Vec<Edge<'a>>,
        limit: u64,
        found: &mut Vec<(u64, EvidenceProjection)>,
    ) -> Result<(), ObserverInvariant> {
        let document: &'a MaterializedDocument = self.document;
        let located = document.node(node);
        match located.kind() {
            NodeKind::String(text) => {
                let bytes = utf8_bytes(text);
                if bytes > limit {
                    found.push((bytes, self.at(path, located.span())?));
                }
            }
            NodeKind::Array(elements) => {
                for (index, element) in (0..).zip(elements) {
                    path.push(Edge::Index(index));
                    self.walk(*element, path, limit, found)?;
                    path.pop();
                }
            }
            NodeKind::Object(members) => {
                for (name, member) in members {
                    path.push(Edge::Member(name));
                    let bytes = utf8_bytes(name);
                    if bytes > limit {
                        found.push((bytes, self.at(path, member.key_span())?));
                    }
                    self.walk(member.value(), path, limit, found)?;
                    path.pop();
                }
            }
            NodeKind::Null | NodeKind::Boolean(_) | NodeKind::Number(_) => {}
        }
        Ok(())
    }

    /// The selector `input`, projected under its final authority `authority`,
    /// beside the declarations when they are `available` (D15:812, 816; plan
    /// 025 interpretation #215). A selector past the bound names no
    /// declaration (D15:807).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a selector without a projection under the
    /// authority, or of declarations that have no Evidence.
    pub(crate) fn selector(
        &self,
        input: &SelectorInput,
        authority: Bound,
        available: bool,
    ) -> Result<Observation, ObserverInvariant> {
        let primary =
            EvidenceProjection::selector(SELECTOR_ORIGIN, input, authority, std::iter::empty())
                .map_err(ObserverInvariant::Evidence)?;
        let related = if available {
            vec![self.declarations()?]
        } else {
            Vec::new()
        };
        Ok(Observation { primary, related })
    }

    /// The value at the internal path `edges` from the root, at the token
    /// `span`.
    fn at(
        &self,
        edges: &[Edge<'_>],
        span: ByteSpan,
    ) -> Result<EvidenceProjection, ObserverInvariant> {
        self.file
            .value_evidence(edges, span)
            .map_err(ObserverInvariant::Evidence)
    }
}

/// The UTF-8 byte length of `text`.
fn utf8_bytes(text: &str) -> u64 {
    u64::try_from(text.len()).unwrap_or(u64::MAX)
}

/// The value of the member `name` of the object `node`.
fn member(document: &MaterializedDocument, node: NodeId, name: &str) -> Option<NodeId> {
    match document.node(node).kind() {
        NodeKind::Object(members) => members.get(name).map(ObjectMember::value),
        _ => None,
    }
}

/// What the tests of the phases after profile selection observe the
/// declaration `app` of a configuration through.
#[cfg(test)]
pub(crate) mod fixtures {
    use std::sync::Arc;

    use serde_json::Value;

    use crate::materialize::{materialize_file, MaterializedDocument, NodeId};
    use crate::materialize_tests::limits;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evidence::EvidenceProjection;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::token::{Sha256, Specification};

    use super::{member, DeclarationObserver};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// The view of `projection`, under the tokens of its own redacted
    /// components.
    pub(crate) fn view(projection: &EvidenceProjection) -> Value {
        let tokens = EvidenceTokens::assign(PROFILE, projection.redacted(), &Sha256);
        projection.retained(PROFILE, &tokens).unwrap().view()
    }

    /// `config` as a file holds it, and the document it materializes with
    /// the root's `profiles` object.
    pub(crate) fn materialized(config: &Value) -> (Arc<[u8]>, MaterializedDocument, NodeId) {
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(config).unwrap());
        let document = materialize_file(Arc::clone(&bytes), limits()).unwrap();
        let profiles = member(&document, document.root(), "profiles").unwrap();
        (bytes, document, profiles)
    }

    /// A configuration as a test file holds it, admitted under bounds that
    /// classify every identifier key of a test non-secret.
    pub(crate) struct Observed {
        bytes: Arc<[u8]>,
        document: MaterializedDocument,
        profiles: NodeId,
        fixture: FileFixture,
    }

    impl Observed {
        /// A configuration whose one declaration, `app`, is `declaration` as
        /// a file writes it.
        pub(crate) fn declaration(declaration: &impl serde::Serialize) -> Self {
            Self::new(&serde_json::json!({ "profiles": { "app": declaration } }))
        }

        pub(crate) fn new(config: &Value) -> Self {
            let (bytes, document, profiles) = materialized(config);
            Self {
                bytes,
                document,
                profiles,
                fixture: FileFixture::new(&fixture_limits(10_000)),
            }
        }

        /// What `observe` makes of the observer of the declaration `app`.
        pub(crate) fn app<R>(&self, observe: impl FnOnce(&DeclarationObserver<'_>) -> R) -> R {
            let file = self.fixture.file(Arc::clone(&self.bytes));
            let observer =
                DeclarationObserver::new(&file, (&self.document, self.profiles), "app").unwrap();
            observe(&observer)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::fixtures::{complete_config, minimal_config};
    use crate::input_limits::Bound;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::limits::fixture_limits;
    use crate::structural::selection::SelectorInput;

    use super::fixtures::{materialized, view};
    use super::{DeclarationObserver, ObserverInvariant};

    /// The view of the span `value` occupies in `source`, at its first
    /// occurrence.
    fn span(source: &[u8], value: &Value) -> Value {
        let text = serde_json::to_vec(value).unwrap();
        let start = source
            .windows(text.len())
            .position(|window| window == text)
            .unwrap();
        json!({ "start": start.to_string(), "end": (start + text.len()).to_string() })
    }

    /// A value under the declaration is observed at its token, at the
    /// pointer of its path from the root (D15:790).
    #[test]
    fn a_value_under_the_declaration_is_observed_at_its_token() {
        let config = minimal_config();
        let (bytes, document, profiles) = materialized(&config);
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(Arc::clone(&bytes));
        let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        for (path, pointer, value) in [
            (&[][..], "/profiles/app", &config["profiles"]["app"]),
            (
                &["policies", "resourceLimits"][..],
                "/profiles/app/policies/resourceLimits",
                &config["profiles"]["app"]["policies"]["resourceLimits"],
            ),
        ] {
            let view = view(&observer.value(path).unwrap());
            assert_eq!(view["kind"], "configuration-value", "{view}");
            assert_eq!(view["origin"], "configuration-file", "{view}");
            assert_eq!(view["value"], json!({ "exact-json-pointer": pointer }));
            assert_eq!(view["span"], span(&bytes, value));
        }
    }

    /// The root, the declarations, and a declared Profile ID's key are each
    /// observed at its token: the key at the member, at the key's span
    /// (D15:776, 790).
    #[test]
    fn the_root_the_declarations_and_a_key_are_observed_at_their_tokens() {
        let config = minimal_config();
        let (bytes, document, profiles) = materialized(&config);
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(Arc::clone(&bytes));
        let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        for (observed, pointer, text) in [
            (observer.root(), "", serde_json::to_vec(&config).unwrap()),
            (
                observer.declarations(),
                "/profiles",
                serde_json::to_vec(&config["profiles"]).unwrap(),
            ),
            (observer.key("app"), "/profiles/app", br#""app""#.to_vec()),
        ] {
            let view = view(&observed.unwrap());
            assert_eq!(view["kind"], "configuration-value", "{view}");
            assert_eq!(view["value"], json!({ "exact-json-pointer": pointer }));
            assert_eq!(
                view["span"],
                span(&bytes, &serde_json::from_slice(&text).unwrap())
            );
        }
        assert_eq!(observer.key("web").err(), Some(ObserverInvariant::Missing));
    }

    /// A member's key under the declaration is observed at its token, at the
    /// pointer of its member (D15:776, 790).
    #[test]
    fn a_key_under_the_declaration_is_observed_at_its_token() {
        // Both names occur elsewhere in the file too: `server` as a group
        // member, `requestedLocales` in a coverage rule.
        let config = complete_config();
        let (bytes, document, profiles) = materialized(&config);
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(Arc::clone(&bytes));
        let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        for (path, name, pointer) in [
            (
                &["targetProfiles"][..],
                "server",
                "/profiles/app/targetProfiles/server",
            ),
            (
                &[][..],
                "requestedLocales",
                "/profiles/app/requestedLocales",
            ),
        ] {
            let view = view(&observer.member_key(path, name).unwrap());
            assert_eq!(view["kind"], "configuration-value", "{view}");
            assert_eq!(view["value"], json!({ "exact-json-pointer": pointer }));
            let start: usize = view["span"]["start"].as_str().unwrap().parse().unwrap();
            let end: usize = view["span"]["end"].as_str().unwrap().parse().unwrap();
            // The key's token, which a colon follows.
            assert_eq!(
                serde_json::from_slice::<String>(&bytes[start..end]).unwrap(),
                name
            );
            assert_eq!(bytes[end], b':');
        }
        for (path, name) in [
            (&["targetProfiles"][..], "absent"),
            (&["requestedLocales"][..], "en"),
            (&["absent"][..], "server"),
        ] {
            assert_eq!(
                observer.member_key(path, name).err(),
                Some(ObserverInvariant::Missing)
            );
        }
    }

    /// The selector is projected under the authority it is given, beside the
    /// declarations exactly when they are available (D15:812, 816).
    #[test]
    fn the_selector_is_projected_under_its_authority_beside_the_declarations() {
        let (bytes, document, profiles) = materialized(&minimal_config());
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(bytes);
        let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        let input = SelectorInput::string("storefront", Bound::new(20).unwrap());
        for (available, related) in [(true, 1), (false, 0)] {
            let observation = observer
                .selector(&input, Bound::new(4).unwrap(), available)
                .unwrap();
            let primary = view(&observation.primary);
            assert_eq!(primary["kind"], "profile-selector");
            assert_eq!(
                primary["selector"]["redacted-value"]["length"],
                "over-limit"
            );
            assert_eq!(observation.related.len(), related);
            if available {
                assert_eq!(
                    view(&observation.related[0])["value"],
                    json!({ "exact-json-pointer": "/profiles" })
                );
            }
        }
    }

    /// Every string value and object key of `value` longer than `limit`
    /// bytes, as the pointer of its value or member and the string.
    fn strings_of(value: &Value, pointer: &str, limit: usize, found: &mut Vec<(String, String)>) {
        match value {
            Value::String(text) if text.len() > limit => {
                found.push((pointer.to_owned(), text.clone()));
            }
            Value::Array(elements) => {
                for (index, element) in elements.iter().enumerate() {
                    strings_of(element, &format!("{pointer}/{index}"), limit, found);
                }
            }
            Value::Object(members) => {
                for (name, member) in members {
                    let at = format!("{pointer}/{}", name.replace('~', "~0").replace('/', "~1"));
                    if name.len() > limit {
                        found.push((at.clone(), name.clone()));
                    }
                    strings_of(member, &at, limit, found);
                }
            }
            _ => {}
        }
    }

    /// Every string value and object key past a bound is observed at its
    /// token, a key at its member, and nothing within the bound is: each
    /// once, at a span holding the string, and at its exact pointer unless a
    /// member on its path is redacted (D15:776, 790, 1884, 2203).
    #[test]
    fn every_string_past_a_bound_is_observed_at_its_token() {
        // The complete configuration has arrays of several strings and of
        // objects, each element under its own index.
        for (config, limit) in [minimal_config(), complete_config()]
            .into_iter()
            .flat_map(|config| [0, 8, 16, 1_000].map(|limit| (config.clone(), limit)))
        {
            let (bytes, document, profiles) = materialized(&config);
            let fixture = FileFixture::new(&fixture_limits(10_000));
            let file = fixture.file(Arc::clone(&bytes));
            let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
            let mut expected = Vec::new();
            strings_of(&config, "", limit, &mut expected);
            let mut texts = Vec::new();
            for (length, evidence) in observer
                .strings_past(u64::try_from(limit).unwrap())
                .unwrap()
            {
                let view = view(&evidence);
                let span = &view["span"];
                let start: usize = span["start"].as_str().unwrap().parse().unwrap();
                let end: usize = span["end"].as_str().unwrap().parse().unwrap();
                // The span is the string's token in the file.
                let text: String = serde_json::from_slice(&bytes[start..end]).unwrap();
                assert_eq!(length, u64::try_from(text.len()).unwrap());
                if let Some(pointer) = view["value"]["exact-json-pointer"].as_str() {
                    assert!(
                        expected.contains(&(pointer.to_owned(), text.clone())),
                        "{pointer} {text}"
                    );
                }
                texts.push(text);
            }
            let mut wanted: Vec<String> = expected.into_iter().map(|(_, text)| text).collect();
            texts.sort();
            wanted.sort();
            assert_eq!(texts, wanted, "{limit}");
        }
    }

    /// A declaration or a value the document lacks is a resolver bug.
    #[test]
    fn a_value_the_document_lacks_is_a_resolver_bug() {
        let (bytes, document, profiles) = materialized(&minimal_config());
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(bytes);
        assert_eq!(
            DeclarationObserver::new(&file, (&document, profiles), "web").err(),
            Some(ObserverInvariant::Missing)
        );
        let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        assert_eq!(
            observer.value(&["policies", "absent"]).err(),
            Some(ObserverInvariant::Missing)
        );
        assert_eq!(
            observer.value(&["projectId", "deeper"]).err(),
            Some(ObserverInvariant::Missing)
        );
    }
}
