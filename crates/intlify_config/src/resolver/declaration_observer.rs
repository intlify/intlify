// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Where a check of the selected declaration looks: a value under the
//! declaration, as Materialized Value Evidence at the value's token in the
//! file Phase 1 read (D15:790, 802). The phases after profile selection
//! observe their Findings through it.

use crate::materialize::{MaterializedDocument, NodeId, NodeKind, ObjectMember};
use crate::structural::Edge;

use super::entry::EntryFile;
use super::evidence::{EvidenceInvariant, EvidenceProjection};

/// The declaration profile selection selected, in the file Phase 1 read and
/// the document it materialized.
pub(crate) struct DeclarationObserver<'a> {
    file: &'a EntryFile<'a>,
    document: &'a MaterializedDocument,
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
        self.file
            .value_evidence(&edges, self.document.node(node).span())
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
    use crate::materialize::{materialize_file, MaterializedDocument, NodeId};
    use crate::materialize_tests::limits;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evidence::EvidenceProjection;
    use crate::resolver::evidence_token::EvidenceTokens;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::occurrence::redacted;
    use crate::resolver::token::{Sha256, Specification};

    use super::{member, DeclarationObserver, ObserverInvariant};

    const PROFILE: Specification<'static> = Specification {
        identity: "intlify-project-profile",
        revision: "0",
    };

    /// The view of `projection`, which names nothing redacted.
    fn view(projection: &EvidenceProjection) -> Value {
        let tokens = EvidenceTokens::assign(PROFILE, redacted(&[]), &Sha256);
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
