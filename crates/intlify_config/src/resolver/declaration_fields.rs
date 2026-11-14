// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The selected declaration's fields as the semantic phases read them, each
//! on its own from what structural admission admitted: the field's value,
//! absent where an optional field is, or the causes of its absence, the
//! Findings of the field and of what lies below it. A field structural
//! admission did not admit suppresses only the work that reads it, while its
//! independently admitted siblings continue (D15:393, 2367, 3340); an
//! undeclared member of the declaration belongs to no field.
//!
//! The checks decide: a field they admitted and the analysis did not, or a
//! required one they admitted though it is absent, is a resolver bug.

use std::collections::BTreeMap;

use serde::de::DeserializeOwned;

use crate::model::{
    Coverage, Delivery, DeploymentGroup, DeploymentGroupId, FallbackCandidate, LocaleNegotiation,
    NonEmptyMap, NonEmptyVec, Policies, ProjectId, SelectionScope, TargetDeclaration, TargetId,
};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::{Member, StructuralAnalysis};

use super::evaluation::{Admitted, Causes, Evaluation};
use super::locales::LocaleFields;
use super::resource_policy::DeclarationReferences;
use super::structural_admission::{field, StructuralInvariant};

type TargetProfiles<Target> = NonEmptyMap<TargetId, TargetDeclaration<Target>>;
type DeploymentGroups = NonEmptyMap<DeploymentGroupId, DeploymentGroup>;
type MessageFallback = BTreeMap<String, NonEmptyVec<FallbackCandidate>>;

/// The selected declaration's fields, each admitted, with an optional one
/// possibly absent, or unavailable with the causes behind it.
#[derive(Debug)]
pub(crate) struct DeclarationFields<Policy, Target> {
    project_id: Admitted<ProjectId>,
    selection_scope: Admitted<SelectionScope>,
    requested_locales: Admitted<NonEmptyVec<String>>,
    default_requested_locale: Admitted<String>,
    default_source_locale: Admitted<Option<String>>,
    locale_negotiation: Admitted<Option<LocaleNegotiation>>,
    message_fallback: Admitted<Option<MessageFallback>>,
    coverage: Admitted<Option<Coverage>>,
    policies: Admitted<Policies<Policy>>,
    target_profiles: Admitted<TargetProfiles<Target>>,
    deployment_groups: Admitted<DeploymentGroups>,
    delivery: Admitted<Option<Delivery>>,
}

impl<Policy: DeserializeOwned, Target: DeserializeOwned> DeclarationFields<Policy, Target> {
    /// The fields of the declaration `id` of `analysis`, as `evaluation`
    /// recorded structural admission's checks under each.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a field the checks admitted and the
    /// analysis did not, or of a required one they admitted though it is
    /// absent.
    pub(crate) fn read(
        evaluation: &Evaluation,
        analysis: &StructuralAnalysis<Policy, Target>,
        id: &str,
    ) -> Result<Self, StructuralInvariant> {
        let read = Reader {
            evaluation,
            analysis,
            id,
        };
        Ok(Self {
            project_id: read.required("projectId")?,
            selection_scope: read.required("selectionScope")?,
            requested_locales: read.required("requestedLocales")?,
            default_requested_locale: read.required("defaultRequestedLocale")?,
            default_source_locale: read.optional("defaultSourceLocale")?,
            locale_negotiation: read.optional("localeNegotiation")?,
            message_fallback: read.optional("messageFallback")?,
            coverage: read.optional("coverage")?,
            policies: read.required("policies")?,
            target_profiles: read.required("targetProfiles")?,
            deployment_groups: read.required("deploymentGroups")?,
            delivery: read.optional("delivery")?,
        })
    }

    pub(crate) fn project_id(&self) -> Admitted<ProjectId> {
        self.project_id.clone()
    }

    pub(crate) fn selection_scope(&self) -> Admitted<SelectionScope> {
        self.selection_scope.clone()
    }

    /// The locale-bearing fields, which Phases 7 and 8 read.
    pub(crate) fn locale_fields(&self) -> LocaleFields<'_, Target> {
        LocaleFields {
            requested_locales: borrowed(&self.requested_locales, NonEmptyVec::as_slice),
            default_requested_locale: borrowed(&self.default_requested_locale, String::as_str),
            default_source_locale: borrowed(&self.default_source_locale, Option::as_deref),
            locale_negotiation: borrowed(&self.locale_negotiation, Option::as_ref),
            message_fallback: borrowed(&self.message_fallback, Option::as_ref),
            coverage: borrowed(&self.coverage, Option::as_ref),
            target_profiles: borrowed(&self.target_profiles, |profiles| profiles),
        }
    }

    pub(crate) fn target_profiles(&self) -> Admitted<&TargetProfiles<Target>> {
        borrowed(&self.target_profiles, |profiles| profiles)
    }

    pub(crate) fn deployment_groups(&self) -> Admitted<&DeploymentGroups> {
        borrowed(&self.deployment_groups, |groups| groups)
    }

    pub(crate) fn delivery(&self) -> Admitted<Option<&Delivery>> {
        borrowed(&self.delivery, Option::as_ref)
    }
}

impl DeclarationFields<PolicyReference, TargetProfileReference> {
    /// The fields that hold artifact references, which Phases 4 and 9 read.
    pub(crate) fn references(&self) -> DeclarationReferences<'_> {
        DeclarationReferences {
            policies: borrowed(&self.policies, |policies| policies),
            target_profiles: borrowed(&self.target_profiles, |profiles| profiles),
        }
    }
}

/// `field` borrowed through `project`, or its causes.
fn borrowed<'a, T, U>(field: &'a Admitted<T>, project: impl FnOnce(&'a T) -> U) -> Admitted<U> {
    field.as_ref().map(project).map_err(Causes::clone)
}

/// Reads the fields of the declaration `id`.
struct Reader<'r, Policy, Target> {
    evaluation: &'r Evaluation,
    analysis: &'r StructuralAnalysis<Policy, Target>,
    id: &'r str,
}

impl<Policy, Target> Reader<'_, Policy, Target> {
    /// The optional field `name`: its value once the checks under it admitted
    /// it, absent when it is, or the causes.
    fn optional<T: DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Admitted<Option<T>>, StructuralInvariant> {
        let member = self
            .analysis
            .profile_member(self.id, name)
            .map_err(StructuralInvariant::Admission)?;
        match (field(self.evaluation, self.id, name), member) {
            (Err(causes), _) => Ok(Err(causes)),
            (Ok(()), Member::Admitted(value)) => Ok(Ok(Some(value))),
            (Ok(()), Member::Absent) => Ok(Ok(None)),
            // The checks decide, so a field they admitted and the analysis
            // did not is a resolver bug.
            (Ok(()), Member::Unadmitted) => Err(StructuralInvariant::UnreportedFailure),
        }
    }

    /// The required field `name`, whose absence its required check reports.
    fn required<T: DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Admitted<T>, StructuralInvariant> {
        match self.optional(name)? {
            Ok(Some(value)) => Ok(Ok(value)),
            Ok(None) => Err(StructuralInvariant::UnreportedFailure),
            Err(causes) => Ok(Err(causes)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use serde_json::{json, Value};

    use crate::fixtures::{
        complete_config, minimal_config, FixturePolicyReference, FixtureTargetReference,
    };
    use crate::input_limits::Bound;
    use crate::materialize::materialize_file;
    use crate::materialize_tests::limits as input_limits;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{Admitted, Check, Evaluation, IssueKey, Unit};
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::structural_admission::{admit_structure, StructuralInvariant};
    use crate::structural::{AuthoringSchema, StructuralAnalysis, StructuralLimits};

    use super::DeclarationFields;

    type Schema = AuthoringSchema<FixturePolicyReference, FixtureTargetReference>;
    type Analysis = StructuralAnalysis<FixturePolicyReference, FixtureTargetReference>;
    type Fields = DeclarationFields<FixturePolicyReference, FixtureTargetReference>;

    /// Phase 2 over `value`: the evaluation it recorded and its analysis.
    fn structured(value: &Value) -> (Evaluation, Analysis) {
        let mut evaluation = Evaluation::default();
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(value).unwrap());
        let document = materialize_file(Arc::clone(&bytes), input_limits()).unwrap();
        let fixture = FileFixture::new(&fixture_limits(100));
        let limits = StructuralLimits {
            max_profiles: Bound::new(16).unwrap(),
            max_profile_id_bytes: Bound::new(64).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        };
        let analysis = admit_structure(
            &mut evaluation,
            &fixture.file(bytes),
            document,
            &Schema::for_model().unwrap(),
            limits,
            Ok(()),
        )
        .unwrap()
        .unwrap();
        (evaluation, analysis)
    }

    /// The fields of the declaration `app` of `value`.
    fn read(value: &Value) -> Fields {
        let (evaluation, analysis) = structured(value);
        Fields::read(&evaluation, &analysis, "app").unwrap()
    }

    /// The checks behind `causes`.
    fn checks<T>(field: &Admitted<T>) -> BTreeSet<(Vec<String>, Check)> {
        field
            .as_ref()
            .err()
            .unwrap_or_else(|| panic!("the field is admitted"))
            .keys()
            .map(IssueKey::unit)
            .map(|unit: &Unit| (path(unit), unit.check))
            .collect()
    }

    /// The path of a structural unit's field below the declaration, `*` for
    /// an entry's key or field.
    fn path(unit: &Unit) -> Vec<String> {
        use crate::resolver::evaluation::Subject;
        use crate::resolver::structural_admission::StructuralSubject as S;
        match &unit.subject {
            Subject::Structural(S::ProfileField(_, field)) => {
                field.path.iter().map(ToString::to_string).collect()
            }
            Subject::Structural(S::TargetProfileKey(..) | S::TargetField(..)) => {
                vec!["targetProfiles".to_owned(), "*".to_owned()]
            }
            Subject::Structural(S::DeploymentGroupKey(..) | S::GroupField(..)) => {
                vec!["deploymentGroups".to_owned(), "*".to_owned()]
            }
            subject => panic!("{subject:?} is no field of a declaration"),
        }
    }

    /// A declaration structural admission admitted whole has every field,
    /// an optional one absent where the declaration omits it.
    #[test]
    fn a_declaration_admitted_whole_has_every_field() {
        let fields = read(&minimal_config());
        assert!(fields.project_id.is_ok() && fields.selection_scope.is_ok());
        assert_eq!(
            fields.requested_locales.as_ref().unwrap().as_slice(),
            ["en"]
        );
        assert_eq!(fields.default_source_locale, Ok(None));
        assert_eq!(fields.coverage, Ok(None));
        assert_eq!(fields.delivery, Ok(None));
        assert!(fields.policies.is_ok());
        assert!(fields.target_profiles.is_ok() && fields.deployment_groups.is_ok());
        let complete = read(&complete_config());
        assert!(matches!(complete.locale_negotiation, Ok(Some(_))));
        assert!(matches!(complete.message_fallback, Ok(Some(_))));
        assert!(matches!(complete.coverage, Ok(Some(_))));
        // Each accessor hands on the field it reads.
        assert!(matches!(complete.delivery(), Ok(Some(_))));
        assert_eq!(fields.delivery(), Ok(None));
    }

    /// A field structural admission rejected is unavailable with the
    /// Findings under it, and its siblings stay admitted (D15:2367, 3340).
    #[test]
    fn a_rejected_field_holds_back_only_itself() {
        let mut config = minimal_config();
        config["profiles"]["app"]["requestedLocales"] = json!(["en", 7]);
        config["profiles"]["app"]["coverage"] = json!(false);
        config["profiles"]["app"]
            .as_object_mut()
            .unwrap()
            .remove("projectId");
        let fields = read(&config);
        assert_eq!(
            checks(&fields.requested_locales),
            BTreeSet::from([(
                vec!["requestedLocales".to_owned(), "*".to_owned()],
                Check::FieldType
            )])
        );
        assert_eq!(
            checks(&fields.coverage),
            BTreeSet::from([(vec!["coverage".to_owned()], Check::FieldType)])
        );
        assert_eq!(
            checks(&fields.project_id),
            BTreeSet::from([(vec!["projectId".to_owned()], Check::RequiredField)])
        );
        assert!(fields.selection_scope.is_ok());
        assert!(fields.default_requested_locale.is_ok());
        assert!(fields.policies.is_ok());
        assert!(fields.target_profiles.is_ok() && fields.deployment_groups.is_ok());
    }

    /// An undeclared member of the declaration belongs to no field.
    #[test]
    fn an_undeclared_member_holds_back_no_field() {
        let mut config = minimal_config();
        config["profiles"]["app"]["extra"] = json!(true);
        let fields = read(&config);
        assert!(fields.project_id.is_ok() && fields.requested_locales.is_ok());
        assert!(fields.policies.is_ok() && fields.target_profiles.is_ok());
    }

    /// A Target ID entry's failure holds back `targetProfiles` alone, its key
    /// and its fields both under it.
    #[test]
    fn a_target_failure_holds_back_target_profiles_alone() {
        let mut config = minimal_config();
        config["profiles"]["app"]["targetProfiles"]["browser"]["requestedLocales"] = json!("en");
        let fields = read(&config);
        assert_eq!(
            checks(&fields.target_profiles),
            BTreeSet::from([(
                vec!["targetProfiles".to_owned(), "*".to_owned()],
                Check::FieldType
            )])
        );
        assert!(fields.deployment_groups.is_ok() && fields.requested_locales.is_ok());
        let mut keyed = minimal_config();
        let target = keyed["profiles"]["app"]["targetProfiles"]["browser"].clone();
        keyed["profiles"]["app"]["targetProfiles"]["Web App"] = target;
        let fields = read(&keyed);
        assert_eq!(
            checks(&fields.target_profiles),
            BTreeSet::from([(
                vec!["targetProfiles".to_owned(), "*".to_owned()],
                Check::FieldValue
            )])
        );
    }

    /// The checks decide: a field they admitted and the analysis did not,
    /// or a required one they admitted though it is absent, is a resolver
    /// bug.
    #[test]
    fn a_field_the_checks_admit_and_the_analysis_lacks_is_a_resolver_bug() {
        let mut invalid = minimal_config();
        invalid["profiles"]["app"]["projectId"] = json!("Store Front");
        let (_, analysis) = structured(&invalid);
        assert_eq!(
            Fields::read(&Evaluation::default(), &analysis, "app").err(),
            Some(StructuralInvariant::UnreportedFailure)
        );
        let mut missing = minimal_config();
        missing["profiles"]["app"]
            .as_object_mut()
            .unwrap()
            .remove("projectId");
        let (_, analysis) = structured(&missing);
        assert_eq!(
            Fields::read(&Evaluation::default(), &analysis, "app").err(),
            Some(StructuralInvariant::UnreportedFailure)
        );
        // An optional field too: present, its checks admitting it, yet not
        // admitted by the analysis.
        let mut optional = minimal_config();
        optional["profiles"]["app"]["coverage"] = json!(false);
        let (_, analysis) = structured(&optional);
        assert_eq!(
            Fields::read(&Evaluation::default(), &analysis, "app").err(),
            Some(StructuralInvariant::UnreportedFailure)
        );
    }

    /// A Group ID entry's failure holds back `deploymentGroups` alone.
    #[test]
    fn a_group_failure_holds_back_deployment_groups_alone() {
        let mut config = minimal_config();
        config["profiles"]["app"]["deploymentGroups"]["web"]["members"] = json!("browser");
        let fields = read(&config);
        assert_eq!(
            checks(&fields.deployment_groups),
            BTreeSet::from([(
                vec!["deploymentGroups".to_owned(), "*".to_owned()],
                Check::FieldType
            )])
        );
        assert!(fields.target_profiles.is_ok() && fields.policies.is_ok());
    }

    /// Another declaration's failures hold back none of this one's fields.
    #[test]
    fn another_declarations_failure_holds_back_no_field() {
        let mut config = minimal_config();
        let mut web = config["profiles"]["app"].clone();
        web["coverage"] = json!(false);
        config["profiles"]["web"] = web;
        let fields = read(&config);
        assert_eq!(fields.coverage, Ok(None));
        assert!(fields.project_id.is_ok() && fields.requested_locales.is_ok());
    }

    /// A member the declaration's schema does not define is a defect of the
    /// model's layout.
    #[test]
    fn a_member_the_schema_does_not_define_is_a_layout_defect() {
        let (_, analysis) = structured(&minimal_config());
        assert_eq!(
            analysis.profile_member::<Value>("app", "extra").err(),
            Some(crate::structural::AdmissionInvariant::ModelLayout)
        );
    }
}
