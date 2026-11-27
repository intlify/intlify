// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What structural admission left at each value of the materialized
//! configuration: the causes of every candidate that did not pass, at the
//! value it looked at. One unit keyed by a path with `*` holds every element
//! or entry of the path, so its result cannot tell them apart, while each of
//! its candidates has a result of its own (D15:2185, 2204). Reading a value
//! from its candidates therefore keeps a failure of one element out of its
//! siblings (D15:394, 2368, 3476).
//!
//! A value's candidates are its own checks. The check of an identity map's
//! key is kept apart, by the entry's value: an entry without an admitted key
//! has no subject in a later phase, while its fields are still checked
//! (D15:2185). An undeclared member is a failure of the object that holds it,
//! and a missing required member is kept by its object and its name, since it
//! has no value (D15:2181, 2185). Only what did not pass is kept, so a
//! configuration structural admission admitted whole holds nothing here.

use std::collections::BTreeMap;

use crate::materialize::{MaterializedDocument, NodeId, NodeKind, ObjectMember};
use crate::structural::facts::{At, Candidate, Facts};

use super::evaluation::{Admitted, Causes};

/// The causes of each candidate of structural admission that did not pass,
/// where it looked.
#[derive(Debug, Default)]
pub(crate) struct ValueAdmissions {
    /// By the value a candidate looked at.
    values: BTreeMap<NodeId, Causes>,
    /// By the value of the entry whose key a candidate looked at.
    keys: BTreeMap<NodeId, Causes>,
    /// By the object that lacks a required member, and the member's name.
    missing: BTreeMap<NodeId, BTreeMap<Box<str>, Causes>>,
}

impl ValueAdmissions {
    /// Keeps `causes`, why `candidate` of `facts` did not pass.
    pub(crate) fn reject(&mut self, facts: &Facts, candidate: &Candidate, causes: &Causes) {
        match candidate.at {
            At::Value(value) | At::Member { object: value, .. } => {
                join(&mut self.values, value, causes);
            }
            At::Key(scope) => {
                let entry = facts
                    .scope(scope)
                    .entry
                    .expect("a key's scope is its entry's");
                join(&mut self.keys, entry.value, causes);
            }
            At::Missing { object } => {
                let steps = facts.steps(candidate.path);
                let name = steps.last().expect("a missing member's path names it");
                join(
                    self.missing.entry(object).or_default(),
                    Box::from(name.name()),
                    causes,
                );
            }
        }
    }

    /// Whether the check of the key of the entry whose value is `entry`
    /// passed, or its causes.
    pub(crate) fn key(&self, entry: NodeId) -> Admitted<()> {
        self.keys.get(&entry).cloned().map_or(Ok(()), Err)
    }

    /// Whether every candidate that looked at `value` itself passed, or the
    /// causes of those that did not: an array's own type and not its elements',
    /// each of which has a result of its own (D15:2185).
    pub(crate) fn value(&self, value: NodeId) -> Admitted<()> {
        self.values.get(&value).cloned().map_or(Ok(()), Err)
    }

    /// Whether every candidate of the member `name` of `object` passed, those
    /// of the values below it and of the keys of their entries included, or
    /// the causes of those that did not. A member `object` does not hold has
    /// only the check that requires it, and a value that is no object holds
    /// none for the failure of its own type.
    pub(crate) fn member(
        &self,
        document: &MaterializedDocument,
        object: NodeId,
        name: &str,
    ) -> Admitted<()> {
        let mut found = self
            .missing
            .get(&object)
            .and_then(|missing| missing.get(name))
            .cloned();
        let mut pending: Vec<NodeId> = Vec::new();
        match document.node(object).kind() {
            NodeKind::Object(members) => {
                pending.extend(members.get(name).map(ObjectMember::value));
            }
            _ => found = self.values.get(&object).cloned(),
        }
        while let Some(value) = pending.pop() {
            let lacking = self
                .missing
                .get(&value)
                .into_iter()
                .flat_map(BTreeMap::values);
            let own = self.values.get(&value).into_iter();
            for causes in own.chain(self.keys.get(&value)).chain(lacking) {
                found = Some(match found {
                    Some(earlier) => earlier.union(causes.clone()),
                    None => causes.clone(),
                });
            }
            match document.node(value).kind() {
                NodeKind::Array(items) => pending.extend(items),
                NodeKind::Object(members) => {
                    pending.extend(members.values().map(ObjectMember::value));
                }
                _ => {}
            }
        }
        found.map_or(Ok(()), Err)
    }
}

/// Adds `causes` to what `key` holds.
fn join<K: Ord>(held: &mut BTreeMap<K, Causes>, key: K, causes: &Causes) {
    let joined = match held.remove(&key) {
        Some(earlier) => earlier.union(causes.clone()),
        None => causes.clone(),
    };
    held.insert(key, joined);
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use schemars::JsonSchema;
    use serde_json::{json, Value};

    use crate::fixtures::{
        complete_config, minimal_config, FixturePolicyReference, FixtureTargetReference,
    };
    use crate::input_limits::Bound;
    use crate::materialize::{materialize_file, MaterializedDocument, NodeId, NodeKind};
    use crate::materialize_tests::limits as input_limits;
    use crate::resolver::entry::fixtures::FileFixture;
    use crate::resolver::evaluation::{Admitted, Check, Evaluation, IssueKey, Subject};
    use crate::resolver::limits::fixture_limits;
    use crate::resolver::structural_admission::{
        admit_structure, EntryKey, StructuralAdmission, StructuralSubject as S,
    };
    use crate::structural::{AuthoringSchema, StructuralAnalysis, StructuralLimits};

    use super::ValueAdmissions;

    pub(crate) type Analysis = StructuralAnalysis<FixturePolicyReference, FixtureTargetReference>;

    /// Phase 2 over `value`, recording into `evaluation`.
    pub(crate) fn structured_in(
        evaluation: Evaluation,
        value: &Value,
    ) -> (Evaluation, ValueAdmissions, Analysis) {
        structured_as(evaluation, value)
    }

    /// The same under the authoring model of `Policy` and `Target`.
    pub(crate) fn structured_as<Policy: JsonSchema, Target: JsonSchema>(
        mut evaluation: Evaluation,
        value: &Value,
    ) -> (
        Evaluation,
        ValueAdmissions,
        StructuralAnalysis<Policy, Target>,
    ) {
        let bytes: Arc<[u8]> = Arc::from(serde_json::to_vec(value).unwrap());
        let document = materialize_file(Arc::clone(&bytes), input_limits()).unwrap();
        let fixture = FileFixture::new(&fixture_limits(100));
        let limits = StructuralLimits {
            max_profiles: Bound::new(16).unwrap(),
            max_profile_id_bytes: Bound::new(64).unwrap(),
            max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
        };
        let StructuralAdmission { analysis, values } = admit_structure(
            &mut evaluation,
            &fixture.file(bytes),
            document,
            &AuthoringSchema::for_model().unwrap(),
            limits,
            Ok(()),
        )
        .unwrap();
        (evaluation, values, analysis.unwrap())
    }

    /// The declaration `app` of what Phase 2 analyzed.
    fn app(analysis: &Analysis) -> (&MaterializedDocument, NodeId) {
        analysis.declaration_value("app").unwrap()
    }

    /// What structural admission left at the member `name` of `app`.
    fn member(values: &ValueAdmissions, analysis: &Analysis, name: &str) -> Admitted<()> {
        let (document, declaration) = app(analysis);
        values.member(document, declaration, name)
    }

    /// The checks behind `admitted`, by the kind of subject and the check.
    fn checks(admitted: &Admitted<()>) -> Vec<(&'static str, Check)> {
        admitted
            .as_ref()
            .unwrap_err()
            .keys()
            .map(IssueKey::unit)
            .map(|unit| {
                let kind = match &unit.subject {
                    Subject::Structural(S::ProfileField(..)) => "profile",
                    Subject::Structural(S::TargetProfileKey(_, key)) => match key {
                        EntryKey::Identity(_) => "target-key",
                        EntryKey::Content(_) => "target-content-key",
                    },
                    Subject::Structural(S::TargetField(..)) => "target",
                    Subject::Structural(S::GroupField(..)) => "group",
                    subject => panic!("{subject:?} is below no field"),
                };
                (kind, unit.check)
            })
            .collect()
    }

    /// The members of a declaration.
    pub(crate) const FIELDS: [&str; 12] = [
        "projectId",
        "selectionScope",
        "requestedLocales",
        "defaultRequestedLocale",
        "defaultSourceLocale",
        "localeNegotiation",
        "messageFallback",
        "coverage",
        "policies",
        "targetProfiles",
        "deploymentGroups",
        "delivery",
    ];

    /// The fields with a failure at or below them.
    fn held_back(values: &ValueAdmissions, analysis: &Analysis) -> Vec<&'static str> {
        FIELDS
            .into_iter()
            .filter(|name| member(values, analysis, name).is_err())
            .collect()
    }

    /// A configuration structural admission admitted whole holds nothing.
    #[test]
    fn a_configuration_admitted_whole_holds_nothing() {
        for config in [minimal_config(), complete_config()] {
            let (_, values, analysis) = structured_in(Evaluation::default(), &config);
            assert!(values.values.is_empty() && values.keys.is_empty());
            assert!(values.missing.is_empty());
            assert_eq!(held_back(&values, &analysis), Vec::<&str>::new());
        }
    }

    /// A failing array element is kept at the element, not at its siblings or
    /// its array, and holds back the field above it alone.
    #[test]
    fn a_failing_element_is_kept_at_the_element() {
        let mut config = minimal_config();
        config["profiles"]["app"]["requestedLocales"] = json!(["en", 7, "ja"]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let (document, declaration) = app(&analysis);
        let NodeKind::Object(members) = document.node(declaration).kind() else {
            panic!("the declaration is an object");
        };
        let array = members["requestedLocales"].value();
        let NodeKind::Array(items) = document.node(array).kind() else {
            panic!("an array");
        };
        assert_eq!(values.values.keys().collect::<Vec<_>>(), [&items[1]]);
        assert!(values.value(items[1]).is_err());
        assert_eq!(values.value(items[0]), Ok(()));
        assert_eq!(values.value(array), Ok(()));
        assert_eq!(held_back(&values, &analysis), ["requestedLocales"]);
        assert_eq!(
            checks(&member(&values, &analysis, "requestedLocales")),
            [("profile", Check::FieldType)]
        );
    }

    /// A required member the declaration lacks has no value: it holds back
    /// the field of its name alone.
    #[test]
    fn a_missing_member_holds_back_its_name() {
        let mut config = minimal_config();
        config["profiles"]["app"]
            .as_object_mut()
            .unwrap()
            .remove("projectId");
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        assert_eq!(held_back(&values, &analysis), ["projectId"]);
        assert_eq!(
            checks(&member(&values, &analysis, "projectId")),
            [("profile", Check::RequiredField)]
        );
    }

    /// A required member a nested object lacks holds back the field above it.
    #[test]
    fn a_member_a_nested_object_lacks_holds_back_the_field_above_it() {
        let mut config = minimal_config();
        config["profiles"]["app"]["targetProfiles"]["browser"]
            .as_object_mut()
            .unwrap()
            .remove("profile");
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        assert_eq!(held_back(&values, &analysis), ["targetProfiles"]);
        assert_eq!(
            checks(&member(&values, &analysis, "targetProfiles")),
            [("target", Check::RequiredField)]
        );
        // It is kept by the name of the member, the last step of its path.
        let mut config = minimal_config();
        config["profiles"]["app"]["policies"]
            .as_object_mut()
            .unwrap()
            .remove("trust");
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let (document, declaration) = app(&analysis);
        let NodeKind::Object(members) = document.node(declaration).kind() else {
            panic!("the declaration is an object");
        };
        let policies = members["policies"].value();
        assert!(values.member(document, policies, "trust").is_err());
        assert_eq!(values.member(document, policies, "approval"), Ok(()));
    }

    /// An undeclared member is a failure of the object that holds it: below
    /// a field it holds the field back, and in the declaration it belongs to
    /// no field.
    #[test]
    fn an_undeclared_member_is_a_failure_of_its_object() {
        let mut config = minimal_config();
        config["profiles"]["app"]["delivery"] = json!({"extra": true});
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        assert_eq!(held_back(&values, &analysis), ["delivery"]);
        assert_eq!(
            checks(&member(&values, &analysis, "delivery")),
            [("profile", Check::KnownField)]
        );
        let mut config = minimal_config();
        config["profiles"]["app"]["extra"] = json!(true);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        assert_eq!(held_back(&values, &analysis), Vec::<&str>::new());
    }

    /// A key without the identifier syntax is kept apart from the failures
    /// of its entry's value, and a field gathers both.
    #[test]
    fn a_failing_key_joins_the_failures_of_its_entry() {
        let mut config = minimal_config();
        let mut target = config["profiles"]["app"]["targetProfiles"]["browser"].clone();
        target["extra"] = json!(true);
        config["profiles"]["app"]["targetProfiles"]["Web App"] = target;
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        assert_eq!((values.values.len(), values.keys.len()), (1, 1));
        let entry = *values.keys.keys().next().unwrap();
        assert!(values.key(entry).is_err());
        let (document, _) = app(&analysis);
        assert_eq!(values.member(document, entry, "profile"), Ok(()));
        assert_eq!(held_back(&values, &analysis), ["targetProfiles"]);
        assert_eq!(
            checks(&member(&values, &analysis, "targetProfiles")),
            [
                ("target-content-key", Check::FieldValue),
                ("target", Check::KnownField)
            ]
        );
    }

    /// The causes of a field are those of every value below it, through
    /// objects and arrays alike, and of each required member they lack.
    #[test]
    fn a_field_gathers_the_failures_of_every_value_below_it() {
        let mut config = complete_config();
        let groups = &mut config["profiles"]["app"]["deploymentGroups"];
        let group = groups.as_object_mut().unwrap().values_mut().next().unwrap();
        group["hydrationRelations"] =
            json!([{"server": 1}, {"server": "a", "client": "b", "x": 1}]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        assert_eq!(held_back(&values, &analysis), ["deploymentGroups"]);
        assert_eq!(
            checks(&member(&values, &analysis, "deploymentGroups")),
            [
                ("group", Check::KnownField),
                ("group", Check::RequiredField),
                ("group", Check::FieldType)
            ]
        );
    }

    /// A value that is no object holds no member: a member of it waits on
    /// the failure of the value's own type, and on nothing below it.
    #[test]
    fn a_value_that_is_no_object_has_no_member() {
        let mut config = minimal_config();
        config["profiles"]["app"]["requestedLocales"] = json!([7]);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let (document, declaration) = app(&analysis);
        let NodeKind::Object(members) = document.node(declaration).kind() else {
            panic!("the declaration is an object");
        };
        let array = members["requestedLocales"].value();
        assert_eq!(values.member(document, array, "requestedLocales"), Ok(()));
        let mut config = minimal_config();
        config["profiles"]["app"]["targetProfiles"]["browser"] = json!(7);
        let (_, values, analysis) = structured_in(Evaluation::default(), &config);
        let (document, _) = app(&analysis);
        let browser = *values.values.keys().next().unwrap();
        assert_eq!(
            checks(&values.member(document, browser, "profile")),
            [("target", Check::FieldType)]
        );
    }
}
