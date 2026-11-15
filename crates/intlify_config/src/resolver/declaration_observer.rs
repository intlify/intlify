// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where a check of the selected declaration looks: a value under the
//! declaration, as Materialized Value Evidence at the value's token in the
//! file Phase 1 read (D15:790, 802), and the configuration around it, the
//! root, the declarations, a declared Profile ID's key, and the selector.
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
        let mut node = self.declaration;
        for name in path {
            node = member(self.document, node, name).ok_or(ObserverInvariant::Missing)?;
        }
        let edges: Vec<Edge<'_>> = [Edge::Member("profiles"), Edge::Member(self.id)]
            .into_iter()
            .chain(path.iter().map(|name| Edge::Member(name)))
            .collect();
        self.at(&edges, self.document.node(node).span())
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

/// The value of the member `name` of the object `node`.
fn member(document: &MaterializedDocument, node: NodeId, name: &str) -> Option<NodeId> {
    match document.node(node).kind() {
        NodeKind::Object(members) => members.get(name).map(ObjectMember::value),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::fixtures::minimal_config;
    use crate::input_limits::Bound;
    use crate::materialize::{materialize_file, MaterializedDocument, NodeId};
    use crate::materialize_tests::limits;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evidence::EvidenceProjection;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::token::{Sha256, Specification};
    use crate::structural::selection::SelectorInput;

    use super::{member, DeclarationObserver, ObserverInvariant};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// The view of `projection`, under the tokens of its own redacted
    /// components.
    fn view(projection: &EvidenceProjection) -> Value {
        let tokens = EvidenceTokens::assign(PROFILE, projection.redacted(), &Sha256);
        projection.retained(PROFILE, &tokens).unwrap().view()
    }

    /// `config` as a file holds it, and the document it materializes with
    /// the root's `profiles` object.
    fn materialized(config: &Value) -> (Arc<[u8]>, MaterializedDocument, NodeId) {
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(config).unwrap());
        let document = materialize_file(Arc::clone(&bytes), limits()).unwrap();
        let profiles = member(&document, document.root(), "profiles").unwrap();
        (bytes, document, profiles)
    }

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
