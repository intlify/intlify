// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The selected declaration as the semantic phases read it: each member's
//! value once every candidate at and below it passed, or the causes of those
//! that did not, as [`ValueAdmissions`] kept them (D15:393, 2367, 3340). The
//! checks decide: a member they admitted and the analysis did not, or a
//! required one they admitted though it is absent, is a resolver bug.

use serde::de::value::{BorrowedStrDeserializer, Error as KeyError};
use serde::de::{Deserialize, DeserializeOwned};

use crate::materialize::{MaterializedDocument, NodeId, NodeKind, ObjectMember};
use crate::model::{RequiredNullable, TargetId};
use crate::references::{PolicyReference, TargetProfileReference};
use crate::structural::{AdmissionInvariant, Member, StructuralAnalysis};

use super::evaluation::Admitted;
use super::resource_policy::{PolicySlot, ReferenceField, ReferenceOccurrence};
use super::structural_admission::StructuralInvariant;
use super::tally::{total, Tally};
use super::value_admissions::ValueAdmissions;

/// The declaration `id` of an analysis, with what structural admission left
/// at its values.
pub(crate) struct DeclarationFragments<'r, Policy, Target> {
    values: &'r ValueAdmissions,
    analysis: &'r StructuralAnalysis<Policy, Target>,
    document: &'r MaterializedDocument,
    declaration: NodeId,
    id: &'r str,
}

impl<'r, Policy, Target> DeclarationFragments<'r, Policy, Target> {
    /// The declaration `id` of `analysis`.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a declaration the analysis does not hold.
    pub(crate) fn new(
        values: &'r ValueAdmissions,
        analysis: &'r StructuralAnalysis<Policy, Target>,
        id: &'r str,
    ) -> Result<Self, StructuralInvariant> {
        let (document, declaration) = analysis
            .declaration_value(id)
            .ok_or(StructuralInvariant::UnreportedFailure)?;
        Ok(Self {
            values,
            analysis,
            document,
            declaration,
            id,
        })
    }

    /// The optional member `name`: its value once the checks at and below it
    /// admitted it, absent when it is, or the causes.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a member the checks admitted and the
    /// analysis did not.
    pub(crate) fn optional<T: DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Admitted<Option<T>>, StructuralInvariant> {
        let member = self
            .analysis
            .profile_member(self.id, name)
            .map_err(StructuralInvariant::Admission)?;
        let admission = self.values.member(self.document, self.declaration, name);
        match (admission, member) {
            (Err(causes), _) => Ok(Err(causes)),
            (Ok(()), Member::Admitted(value)) => Ok(Ok(Some(value))),
            (Ok(()), Member::Absent) => Ok(Ok(None)),
            // The checks decide, so a member they admitted and the analysis
            // did not is a resolver bug.
            (Ok(()), Member::Unadmitted) => Err(StructuralInvariant::UnreportedFailure),
        }
    }

    /// The required member `name`, whose absence its required check reports.
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a member the checks admitted and the
    /// analysis did not, or of one they admitted though it is absent.
    pub(crate) fn required<T: DeserializeOwned>(
        &self,
        name: &str,
    ) -> Result<Admitted<T>, StructuralInvariant> {
        match self.optional(name)? {
            Ok(Some(value)) => Ok(Ok(value)),
            Ok(None) => Err(StructuralInvariant::UnreportedFailure),
            Err(causes) => Ok(Err(causes)),
        }
    }

    /// Every locale spelling of the declaration, as `maxLocaleOccurrences`
    /// counts them, whatever structural admission made of the fields that hold
    /// them: each string at a locale role and each key of a map keyed by
    /// locale. A value that is no string is no spelling, and an invalid string
    /// is one (D15:1896, 1907, 2184, 3348).
    pub(crate) fn locale_spellings(&self) -> Tally {
        let declaration = self.declaration;
        let member = |object, name| self.member_value(object, name);
        let aliases = member(declaration, "localeNegotiation")
            .and_then(|negotiation| member(negotiation, "aliases"));
        let rules = member(declaration, "coverage").and_then(|coverage| member(coverage, "rules"));
        let selectors = match rules.map(|rules| self.document.node(rules).kind()) {
            Some(NodeKind::Array(rules)) => total(
                rules
                    .iter()
                    .map(|&rule| self.strings_in(member(rule, "requestedLocales"))),
            ),
            _ => Tally::of(0),
        };
        total([
            self.strings_in(member(declaration, "requestedLocales")),
            self.string_at(member(declaration, "defaultRequestedLocale")),
            self.string_at(member(declaration, "defaultSourceLocale")),
            total(
                self.entries(aliases)
                    .map(|value| Tally::of(1).plus(self.string_at(Some(value)))),
            ),
            total(
                self.entries(member(declaration, "messageFallback"))
                    .map(|sequence| Tally::of(1).plus(self.strings_in(Some(sequence)))),
            ),
            selectors,
            total(
                self.entries(member(declaration, "targetProfiles"))
                    .map(|target| {
                        self.strings_in(member(target, "requestedLocales"))
                            .plus(self.string_at(member(target, "defaultRequestedLocale")))
                    }),
            ),
        ])
    }

    /// One when `value` is a string.
    fn string_at(&self, value: Option<NodeId>) -> Tally {
        Tally::of(usize::from(value.is_some_and(|value| {
            matches!(self.document.node(value).kind(), NodeKind::String(_))
        })))
    }

    /// The strings among the elements of `array`, when it is one.
    fn strings_in(&self, array: Option<NodeId>) -> Tally {
        match array.map(|array| self.document.node(array).kind()) {
            Some(NodeKind::Array(items)) => {
                total(items.iter().map(|&item| self.string_at(Some(item))))
            }
            _ => Tally::of(0),
        }
    }

    /// The value of each entry of `object`, when it is one.
    fn entries(&self, object: Option<NodeId>) -> impl Iterator<Item = NodeId> + '_ {
        object
            .map(|object| self.document.node(object).kind())
            .into_iter()
            .filter_map(|kind| match kind {
                NodeKind::Object(members) => Some(members),
                _ => None,
            })
            .flat_map(|members| members.values().map(ObjectMember::value))
    }

    /// The value of the member `name` of `object`, when it holds one.
    fn member_value(&self, object: NodeId, name: &str) -> Option<NodeId> {
        match self.document.node(object).kind() {
            NodeKind::Object(members) => members.get(name).map(ObjectMember::value),
            _ => None,
        }
    }
}

impl DeclarationFragments<'_, PolicyReference, TargetProfileReference> {
    /// Every reference of the declaration, whether or not structural
    /// admission admitted it, with what Phase 4's bounds read of each: each
    /// `policies` slot but an admitted `null`, then each entry's `profile`
    /// (D15:1918, 1924).
    ///
    /// # Errors
    ///
    /// Returns the resolver bug of a reference the checks admitted that is
    /// absent or that its type cannot read.
    pub(crate) fn reference_occurrences(
        &self,
    ) -> Result<Vec<ReferenceOccurrence>, StructuralInvariant> {
        let policies = self
            .member_value(self.declaration, "policies")
            .ok_or(StructuralInvariant::UnreportedFailure)?;
        let mut references = Vec::with_capacity(PolicySlot::ALL.len());
        for slot in PolicySlot::ALL {
            let value = self.member_value(policies, slot.name());
            let admission = self.values.member(self.document, policies, slot.name());
            let Some(bytes) = self.slot_bytes(admission, value)? else {
                continue;
            };
            references.push(ReferenceOccurrence {
                field: Some(ReferenceField::Policy(slot)),
                occurs: value.is_some(),
                bytes,
            });
        }
        if let Some(targets) = self.member_value(self.declaration, "targetProfiles") {
            if let NodeKind::Object(entries) = self.document.node(targets).kind() {
                for (key, entry) in entries {
                    references.extend(self.target_occurrence(key, entry.value())?);
                }
            }
        }
        Ok(references)
    }

    /// A slot's canonical bytes, why it has none, or nothing for an admitted
    /// `null`, the explicit absence of a reference.
    fn slot_bytes(
        &self,
        admission: Admitted<()>,
        value: Option<NodeId>,
    ) -> Result<Option<Admitted<u64>>, StructuralInvariant> {
        if let Err(causes) = admission {
            return Ok(Some(Err(causes)));
        }
        let value = value.ok_or(StructuralInvariant::UnreportedFailure)?;
        Ok(
            match self.decoded::<RequiredNullable<PolicyReference>>(value)? {
                RequiredNullable::Reference(reference) => Some(Ok(reference.canonical_bytes())),
                RequiredNullable::Null => None,
            },
        )
    }

    /// The `profile` of the entry `key` of `targetProfiles`. An entry whose key
    /// failed has no later subject, so one without a `profile` has nothing
    /// to report here (D15:2184).
    fn target_occurrence(
        &self,
        key: &str,
        entry: NodeId,
    ) -> Result<Option<ReferenceOccurrence>, StructuralInvariant> {
        let field = match self.values.key(entry) {
            Ok(()) => Some(ReferenceField::TargetProfile(
                TargetId::deserialize(BorrowedStrDeserializer::<KeyError>::new(key))
                    .map_err(|_| StructuralInvariant::UnreportedFailure)?,
            )),
            Err(_) => None,
        };
        let value = self.member_value(entry, "profile");
        let bytes = match (self.values.member(self.document, entry, "profile"), value) {
            (Err(_), None) if field.is_none() => return Ok(None),
            (Err(causes), _) => Err(causes),
            (Ok(()), Some(value)) => Ok(self
                .decoded::<TargetProfileReference>(value)?
                .canonical_bytes()),
            (Ok(()), None) => return Err(StructuralInvariant::UnreportedFailure),
        };
        Ok(Some(ReferenceOccurrence {
            field,
            occurs: value.is_some(),
            bytes,
        }))
    }

    /// The admitted value at `node`, as `T`.
    fn decoded<T: DeserializeOwned>(&self, node: NodeId) -> Result<T, StructuralInvariant> {
        self.document
            .decode(node)
            .map_err(|_| StructuralInvariant::Admission(AdmissionInvariant::AuthoringTypeMismatch))
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::fixtures::{complete_config, minimal_config};
    use crate::model::ProfileDeclaration;
    use crate::references::{PolicyReference, TargetProfileReference};
    use crate::resolver::evaluation::{Admitted, Evaluation, Subject, Unit};
    use crate::resolver::locales::fixtures::declaration;
    use crate::resolver::locales::LocaleFields;
    use crate::resolver::profile_resolution::tests::Inputs;
    use crate::resolver::resource_policy::{
        DeclarationReferences, PolicySlot, ReferenceField, ReferenceOccurrence,
    };
    use crate::resolver::structural_admission::{
        EntryKey, StructuralInvariant, StructuralSubject as S,
    };
    use crate::resolver::value_admissions::tests::{
        structured_as, structured_in, Analysis, FIELDS,
    };
    use crate::resolver::value_admissions::ValueAdmissions;
    use crate::structural::{AdmissionInvariant, StructuralAnalysis};

    use super::DeclarationFragments;

    /// One way to break the declaration, by its name.
    type Edit = (&'static str, fn(&mut Value));

    /// Whether `unit` lies under the field `name` of the declaration `app`,
    /// the way the field's units are named: its path from the declaration, or
    /// a key or a field of an entry of the map the field holds.
    fn under(unit: &Unit, name: &str) -> bool {
        let app = |key: &EntryKey| matches!(key, EntryKey::Identity(id) if &**id == "app");
        match &unit.subject {
            Subject::Structural(S::ProfileField(profile, field)) => {
                app(profile) && field.path.first().is_some_and(|first| &**first == name)
            }
            Subject::Structural(S::TargetProfileKey(profile, _) | S::TargetField(profile, ..)) => {
                app(profile) && name == "targetProfiles"
            }
            Subject::Structural(S::DeploymentGroupKey(profile, _) | S::GroupField(profile, ..)) => {
                app(profile) && name == "deploymentGroups"
            }
            _ => false,
        }
    }

    /// What the units under each field say of it: their blocking results,
    /// and the causes of each not evaluated.
    fn by_units(evaluation: &Evaluation, name: &str) -> Admitted<()> {
        evaluation.admitted_within(|unit| under(unit, name))
    }

    /// Each field's fragment holds exactly what the units under it found.
    fn agrees(evaluation: &Evaluation, values: &ValueAdmissions, analysis: &Analysis, case: &str) {
        let fragments = DeclarationFragments::new(values, analysis, "app").unwrap();
        for name in FIELDS {
            let read = fragments.optional::<Value>(name).unwrap().map(|_| ());
            assert_eq!(read, by_units(evaluation, name), "{case}: {name}");
        }
    }

    /// Each kind of failure at each depth holds back the fields whose units
    /// found it, and no other.
    #[test]
    fn a_fragment_holds_what_the_units_of_its_field_found() {
        let edits: [Edit; 14] = [
            ("complete", |_| {}),
            ("scalar", |app| app["projectId"] = json!("Store Front")),
            ("element", |app| app["requestedLocales"] = json!(["en", 7])),
            ("container", |app| app["requestedLocales"] = json!("en")),
            ("missing", |app| {
                app.as_object_mut().unwrap().remove("selectionScope");
            }),
            ("undeclared", |app| app["extra"] = json!(1)),
            ("alias", |app| {
                app["localeNegotiation"] = json!({"aliases": {"fr": 1}, "x": 1});
            }),
            ("fallback", |app| {
                app["messageFallback"] = json!({"ja": [1]});
            }),
            ("rule", |app| {
                app["coverage"] = json!({"rules": [{"mode": "x"}]});
            }),
            ("slot", |app| app["policies"]["trust"] = json!(1)),
            ("target key", |app| {
                let target = app["targetProfiles"]["browser"].clone();
                app["targetProfiles"]["Web App"] = target;
            }),
            ("target field", |app| {
                app["targetProfiles"]["browser"]["requestedLocales"] = json!([1]);
            }),
            ("group", |app| {
                let groups = app["deploymentGroups"].as_object_mut().unwrap();
                let group = groups.values_mut().next().unwrap();
                group["hydrationRelations"] = json!([{"server": 1}]);
            }),
            ("delivery", |app| {
                app["delivery"] = json!({"placement": "hoist"});
            }),
        ];
        for (case, edit) in edits {
            let mut config = complete_config();
            edit(&mut config["profiles"]["app"]);
            let (evaluation, values, analysis) = structured_in(Evaluation::default(), &config);
            agrees(&evaluation, &values, &analysis, case);
        }
    }

    /// Whichever structural unit of a complete configuration blocks, each
    /// fragment holds what the units of its field say: the checks decide. A
    /// unit before the scope levels leaves no analysis to read.
    #[test]
    fn a_blocked_unit_holds_back_the_fragments_of_its_field() {
        let config = complete_config();
        let (complete, _, _) = structured_in(Evaluation::default(), &config);
        let scope_levels = complete
            .candidates()
            .map(|(unit, _)| unit)
            .filter(|unit| matches!(unit.subject, Subject::Structural(_)));
        for unit in scope_levels {
            let blocking = Evaluation::blocking(unit.clone());
            let (evaluation, values, analysis) = structured_in(blocking, &config);
            agrees(&evaluation, &values, &analysis, &format!("{unit:?}"));
        }
    }

    /// A declaration the analysis does not hold is a resolver bug.
    #[test]
    fn a_declaration_the_analysis_lacks_is_a_resolver_bug() {
        let (_, values, analysis) = structured_in(Evaluation::default(), &minimal_config());
        assert_eq!(
            DeclarationFragments::new(&values, &analysis, "absent").err(),
            Some(StructuralInvariant::UnreportedFailure)
        );
    }

    /// What the document holds of a declaration admitted whole is what its
    /// type counts, every role included (D15:1907).
    #[test]
    fn the_spellings_of_a_declaration_admitted_whole_are_those_its_type_counts() {
        for config in [minimal_config(), complete_config()] {
            let (_, values, analysis) = structured_in(Evaluation::default(), &config);
            let fragments = DeclarationFragments::new(&values, &analysis, "app").unwrap();
            let declared = declaration(&config, |_| {});
            assert_eq!(
                LocaleFields::declared(&declared).spellings,
                Ok(fragments.locale_spellings())
            );
        }
    }

    type Referencing = StructuralAnalysis<PolicyReference, TargetProfileReference>;

    /// The complete example's configuration once `edit` changed its
    /// declaration, and Phase 2 over it.
    fn analyzed(edit: impl FnOnce(&mut Value)) -> (ValueAdmissions, Referencing) {
        let mut app = Inputs::complete().declaration().clone();
        edit(&mut app);
        let config = json!({"schemaVersion": "0", "profiles": {"app": app}});
        let (_, values, analysis) = structured_as(Evaluation::default(), &config);
        (values, analysis)
    }

    fn occurrences(edit: impl FnOnce(&mut Value)) -> Vec<ReferenceOccurrence> {
        let (values, analysis) = analyzed(edit);
        DeclarationFragments::new(&values, &analysis, "app")
            .unwrap()
            .reference_occurrences()
            .unwrap()
    }

    /// Whether the reference at `field` occurs, and whether it has bytes.
    fn at(references: &[ReferenceOccurrence], field: &ReferenceField) -> (bool, bool) {
        let found = references
            .iter()
            .find(|each| each.field.as_ref() == Some(field));
        let found = found.unwrap();
        (found.occurs, found.bytes.is_ok())
    }

    fn server() -> ReferenceField {
        ReferenceField::TargetProfile(serde_json::from_value(json!("server")).unwrap())
    }

    fn lacking(app: &mut Value, object: &[&str], name: &str) {
        let held = object.iter().fold(app, |value, step| &mut value[*step]);
        held.as_object_mut().unwrap().remove(name);
    }

    /// Each slot of a complete declaration has its bytes. A value that failed
    /// occurs without them, a required member that is missing does not occur,
    /// and an admitted `null` is no reference (D15:1918, 1924).
    #[test]
    fn each_slot_occurs_as_structural_admission_left_it() {
        let app = Inputs::complete().declaration().clone();
        let declared: ProfileDeclaration<_, _> = serde_json::from_value(app).unwrap();
        let typed = DeclarationReferences {
            policies: Ok(&declared.policies),
            target_profiles: Ok(&declared.target_profiles),
        };
        assert_eq!(Ok(occurrences(|_| {})), typed.occurrences());
        let trust = ReferenceField::Policy(PolicySlot::Trust);
        let references = occurrences(|app| app["policies"]["trust"] = json!(42));
        assert_eq!(at(&references, &trust), (true, false));
        let references = occurrences(|app| app["policies"]["trust"] = Value::Null);
        assert_eq!(at(&references, &trust), (true, false));
        let references = occurrences(|app| lacking(app, &["policies"], "trust"));
        assert_eq!(at(&references, &trust), (false, false));
        let routing = Some(ReferenceField::Policy(PolicySlot::ProviderRouting));
        let references = occurrences(|app| app["policies"]["providerRouting"] = Value::Null);
        assert_eq!(references.len(), 8);
        assert!(!references.iter().any(|each| each.field == routing));
    }

    /// A target's `profile` occurs as a slot does; one under a key that
    /// failed has no subject but still counts with its bytes; and a map of
    /// the wrong type holds none (D15:1918, 1924, 2184).
    #[test]
    fn each_target_profile_occurs_as_structural_admission_left_it() {
        let references = occurrences(|app| app["targetProfiles"]["server"]["profile"] = json!(1));
        assert_eq!(at(&references, &server()), (true, false));
        let references = occurrences(|app| lacking(app, &["targetProfiles", "server"], "profile"));
        assert_eq!(at(&references, &server()), (false, false));
        let references = occurrences(|app| app["targetProfiles"]["server"] = json!(7));
        assert_eq!(at(&references, &server()), (false, false));
        let references = occurrences(|app| {
            app["targetProfiles"]["Web App"] = app["targetProfiles"]["server"].clone();
        });
        assert_eq!(references.len(), 10);
        let unnamed = references.iter().find(|each| each.field.is_none()).unwrap();
        assert!(unnamed.occurs && unnamed.bytes.is_ok());
        let failed = occurrences(|app| app["targetProfiles"]["Web App"] = json!({"profile": 1}));
        let unnamed = failed.iter().find(|each| each.field.is_none()).unwrap();
        assert!(unnamed.occurs && unnamed.bytes.is_err());
        let absent = occurrences(|app| app["targetProfiles"]["Web App"] = json!({}));
        assert_eq!(absent.len(), 9);
        assert_eq!(occurrences(|app| app["targetProfiles"] = json!(7)).len(), 7);
    }

    /// What the checks admitted and the value does not hold is a resolver
    /// bug.
    #[test]
    fn a_reference_the_checks_admit_and_the_value_lacks_is_a_resolver_bug() {
        let bug = |edit: fn(&mut Value)| {
            let (_, analysis) = analyzed(edit);
            DeclarationFragments::new(&ValueAdmissions::default(), &analysis, "app")
                .unwrap()
                .reference_occurrences()
                .err()
        };
        let unreported: [fn(&mut Value); 4] = [
            |app| lacking(app, &[], "policies"),
            |app| lacking(app, &["policies"], "trust"),
            |app| lacking(app, &["targetProfiles", "server"], "profile"),
            |app| app["targetProfiles"]["Web App"] = app["targetProfiles"]["server"].clone(),
        ];
        for edit in unreported {
            assert_eq!(bug(edit), Some(StructuralInvariant::UnreportedFailure));
        }
        let mismatched: [fn(&mut Value); 2] = [
            |app| app["policies"]["trust"] = json!(42),
            |app| app["targetProfiles"]["server"]["profile"] = json!(42),
        ];
        for edit in mismatched {
            assert_eq!(
                bug(edit),
                Some(StructuralInvariant::Admission(
                    AdmissionInvariant::AuthoringTypeMismatch
                ))
            );
        }
    }
}
