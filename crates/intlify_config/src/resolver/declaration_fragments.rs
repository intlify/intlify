// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The selected declaration as the semantic phases read it: each member's
//! value once every candidate at and below it passed, or the causes of those
//! that did not, as [`ValueAdmissions`] kept them (D15:393, 2367, 3340). The
//! checks decide: a member they admitted and the analysis did not, or a
//! required one they admitted though it is absent, is a resolver bug.

use serde::de::DeserializeOwned;

use crate::materialize::{MaterializedDocument, NodeId};
use crate::structural::{Member, StructuralAnalysis};

use super::evaluation::Admitted;
use super::structural_admission::StructuralInvariant;
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
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use crate::fixtures::{complete_config, minimal_config};
    use crate::resolver::evaluation::{Admitted, Evaluation, Subject, Unit};
    use crate::resolver::structural_admission::{
        EntryKey, StructuralInvariant, StructuralSubject as S,
    };
    use crate::resolver::value_admissions::tests::{structured_in, Analysis, FIELDS};
    use crate::resolver::value_admissions::ValueAdmissions;

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
}
