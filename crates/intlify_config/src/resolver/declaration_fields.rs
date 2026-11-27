// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The selected declaration as the semantic phases read it, from its
//! fragments ([`DeclarationFragments`]): a single-valued field as one value,
//! and each `policies` slot, map entry, and array element on its own, with
//! its value, absent where an optional one is, or the causes of its absence.
//! What structural admission did not admit suppresses only the work that
//! reads it, while its independently admitted siblings continue (D15:394,
//! 2368, 3476); an undeclared member of the declaration belongs to no field.
//!
//! The checks decide: a field they admitted and the analysis did not, or a
//! required one they admitted though it is absent, is a resolver bug.

use serde::de::DeserializeOwned;

use crate::model::{Delivery, ProjectId, SelectionScope};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::StructuralAnalysis;

use super::coverage::{CoverageFields, RuleField, SelectorField};
use super::declaration_fragments::{
    AliasEntries, CoverageEntries, DeclarationFragments, FallbackEntry, GroupEntries, PolicySlots,
    SelectorEntry, TargetEntries,
};
use super::evaluation::{Admitted, Causes};
use super::groups::{GroupField, GroupFields};
use super::hydration::{HydrationFields, RelationField, RelationGroup};
use super::locales::{
    AliasField, AliasFields, FallbackField, LocaleFields, TargetLocaleFields, TargetMapFields,
};
use super::resource_policy::{
    DeclarationReferences, PolicyReferences, PolicySlot, TargetReferences,
};
use super::structural_admission::StructuralInvariant;
use super::tally::Tally;
use super::value_admissions::ValueAdmissions;

/// The selected declaration's fields, each admitted, with an optional one
/// possibly absent, or unavailable with the causes behind it; a field that
/// holds several values keeps the result of each.
#[derive(Debug)]
pub(crate) struct DeclarationFields<Policy, Target> {
    project_id: Admitted<ProjectId>,
    selection_scope: Admitted<SelectionScope>,
    /// Every locale spelling the declaration holds, which no field's failure
    /// hides: without a selected declaration, why there is none.
    locale_spellings: Admitted<Tally>,
    /// Each element's value or why it has none, or why the array has none.
    requested_locales: Admitted<Vec<Admitted<String>>>,
    default_requested_locale: Admitted<String>,
    default_source_locale: Admitted<Option<String>>,
    /// The alias map entry by entry, which Phases 7 and 8 read.
    negotiation_aliases: Admitted<AliasEntries>,
    /// The fallback map entry by entry, which Phases 7 and 8 read.
    message_fallback: Admitted<Vec<FallbackEntry>>,
    /// The coverage rule by rule, which Phase 8 reads.
    coverage: Admitted<CoverageEntries>,
    /// Each `policies` slot's reference or why it has none, which Phase 9
    /// reads, and whether the object passed apart from its slots, which both
    /// of its semantic groups wait on.
    policies: PolicySlots<Policy>,
    /// The Target ID map entry by entry, which Phases 7, 9, and 11 read.
    target_entries: Admitted<TargetEntries<Target>>,
    /// The Group ID map entry by entry, which Phases 12 and 13 read.
    group_entries: Admitted<GroupEntries>,
    delivery: Admitted<Option<Delivery>>,
}

impl<Policy: DeserializeOwned, Target: DeserializeOwned> DeclarationFields<Policy, Target> {
    /// The fields of the declaration `id` of `analysis`, as `values` holds
    /// what structural admission's candidates left at and below each.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a declaration the analysis lacks, of a
    /// field the checks admitted and the analysis did not, or of a required
    /// one they admitted though it is absent.
    pub(crate) fn read(
        values: &ValueAdmissions,
        analysis: &StructuralAnalysis<Policy, Target>,
        id: &str,
    ) -> Result<Self, StructuralInvariant> {
        let read = DeclarationFragments::new(values, analysis, id)?;
        Ok(Self {
            project_id: read.required("projectId")?,
            selection_scope: read.required("selectionScope")?,
            locale_spellings: Ok(read.locale_spellings()),
            requested_locales: read.string_items("requestedLocales")?,
            default_requested_locale: read.required("defaultRequestedLocale")?,
            default_source_locale: read.optional("defaultSourceLocale")?,
            negotiation_aliases: read.alias_entries()?,
            message_fallback: read.fallback_entries()?,
            coverage: read.coverage_entries()?,
            policies: read.policy_slots()?,
            target_entries: read.target_entries()?,
            group_entries: read.group_entries()?,
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
    pub(crate) fn locale_fields(&self) -> LocaleFields<'_> {
        LocaleFields {
            spellings: self.locale_spellings.clone(),
            requested_locales: strings(&self.requested_locales),
            default_requested_locale: borrowed(&self.default_requested_locale, String::as_str),
            default_source_locale: borrowed(&self.default_source_locale, Option::as_deref),
            negotiation_aliases: borrowed(&self.negotiation_aliases, |aliases| AliasFields {
                entries: aliases
                    .entries
                    .iter()
                    .map(|entry| AliasField {
                        key: &entry.key,
                        value: entry.value.as_deref().map_err(Causes::clone),
                    })
                    .collect(),
                whole: aliases.whole.clone(),
            }),
            message_fallback: borrowed(&self.message_fallback, |entries| {
                entries
                    .iter()
                    .map(|entry| FallbackField {
                        key: &entry.key,
                        sequence: borrowed(&entry.sequence, |candidates| {
                            candidates
                                .iter()
                                .map(|candidate| candidate.as_ref().map_err(Causes::clone))
                                .collect()
                        }),
                    })
                    .collect()
            }),
            coverage: borrowed(&self.coverage, |coverage| CoverageFields {
                default_mode: coverage.default_mode.clone(),
                rules: borrowed(&coverage.rules, |rules| {
                    rules
                        .iter()
                        .map(|rule| {
                            borrowed(rule, |rule| RuleField {
                                content: rule.content.clone(),
                                requested_locales: selector(&rule.requested_locales),
                                intent_surface_classes: selector(&rule.intent_surface_classes),
                                mode: rule.mode.clone(),
                            })
                        })
                        .collect()
                }),
                locale_items: coverage.locale_items.clone(),
                surface_items: coverage.surface_items.clone(),
                whole: coverage.whole.clone(),
            }),
            target_entries: borrowed(&self.target_entries, |targets| TargetMapFields {
                submitted: targets.submitted,
                entries: targets
                    .entries
                    .iter()
                    .map(|entry| TargetLocaleFields {
                        id: &entry.id,
                        requested_locales: strings(&entry.requested_locales),
                        default_requested_locale: borrowed(
                            &entry.default_requested_locale,
                            Option::as_deref,
                        ),
                    })
                    .collect(),
                keys: targets.keys.clone(),
                whole: targets.whole.clone(),
            }),
        }
    }

    /// The Group ID map entry by entry, which Phase 12 reads.
    pub(crate) fn group_fields(&self) -> Admitted<GroupFields<'_>> {
        borrowed(&self.group_entries, |groups| GroupFields {
            submitted: groups.submitted,
            memberships: groups.memberships.clone(),
            keys: groups.keys.clone(),
            entries: groups
                .entries
                .iter()
                .map(|entry| GroupField {
                    id: &entry.id,
                    members: borrowed(&entry.members, |members| {
                        members
                            .iter()
                            .map(|member| member.as_ref().map_err(Causes::clone))
                            .collect()
                    }),
                })
                .collect(),
        })
    }

    /// The relations of the Group ID map entry by entry, which Phase 13
    /// reads, and whether everything at and below the map passed.
    pub(crate) fn hydration_fields(&self) -> Admitted<HydrationFields<'_>> {
        borrowed(&self.group_entries, |groups| HydrationFields {
            occurrences: groups.relations.clone(),
            groups: groups
                .entries
                .iter()
                .map(|entry| RelationGroup {
                    id: &entry.id,
                    relations: borrowed(&entry.relations, |relations| {
                        relations
                            .iter()
                            .map(|relation| {
                                borrowed(relation, |relation| RelationField {
                                    key: relation.key.clone(),
                                    server: relation.server.as_ref().map_err(Causes::clone),
                                    client: relation.client.as_ref().map_err(Causes::clone),
                                })
                            })
                            .collect()
                    }),
                })
                .collect(),
            whole: groups.whole.clone(),
        })
    }

    pub(crate) fn delivery(&self) -> Admitted<Option<&Delivery>> {
        borrowed(&self.delivery, Option::as_ref)
    }
}

impl<Policy, Target> DeclarationFields<Policy, Target> {
    /// The fields of no declaration: each unavailable with `causes`, why
    /// profile selection selected none, so that each check that reads one is
    /// not evaluated with them (D15:2132).
    pub(crate) fn unavailable(causes: &Causes) -> Self {
        Self {
            project_id: Err(causes.clone()),
            selection_scope: Err(causes.clone()),
            locale_spellings: Err(causes.clone()),
            requested_locales: Err(causes.clone()),
            default_requested_locale: Err(causes.clone()),
            default_source_locale: Err(causes.clone()),
            negotiation_aliases: Err(causes.clone()),
            message_fallback: Err(causes.clone()),
            coverage: Err(causes.clone()),
            policies: PolicySlots {
                slots: PolicySlot::unavailable(causes),
                object: Err(causes.clone()),
            },
            target_entries: Err(causes.clone()),
            group_entries: Err(causes.clone()),
            delivery: Err(causes.clone()),
        }
    }
}

impl DeclarationFields<PolicyReference, TargetProfileReference> {
    /// The fields that hold artifact references, which Phases 4 and 9 read.
    pub(crate) fn references(&self) -> DeclarationReferences<'_> {
        DeclarationReferences {
            policies: PolicyReferences {
                slots: self
                    .policies
                    .slots
                    .iter()
                    .map(|(slot, reference)| (*slot, reference.as_ref().map_err(Causes::clone)))
                    .collect(),
                object: self.policies.object.clone(),
            },
            target_profiles: borrowed(&self.target_entries, |targets| TargetReferences {
                entries: targets
                    .entries
                    .iter()
                    .map(|entry| (&entry.id, borrowed(&entry.profile, |profile| profile)))
                    .collect(),
                whole: targets.whole.clone(),
            }),
        }
    }
}

/// An array of strings, element by element, borrowed.
fn strings(items: &Admitted<Vec<Admitted<String>>>) -> Admitted<Vec<Admitted<&str>>> {
    borrowed(items, |items| {
        items
            .iter()
            .map(|item| item.as_deref().map_err(Causes::clone))
            .collect()
    })
}

/// A coverage selector, item by item, borrowed.
fn selector(items: &SelectorEntry) -> SelectorField<'_> {
    borrowed(items, |items| {
        items.as_ref().map(|items| {
            items
                .iter()
                .map(|item| item.as_deref().map_err(Causes::clone))
                .collect()
        })
    })
}

/// `field` borrowed through `project`, or its causes.
fn borrowed<'a, T, U>(field: &'a Admitted<T>, project: impl FnOnce(&'a T) -> U) -> Admitted<U> {
    field.as_ref().map(project).map_err(Causes::clone)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{json, Value};

    use crate::fixtures::{
        complete_config, minimal_config, FixturePolicyReference, FixtureTargetReference,
    };
    use crate::resolver::evaluation::{Admitted, Check, Evaluation, IssueKey, Unit};
    use crate::resolver::structural_admission::StructuralInvariant;
    use crate::resolver::value_admissions::tests::{structured_in, Analysis};
    use crate::resolver::value_admissions::ValueAdmissions;

    use super::{CoverageEntries, DeclarationFields};

    type Fields = DeclarationFields<FixturePolicyReference, FixtureTargetReference>;

    /// Phase 2 over `value`: what its candidates left, and its analysis.
    fn structured(value: &Value) -> (ValueAdmissions, Analysis) {
        let (_, values, analysis) = structured_in(Evaluation::default(), value);
        (values, analysis)
    }

    /// The fields of the declaration `app` of `value`.
    fn read(value: &Value) -> Fields {
        let (values, analysis) = structured(value);
        Fields::read(&values, &analysis, "app").unwrap()
    }

    /// Whether everything at and below the Target ID map passed.
    fn target_structure<Policy, Target>(
        fields: &DeclarationFields<Policy, Target>,
    ) -> Admitted<()> {
        fields
            .target_entries
            .as_ref()
            .map_err(super::Causes::clone)
            .and_then(|targets| targets.whole.clone())
    }

    /// Whether everything at and below the Group ID map passed.
    fn group_structure<Policy, Target>(fields: &DeclarationFields<Policy, Target>) -> Admitted<()> {
        fields
            .group_entries
            .as_ref()
            .map_err(super::Causes::clone)
            .and_then(|groups| groups.whole.clone())
    }

    /// Whether every slot that holds a reference was admitted.
    fn policies_held<Policy, Target>(fields: &DeclarationFields<Policy, Target>) -> bool {
        fields.policies.slots.iter().all(|(_, slot)| slot.is_ok())
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
        assert_eq!(fields.requested_locales, Ok(vec![Ok("en".to_owned())]));
        assert_eq!(fields.default_source_locale, Ok(None));
        assert_eq!(fields.coverage, Ok(CoverageEntries::default()));
        assert_eq!(fields.delivery, Ok(None));
        assert!(policies_held(&fields));
        assert!(target_structure(&fields).is_ok() && group_structure(&fields).is_ok());
        let complete = read(&complete_config());
        assert_eq!(
            complete.negotiation_aliases.as_ref().unwrap().entries.len(),
            1
        );
        assert_eq!(complete.message_fallback.as_ref().unwrap().len(), 1);
        let rules = complete.coverage.as_ref().unwrap().rules.as_ref();
        assert_eq!(rules.unwrap().len(), 3);
        // Each accessor hands on the field it reads.
        assert!(matches!(complete.delivery(), Ok(Some(_))));
        assert_eq!(fields.delivery(), Ok(None));
    }

    /// A field structural admission rejected is unavailable with the
    /// Findings under it, and its siblings stay admitted (D15:2368, 3476).
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
        // The element that failed is on its own, beside the one that did not.
        let items = fields.requested_locales.as_ref().unwrap();
        assert_eq!(items[0], Ok("en".to_owned()));
        assert_eq!(
            checks(&items[1]),
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
        assert!(policies_held(&fields));
        assert!(target_structure(&fields).is_ok() && group_structure(&fields).is_ok());
    }

    /// An undeclared member of the declaration belongs to no field.
    #[test]
    fn an_undeclared_member_holds_back_no_field() {
        let mut config = minimal_config();
        config["profiles"]["app"]["extra"] = json!(true);
        let fields = read(&config);
        assert!(fields.project_id.is_ok() && fields.requested_locales.is_ok());
        assert!(policies_held(&fields) && target_structure(&fields).is_ok());
    }

    /// A Target ID entry's failure holds back `targetProfiles` alone, its key
    /// and its fields both under it.
    #[test]
    fn a_target_failure_holds_back_target_profiles_alone() {
        let mut config = minimal_config();
        config["profiles"]["app"]["targetProfiles"]["browser"]["requestedLocales"] = json!("en");
        let fields = read(&config);
        assert_eq!(
            checks(&target_structure(&fields)),
            BTreeSet::from([(
                vec!["targetProfiles".to_owned(), "*".to_owned()],
                Check::FieldType
            )])
        );
        assert!(group_structure(&fields).is_ok() && fields.requested_locales.is_ok());
        let mut keyed = minimal_config();
        let target = keyed["profiles"]["app"]["targetProfiles"]["browser"].clone();
        keyed["profiles"]["app"]["targetProfiles"]["Web App"] = target;
        let fields = read(&keyed);
        assert_eq!(
            checks(&target_structure(&fields)),
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
            Fields::read(&ValueAdmissions::default(), &analysis, "app").err(),
            Some(StructuralInvariant::UnreportedFailure)
        );
        let mut missing = minimal_config();
        missing["profiles"]["app"]
            .as_object_mut()
            .unwrap()
            .remove("projectId");
        let (_, analysis) = structured(&missing);
        assert_eq!(
            Fields::read(&ValueAdmissions::default(), &analysis, "app").err(),
            Some(StructuralInvariant::UnreportedFailure)
        );
        // An optional field too: present, its checks admitting it, yet not
        // admitted by the analysis.
        let mut optional = minimal_config();
        optional["profiles"]["app"]["coverage"] = json!(false);
        let (_, analysis) = structured(&optional);
        assert_eq!(
            Fields::read(&ValueAdmissions::default(), &analysis, "app").err(),
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
            checks(&group_structure(&fields)),
            BTreeSet::from([(
                vec!["deploymentGroups".to_owned(), "*".to_owned()],
                Check::FieldType
            )])
        );
        assert!(target_structure(&fields).is_ok() && policies_held(&fields));
    }

    /// Another declaration's failures hold back none of this one's fields.
    #[test]
    fn another_declarations_failure_holds_back_no_field() {
        let mut config = minimal_config();
        let mut web = config["profiles"]["app"].clone();
        web["coverage"] = json!(false);
        config["profiles"]["web"] = web;
        let fields = read(&config);
        assert_eq!(fields.coverage, Ok(CoverageEntries::default()));
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

    /// Without a selected declaration every field is unavailable with the
    /// causes profile selection gives.
    #[test]
    fn no_selected_declaration_leaves_every_field_with_the_selections_causes() {
        let mut config = minimal_config();
        config["profiles"]["app"]["projectId"] = json!(1);
        let causes = read(&config).project_id.unwrap_err();
        let fields = Fields::unavailable(&causes);
        assert_eq!(fields.locale_spellings, Err(causes.clone()));
        let unavailable = [
            fields.project_id.as_ref().err(),
            fields.selection_scope.as_ref().err(),
            fields.requested_locales.as_ref().err(),
            fields.default_requested_locale.as_ref().err(),
            fields.default_source_locale.as_ref().err(),
            fields.negotiation_aliases.as_ref().err(),
            fields.message_fallback.as_ref().err(),
            fields.coverage.as_ref().err(),
            fields.policies.object.as_ref().err(),
            fields.target_entries.as_ref().err(),
            fields.group_entries.as_ref().err(),
            fields.delivery.as_ref().err(),
        ];
        assert!(unavailable.iter().all(|field| *field == Some(&causes)));
        // Each slot is named without a value (D15:1172, 2132).
        assert_eq!(
            fields.policies.slots,
            crate::resolver::resource_policy::PolicySlot::ALL
                .map(|slot| (slot, Err(causes.clone())))
        );
    }
}
