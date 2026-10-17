// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 6: the selected declaration's `projectId` and `selectionScope`
//! as opaque semantic identities. Not their syntax or byte bounds, which
//! structural admission and `configurationInput.maxSingleStringBytes` already
//! enforce, and not the Profile ID, which selects a declaration but never
//! enters its identity.

use crate::model::{ProjectId, SelectionScope};

use super::evaluation::{
    both, Admitted, Blocking, Causes, Check, Evaluation, EvaluationInvariant, Phase, Subject, Unit,
};
use super::limits::ConfirmedPolicy;

/// The selected declaration's fields Phase 6 admits, in their name order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum DeclarationField {
    ProjectId,
    SelectionScope,
}

impl DeclarationField {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::ProjectId => "projectId",
            Self::SelectionScope => "selectionScope",
        }
    }
}

/// The unit Phase 6 records for one field.
pub(crate) const fn unit(field: DeclarationField) -> Unit {
    Unit {
        phase: Phase::ProjectIdentityResolution,
        subject: Subject::DeclarationField(field),
        check: Check::OpaqueIdentity,
    }
}

/// The checked project identity. Equal identities are equal whichever Profile
/// IDs declared them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProjectIdentity {
    project_id: ProjectId,
    selection_scope: SelectionScope,
}

impl ProjectIdentity {
    pub(crate) const fn project_id(&self) -> &ProjectId {
        &self.project_id
    }

    pub(crate) const fn selection_scope(&self) -> &SelectionScope {
        &self.selection_scope
    }
}

/// Phase 6 over the confirmed selection: each field the selected declaration
/// admitted becomes part of the identity. A field structural admission did not
/// admit arrives as the causes the caller found, and leaves only its own unit
/// not evaluated.
pub(crate) fn resolve_project_identity(
    evaluation: &mut Evaluation,
    policy: Result<&ConfirmedPolicy, &Causes>,
    project_id: Admitted<ProjectId>,
    selection_scope: Admitted<SelectionScope>,
) -> Result<Admitted<ProjectIdentity>, EvaluationInvariant> {
    let confirmed = || policy.map(|_| ()).map_err(Causes::clone);
    let project_id = evaluation.check(
        unit(DeclarationField::ProjectId),
        both(confirmed(), project_id),
        |((), id)| Ok::<_, Blocking>(id),
    )?;
    let selection_scope = evaluation.check(
        unit(DeclarationField::SelectionScope),
        both(confirmed(), selection_scope),
        |((), scope)| Ok::<_, Blocking>(scope),
    )?;
    Ok(
        both(project_id, selection_scope).map(|(project_id, selection_scope)| ProjectIdentity {
            project_id,
            selection_scope,
        }),
    )
}

#[cfg(test)]
mod tests {
    use serde::de::DeserializeOwned;
    use serde_json::{json, Value};

    use super::*;
    use crate::fixtures::minimal_config;
    use crate::input_limits::Bound;
    use crate::locale::{Supported, VersionedIdentity};
    use crate::resolver::evaluation::Status;
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::specification::{
        admit_project_profile, SpecificationInput, SupportedSpecifications,
    };
    use crate::structural::admission_tests::{analyze_fixture, FixtureAnalysis};
    use crate::structural::selection::{Selection, SelectorInput};

    fn policy() -> ConfirmedPolicy {
        ConfirmedPolicy::fixture_owned(fixture_limits(100))
    }

    /// Two declarations, `web` and `admin`, each a copy of the minimal one with
    /// the given project identity.
    fn two_profiles(web: (&str, &str), admin: (&str, &str)) -> Value {
        let mut config = minimal_config();
        let app = config["profiles"]["app"].clone();
        let profiles = config["profiles"].as_object_mut().unwrap();
        profiles.clear();
        for (id, (project_id, selection_scope)) in [("web", web), ("admin", admin)] {
            let mut declaration = app.clone();
            declaration["projectId"] = json!(project_id);
            declaration["selectionScope"] = json!(selection_scope);
            profiles.insert(id.into(), declaration);
        }
        config
    }

    /// The causes a caller would pass for a field structural admission left
    /// unavailable. The structural keys arrive with that phase's checks, so a
    /// Phase 0 key stands in for them.
    fn stand_in() -> Causes {
        let input = SpecificationInput::<&str> {
            project_profile: None,
            locale_canonicalization: None,
            locale_negotiation: None,
            message_locale_fallback: None,
            coverage: None,
            intent_surface_class_vocabulary: None,
            delivery_graph: None,
            delivery_placement: None,
        };
        let supported = SupportedSpecifications::revision_zero(
            VersionedIdentity {
                identity: "test-only-profile-specification",
                revision: "0",
            },
            Supported::new(Vec::new()).unwrap(),
        );
        admit_project_profile(&mut Evaluation::default(), &supported, &input)
            .unwrap()
            .unwrap_err()
    }

    /// One field of a declaration as structural admission left it.
    fn field<T: DeserializeOwned>(analysis: &FixtureAnalysis, id: &str, name: &str) -> Admitted<T> {
        analysis
            .profile_field(id, &[name])
            .unwrap()
            .ok_or_else(stand_in)
    }

    /// Selects `id` and resolves its identity from the fields structural
    /// admission admitted, as the caller would after Phase 4.
    fn resolve(config: &Value, id: &str) -> (Evaluation, Admitted<ProjectIdentity>) {
        let analysis = analyze_fixture(config);
        let Selection::Selected(selection) = analysis
            .select(&SelectorInput::string(id, Bound::new(256).unwrap()))
            .unwrap()
        else {
            panic!("{id} is not selected");
        };
        let selected = selection.id().as_str();
        let mut evaluation = Evaluation::default();
        let identity = resolve_project_identity(
            &mut evaluation,
            Ok(&policy()),
            field(&analysis, selected, "projectId"),
            field(&analysis, selected, "selectionScope"),
        )
        .unwrap();
        (evaluation, identity)
    }

    #[test]
    fn fields_are_named_and_ordered_as_the_declaration_spells_them() {
        let fields = [
            DeclarationField::ProjectId,
            DeclarationField::SelectionScope,
        ];
        assert_eq!(
            fields.map(DeclarationField::name),
            ["projectId", "selectionScope"]
        );
        assert!(fields[0] < fields[1]);
        assert!(unit(fields[0]) < unit(fields[1]));
    }

    #[test]
    fn declarations_with_different_profile_ids_and_the_same_fields_have_one_identity() {
        let config = two_profiles(
            ("storefront", "storefront.production"),
            ("storefront", "storefront.production"),
        );
        let (web_evaluation, web) = resolve(&config, "web");
        let (_, admin) = resolve(&config, "admin");
        assert_eq!(web, admin);
        let web = web.unwrap();
        assert_eq!(web.project_id().as_str(), "storefront");
        assert_eq!(web.selection_scope().as_str(), "storefront.production");
        assert_eq!(web_evaluation.issues().count(), 0);
        assert!(web_evaluation
            .statuses()
            .all(|(_, status)| status == &Status::Evaluated));
    }

    #[test]
    fn each_selection_keeps_its_own_declarations_identity() {
        let config = two_profiles(
            ("storefront", "storefront.production"),
            ("back-office", "back-office.staging"),
        );
        for (id, project_id, selection_scope) in [
            ("web", "storefront", "storefront.production"),
            ("admin", "back-office", "back-office.staging"),
        ] {
            let identity = resolve(&config, id).1.unwrap();
            assert_eq!(
                (
                    identity.project_id().as_str(),
                    identity.selection_scope().as_str()
                ),
                (project_id, selection_scope),
                "{id}"
            );
        }
    }

    #[test]
    fn a_field_structural_admission_rejected_leaves_only_its_own_unit_not_evaluated() {
        // `Store Front` breaks the identity syntax in `admin` alone.
        let config = two_profiles(
            ("storefront", "storefront.production"),
            ("Store Front", "storefront.production"),
        );
        let (evaluation, identity) = resolve(&config, "admin");
        assert_eq!(identity, Err(stand_in()));
        assert_eq!(
            evaluation.status(&unit(DeclarationField::ProjectId)),
            Some(&Status::NotEvaluated(stand_in()))
        );
        assert_eq!(
            evaluation.status(&unit(DeclarationField::SelectionScope)),
            Some(&Status::Evaluated)
        );
        assert_eq!(evaluation.issues().count(), 0);
        // The other declaration's fields were never part of this one.
        assert!(resolve(&config, "web").1.is_ok());
    }

    #[test]
    fn an_unconfirmed_policy_leaves_both_fields_not_evaluated() {
        let analysis = analyze_fixture(&minimal_config());
        let mut evaluation = Evaluation::default();
        let causes = stand_in();
        let identity = resolve_project_identity(
            &mut evaluation,
            Err(&causes),
            field(&analysis, "app", "projectId"),
            field(&analysis, "app", "selectionScope"),
        )
        .unwrap();
        assert_eq!(identity, Err(causes.clone()));
        for field in [
            DeclarationField::ProjectId,
            DeclarationField::SelectionScope,
        ] {
            assert_eq!(
                evaluation.status(&unit(field)),
                Some(&Status::NotEvaluated(causes.clone()))
            );
        }
    }
}
