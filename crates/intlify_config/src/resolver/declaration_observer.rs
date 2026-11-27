// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where a check of the selected declaration looks: a value under the
//! declaration, an array's element there, or a member's key there, as
//! Materialized Value Evidence at the token in the configuration entry Phase 1
//! read (D15:791, 803), and the configuration around it, the root, the
//! declarations, a declared Profile ID's key, every string past a bound, and
//! the selector. The phases after profile selection observe their Findings
//! through it.

use crate::input_limits::Bound;
use crate::materialize::{Locus, MaterializedDocument, NodeId, NodeKind, ObjectMember};
use crate::structural::selection::SelectorInput;
use crate::structural::Edge;

use super::entry::ConfigurationEntry;
use super::evaluation::{Causes, Observation};
use super::evidence::{EvidenceInvariant, EvidenceProjection};
use super::evidence_path::LocaleKeys;
use super::limits::ConfirmedPolicy;

/// The declaration profile selection selected, in the configuration entry
/// Phase 1 read and the document it materialized, and what
/// `specification-admission` bound to classify a locale key in a record
/// constructed since, when it has (D15:777).
#[derive(Clone, Copy)]
pub(crate) struct DeclarationObserver<'a> {
    entry: &'a dyn ConfigurationEntry,
    document: &'a MaterializedDocument,
    profiles: NodeId,
    id: &'a str,
    declaration: NodeId,
    locales: Option<&'a dyn LocaleKeys>,
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
    /// `document`, which `entry` holds.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a declaration the document lacks.
    pub(crate) fn new(
        entry: &'a dyn ConfigurationEntry,
        (document, profiles): (&'a MaterializedDocument, NodeId),
        id: &'a str,
    ) -> Result<Self, ObserverInvariant> {
        let declaration = member(document, profiles, id).ok_or(ObserverInvariant::Missing)?;
        Ok(Self {
            entry,
            document,
            profiles,
            id,
            declaration,
            locales: None,
        })
    }

    /// The observer of the records constructed once `specification-admission`
    /// bound `locales`, which then classify a key of `localeNegotiation.aliases`
    /// or `messageFallback` by whether it returns a canonical identifier
    /// (D15:777).
    pub(crate) fn with_locales<'b>(&self, locales: &'b dyn LocaleKeys) -> DeclarationObserver<'b>
    where
        'a: 'b,
    {
        DeclarationObserver {
            entry: self.entry,
            document: self.document,
            profiles: self.profiles,
            id: self.id,
            declaration: self.declaration,
            locales: Some(locales),
        }
    }

    /// The value at the member path `path` under the declaration, at its
    /// token (D15:791).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a value the document lacks, or that has no
    /// Evidence.
    pub(crate) fn value(&self, path: &[&str]) -> Result<EvidenceProjection, ObserverInvariant> {
        let node = self.node(path)?;
        self.at(&self.edges(path, None), self.document.node(node).locus())
    }

    /// The element at `index` of the array at the member path `path` under
    /// the declaration, at its token (D15:791).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of an element the document lacks, or that has
    /// no Evidence.
    pub(crate) fn element(
        &self,
        path: &[&str],
        index: usize,
    ) -> Result<EvidenceProjection, ObserverInvariant> {
        let NodeKind::Array(elements) = self.document.node(self.node(path)?).kind() else {
            return Err(ObserverInvariant::Missing);
        };
        let element = *elements.get(index).ok_or(ObserverInvariant::Missing)?;
        let position = u64::try_from(index).map_err(|_| ObserverInvariant::Missing)?;
        let mut edges = self.edges(path, None);
        edges.push(Edge::Index(position));
        self.at(&edges, self.document.node(element).locus())
    }

    /// The value at the internal path `steps` under the declaration, through
    /// members and array elements alike, at its token (D15:791).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a value the document lacks, or that has no
    /// Evidence.
    pub(crate) fn nested(
        &self,
        steps: &[Edge<'_>],
    ) -> Result<EvidenceProjection, ObserverInvariant> {
        let mut node = self.declaration;
        for step in steps {
            node = match (step, self.document.node(node).kind()) {
                (Edge::Member(name), NodeKind::Object(_)) => member(self.document, node, name),
                (Edge::Index(index), NodeKind::Array(elements)) => usize::try_from(*index)
                    .ok()
                    .and_then(|index| elements.get(index).copied()),
                _ => None,
            }
            .ok_or(ObserverInvariant::Missing)?;
        }
        let edges: Vec<Edge<'_>> = [Edge::Member("profiles"), Edge::Member(self.id)]
            .into_iter()
            .chain(steps.iter().copied())
            .collect();
        self.at(&edges, self.document.node(node).locus())
    }

    /// The key of the member `name` of the object at the member path `path`
    /// under the declaration, at the key's token, which is classified by facts
    /// fixed when the record is constructed, never by a later check (D15:777,
    /// 791).
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
        self.at(&self.edges(path, Some(name)), member.key())
    }

    /// The key of the member at `rank`, in the unsigned byte order of member
    /// names, of the object at the member path `path` under the declaration,
    /// at the key's token (D15:777, 791).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a member the document lacks, or that has
    /// no Evidence.
    pub(crate) fn ranked_key(
        &self,
        path: &[&str],
        rank: usize,
    ) -> Result<EvidenceProjection, ObserverInvariant> {
        let (name, member) = self.ranked(path, rank)?;
        self.at(&self.edges(path, Some(name)), member.key())
    }

    /// The value of that member, at its token (D15:791).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a member the document lacks, or that has
    /// no Evidence.
    pub(crate) fn ranked_value(
        &self,
        path: &[&str],
        rank: usize,
    ) -> Result<EvidenceProjection, ObserverInvariant> {
        let (name, member) = self.ranked(path, rank)?;
        self.at(
            &self.edges(path, Some(name)),
            self.document.node(member.value()).locus(),
        )
    }

    /// The member at `rank` of the object at `path`, and its name.
    fn ranked(
        &self,
        path: &[&str],
        rank: usize,
    ) -> Result<(&'a str, &'a ObjectMember), ObserverInvariant> {
        let document: &'a MaterializedDocument = self.document;
        let NodeKind::Object(members) = document.node(self.node(path)?).kind() else {
            return Err(ObserverInvariant::Missing);
        };
        members
            .iter()
            .nth(rank)
            .map(|(name, member)| (name.as_str(), member))
            .ok_or(ObserverInvariant::Missing)
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

    /// The root value, at its token (D15:791).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a root that has no Evidence.
    pub(crate) fn root(&self) -> Result<EvidenceProjection, ObserverInvariant> {
        self.at(&[], self.document.node(self.document.root()).locus())
    }

    /// The root's `profiles` object, the declarations as one location, at its
    /// token (D15:791, 813).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of declarations that have no Evidence.
    pub(crate) fn declarations(&self) -> Result<EvidenceProjection, ObserverInvariant> {
        self.at(
            &[Edge::Member("profiles")],
            self.document.node(self.profiles).locus(),
        )
    }

    /// The key of the declared Profile ID `id` in the root's `profiles`
    /// object, at the key's token, which is classified by facts fixed when the
    /// record is constructed, never by a later check (D15:777, 791).
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
        self.at(&[Edge::Member("profiles"), Edge::Member(id)], member.key())
    }

    /// Every string value and object key in the configuration longer than
    /// `limit` UTF-8 bytes, with its length, at its token: a key at its
    /// member, at the key's span (D15:791, 1885, 2204).
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
                    found.push((bytes, self.at(path, located.locus())?));
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
                        found.push((bytes, self.at(path, member.key())?));
                    }
                    self.walk(member.value(), path, limit, found)?;
                    path.pop();
                }
            }
            NodeKind::Null | NodeKind::Boolean(_) | NodeKind::Number(_) => {}
        }
        Ok(())
    }

    /// The selector `input`, under the Safe Origin Kind the entry fixes for
    /// it (D15:805), projected under its final authority `authority`, beside
    /// the declarations when they are `available` (D15:813, 817; plan 025
    /// interpretation #215). A selector past the bound names no declaration
    /// (D15:808).
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
        let origin = self.entry.selector_origin();
        let primary = EvidenceProjection::selector(origin, input, authority, std::iter::empty())
            .map_err(ObserverInvariant::Evidence)?;
        let related = if available {
            vec![self.declarations()?]
        } else {
            Vec::new()
        };
        Ok(Observation { primary, related })
    }

    /// The value at the internal path `edges` from the root, at `locus`.
    fn at(
        &self,
        edges: &[Edge<'_>],
        locus: Locus,
    ) -> Result<EvidenceProjection, ObserverInvariant> {
        self.entry
            .value_evidence_after(edges, locus, self.locales)
            .map_err(ObserverInvariant::Evidence)
    }
}

/// What a bound's unit observes past the confirmed `policy`'s limit: where
/// `at` looks in the declaration, beside the policy, which supplies the bound
/// (D15:2250). Nothing is observed without the policy or the declaration's
/// entry.
///
/// # Errors
///
/// Returns the resolver bug of a location the document lacks.
pub(crate) fn beside<'o>(
    policy: Result<&ConfirmedPolicy, &Causes>,
    observer: Option<&DeclarationObserver<'o>>,
    at: impl FnOnce(&DeclarationObserver<'o>) -> Result<EvidenceProjection, ObserverInvariant>,
) -> Result<Option<Observation>, ObserverInvariant> {
    match (policy, observer) {
        (Ok(policy), Some(observer)) => Ok(Some(Observation {
            primary: at(observer)?,
            related: vec![policy.evidence().clone()],
        })),
        _ => Ok(None),
    }
}

/// One observation for each occurrence at `at`, beside `related`, where both
/// were observed (D15:2388).
pub(crate) fn each_beside(
    at: &[Option<EvidenceProjection>],
    related: Option<&EvidenceProjection>,
) -> Vec<Option<Observation>> {
    each_beside_all(at, related.map(std::slice::from_ref))
}

/// One observation for each occurrence at `at`, beside every location in
/// `related`, where all were observed (D15:2388). A check that compares no
/// other location relates none.
pub(crate) fn each_beside_all(
    at: &[Option<EvidenceProjection>],
    related: Option<&[EvidenceProjection]>,
) -> Vec<Option<Observation>> {
    at.iter()
        .map(|primary| {
            Some(Observation {
                primary: primary.clone()?,
                related: related?.to_vec(),
            })
        })
        .collect()
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
    use crate::resolver::entry::fixtures::{FileFixture, Relabeled};
    use crate::resolver::limits::fixture_limits;
    use crate::structural::selection::SelectorInput;

    use super::fixtures::{materialized, view};
    use super::{ConfigurationEntry, DeclarationObserver, EvidenceProjection, ObserverInvariant};

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
    /// pointer of its path from the root (D15:791).
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
    /// (D15:777, 791).
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
    /// pointer of its member (D15:777, 791).
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

    /// A key of `localeNegotiation.aliases` or `messageFallback`, at its rank
    /// among the map's keys, is non-secret in a record constructed once
    /// `specification-admission` bound a canonicalizer that returns a canonical
    /// identifier for it, and redacted otherwise, and before (D15:777, 3516).
    #[test]
    fn a_locale_key_is_classified_by_the_bound_canonicalizer() {
        let mut config = complete_config();
        config["profiles"]["app"]["messageFallback"]["zz"] = json!(["en"]);
        config["profiles"]["app"]["localeNegotiation"]["aliases"]["zz"] = json!("en");
        let (bytes, document, profiles) = materialized(&config);
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(Arc::clone(&bytes));
        let before = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        let canonicalizer = crate::resolver::locales::fixtures::canonicalizer(64);
        let after = before.with_locales(&canonicalizer);
        let pointer = |projection: EvidenceProjection| view(&projection)["value"].clone();
        let exact = |at: &str| json!({ "exact-json-pointer": at });
        // `ja` and then `zz`, in byte order.
        assert_eq!(
            pointer(after.ranked_key(&["messageFallback"], 0).unwrap()),
            exact("/profiles/app/messageFallback/ja")
        );
        let aliases = ["localeNegotiation", "aliases"];
        assert_eq!(
            pointer(after.ranked_key(&aliases, 0).unwrap()),
            exact("/profiles/app/localeNegotiation/aliases/fr")
        );
        for unsafe_key in [
            before.ranked_key(&["messageFallback"], 0).unwrap(),
            after.ranked_key(&["messageFallback"], 1).unwrap(),
            before.ranked_key(&aliases, 0).unwrap(),
            after.ranked_key(&aliases, 1).unwrap(),
        ] {
            assert!(pointer(unsafe_key).get("exact-json-pointer").is_none());
        }
        // A destination sits under its key, and its value's token is its own.
        let destination = after
            .ranked_value(&["localeNegotiation", "aliases"], 0)
            .unwrap();
        let destination = view(&destination);
        assert_eq!(
            destination["value"],
            exact("/profiles/app/localeNegotiation/aliases/fr")
        );
        let start: usize = destination["span"]["start"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        let end: usize = destination["span"]["end"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            serde_json::from_slice::<String>(&bytes[start..end]).unwrap(),
            "fr-FR"
        );
        assert_eq!(
            after.ranked_key(&["messageFallback"], 2).err(),
            Some(ObserverInvariant::Missing)
        );
    }

    /// An array's element under the declaration is observed at its token, at
    /// the pointer of its index (D15:791).
    #[test]
    fn an_element_under_the_declaration_is_observed_at_its_token() {
        let mut config = complete_config();
        config["profiles"]["app"]["requestedLocales"] = json!(["ja", "en"]);
        let (bytes, document, profiles) = materialized(&config);
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(Arc::clone(&bytes));
        let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        for (path, index, pointer) in [
            (
                &["requestedLocales"][..],
                1,
                "/profiles/app/requestedLocales/1",
            ),
            (
                &["targetProfiles", "server", "requestedLocales"][..],
                0,
                "/profiles/app/targetProfiles/server/requestedLocales/0",
            ),
        ] {
            let view = view(&observer.element(path, index).unwrap());
            assert_eq!(view["kind"], "configuration-value", "{view}");
            assert_eq!(view["value"], json!({ "exact-json-pointer": pointer }));
            let start: usize = view["span"]["start"].as_str().unwrap().parse().unwrap();
            let end: usize = view["span"]["end"].as_str().unwrap().parse().unwrap();
            assert_eq!(
                serde_json::from_slice::<String>(&bytes[start..end]).unwrap(),
                "en"
            );
        }
        for (path, index) in [
            (&["requestedLocales"][..], 2),
            (&["projectId"][..], 0),
            (&["absent"][..], 0),
        ] {
            assert_eq!(
                observer.element(path, index).err(),
                Some(ObserverInvariant::Missing)
            );
        }
    }

    /// A value through members and elements alike, such as a coverage rule's
    /// selector item, is observed at its token, and the same path through
    /// members alone is the value's (D15:791).
    #[test]
    fn a_value_through_members_and_elements_is_observed_at_its_token() {
        use crate::structural::Edge::{Index, Member};
        let mut config = complete_config();
        config["profiles"]["app"]["coverage"] = json!({"rules": [
            {"requestedLocales": ["ja"], "mode": "fallback-allowed"},
            {"requestedLocales": ["fr", "en"], "mode": "fallback-allowed"}
        ]});
        let (bytes, document, profiles) = materialized(&config);
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(Arc::clone(&bytes));
        let observer = DeclarationObserver::new(&file, (&document, profiles), "app").unwrap();
        let item = [
            Member("coverage"),
            Member("rules"),
            Index(1),
            Member("requestedLocales"),
            Index(1),
        ];
        let view = view(&observer.nested(&item).unwrap());
        assert_eq!(
            view["value"],
            json!({ "exact-json-pointer": "/profiles/app/coverage/rules/1/requestedLocales/1" })
        );
        let start: usize = view["span"]["start"].as_str().unwrap().parse().unwrap();
        let end: usize = view["span"]["end"].as_str().unwrap().parse().unwrap();
        assert_eq!(
            serde_json::from_slice::<String>(&bytes[start..end]).unwrap(),
            "en"
        );
        assert_eq!(
            observer.nested(&[Member("coverage"), Member("rules")]),
            observer.value(&["coverage", "rules"])
        );
        for missing in [
            &item[..2]
                .iter()
                .copied()
                .chain([Index(2)])
                .collect::<Vec<_>>()[..],
            &[Member("coverage"), Index(0)],
            &[Member("projectId"), Member("deeper")],
        ] {
            assert_eq!(
                observer.nested(missing).err(),
                Some(ObserverInvariant::Missing)
            );
        }
    }

    /// The selector is projected under the authority it is given, beside the
    /// declarations exactly when they are available (D15:813, 817).
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

    /// The selector is observed under the Safe Origin Kind its entry fixes,
    /// beside the declarations in that entry (D15:205, 805, 813).
    #[test]
    fn the_selector_is_observed_under_the_origin_its_entry_fixes() {
        let (bytes, document, profiles) = materialized(&minimal_config());
        let fixture = FileFixture::new(&fixture_limits(10_000));
        let file = fixture.file(Arc::clone(&bytes));
        let relabeled = Relabeled(fixture.file(bytes));
        let bound = Bound::new(20).unwrap();
        let input = SelectorInput::absent(bound);
        for (entry, selector_origin, value_origin) in [
            (
                &file as &dyn ConfigurationEntry,
                "profile-selector-cli",
                "configuration-file",
            ),
            (
                &relabeled,
                "profile-selector-programmatic",
                "configuration-programmatic",
            ),
        ] {
            let observer = DeclarationObserver::new(entry, (&document, profiles), "app").unwrap();
            let observation = observer.selector(&input, bound, true).unwrap();
            assert_eq!(view(&observation.primary)["origin"], selector_origin);
            assert_eq!(view(&observation.related[0])["origin"], value_origin);
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
    /// member on its path is redacted (D15:777, 791, 1885, 2204).
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

    /// Past a bound, where the unit looks is observed beside the confirmed
    /// policy, which supplies the bound; nothing without either.
    #[test]
    fn past_a_bound_the_declaration_is_observed_beside_the_policy() {
        use crate::resolver::evaluation::Observation;
        use crate::resolver::limits::ConfirmedPolicy;
        use crate::resolver::specification::Slot;

        let observed = super::fixtures::Observed::new(&minimal_config());
        let policy = ConfirmedPolicy::fixture_owned(fixture_limits(10_000));
        let unconfirmed = crate::resolver::locales::fixtures::stand_in(Slot::Coverage);
        observed.app(|observer| {
            let requested =
                |observer: &DeclarationObserver<'_>| observer.value(&["requestedLocales"]);
            assert_eq!(
                super::beside(Ok(&policy), Some(observer), requested),
                Ok(Some(Observation {
                    primary: observer.value(&["requestedLocales"]).unwrap(),
                    related: vec![policy.evidence().clone()],
                }))
            );
            assert_eq!(
                super::beside(Err(&unconfirmed), Some(observer), requested),
                Ok(None)
            );
            assert_eq!(super::beside(Ok(&policy), None, requested), Ok(None));
            assert_eq!(
                super::beside(Ok(&policy), Some(observer), |observer| observer
                    .value(&["absent"])),
                Err(ObserverInvariant::Missing)
            );
        });
    }
}
