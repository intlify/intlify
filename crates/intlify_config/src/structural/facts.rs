// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What structural admission found: one candidate for each applicable
//! evaluation of a structural check (D15:2173), with the entry that scopes
//! its subject, the subject's logical field path from that entry, where the
//! check looked, and its outcome. The Check Registry's units and Findings of
//! structural admission are built from them (D15:2184).
//!
//! A candidate holds nodes, spans, and the names the configuration schema
//! defines, never a name or a value from the configuration.

use std::collections::BTreeMap;

use crate::materialize::{ByteSpan, NodeId};

/// A closed schema keyword class of structural admission, in the order of a
/// scope level's rows, and the key of an identity map (D15:2173).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Check {
    /// A required member is present.
    Required,
    /// A value has its schema type.
    Type,
    /// A value meets its closed constraints: an enumeration, a pattern, a
    /// minimum item or member count, or a set of alternatives.
    Value,
    /// A closed object has no undeclared member.
    Known,
    /// A key of an identity map has the identifier syntax.
    Key,
}

/// A scope level of structural admission (D15:2173).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Level {
    /// The configuration root.
    Root,
    /// An entry of the root's `profiles`.
    Profile,
    /// An entry of a profile declaration's `targetProfiles`.
    Target,
    /// An entry of a profile declaration's `deploymentGroups`.
    Group,
}

/// A scope, by its position in [`Facts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct ScopeId(usize);

/// An identity map entry, which scopes the fields of its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Entry {
    /// The identity map that holds the entry.
    pub(crate) map: NodeId,
    /// The entry's value.
    pub(crate) value: NodeId,
    /// Where the entry's key is.
    pub(crate) key: ByteSpan,
    /// Whether the key has the identifier syntax.
    pub(crate) admitted: bool,
}

/// The root, or an identity map entry under its parent scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Scope {
    pub(crate) level: Level,
    /// The scope the entry's map is in; none for the root.
    pub(crate) parent: Option<ScopeId>,
    /// None for the root.
    pub(crate) entry: Option<Entry>,
}

/// One step of a logical field path.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum FieldStep {
    /// A member the schema names.
    Member(Box<str>),
    /// An array element, or an entry of a map whose keys are not identities.
    Any,
}

impl FieldStep {
    /// The step as a `logical-field` path spells it: the member's name, or
    /// `*` (D15:2184).
    pub(crate) fn name(&self) -> &str {
        match self {
            Self::Member(name) => name,
            Self::Any => "*",
        }
    }
}

/// A non-empty logical field path, by its last step in [`Facts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct FieldPathId(usize);

/// Where a candidate looked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum At {
    /// A value.
    Value(NodeId),
    /// An undeclared member of an object, by its key.
    Member { object: NodeId, key: ByteSpan },
    /// A required member that the object lacks, which the path names.
    Missing { object: NodeId },
    /// The key of an identity map entry, which the scope holds.
    Key(ScopeId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Outcome {
    Passed,
    Failed,
    /// The value's type did not match, so this check of the same path did
    /// not evaluate it (D15:2184).
    Excluded,
}

impl Outcome {
    pub(crate) const fn of(passed: bool) -> Self {
        if passed {
            Self::Passed
        } else {
            Self::Failed
        }
    }
}

/// A candidate, by its position in [`Facts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct CandidateId(usize);

/// One applicable evaluation of a check: its subject is the path under the
/// scope's entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Candidate {
    pub(crate) check: Check,
    pub(crate) scope: ScopeId,
    /// None for the empty path.
    pub(crate) path: Option<FieldPathId>,
    pub(crate) at: At,
    pub(crate) outcome: Outcome,
    /// The type check this evaluation needs: the containing value's for a
    /// type, the value's own, or else the containing value's, for a closed
    /// value, a known member, or an excluded check, the object's for a
    /// required member, and the map's for a key. None for the root's type,
    /// which the analysis's measure admits (D15:2149, 2184).
    pub(crate) after: Option<CandidateId>,
}

/// The candidates of one analysis, with the scopes and paths they name.
/// A path is entered once and shared by every candidate under it.
#[derive(Debug)]
pub(crate) struct Facts {
    scopes: Vec<Scope>,
    steps: Vec<(Option<FieldPathId>, FieldStep)>,
    entered: BTreeMap<(Option<FieldPathId>, FieldStep), FieldPathId>,
    candidates: Vec<Candidate>,
}

impl Facts {
    /// The root's scope.
    pub(crate) const ROOT: ScopeId = ScopeId(0);

    pub(crate) fn new() -> Self {
        Self {
            scopes: vec![Scope {
                level: Level::Root,
                parent: None,
                entry: None,
            }],
            steps: Vec::new(),
            entered: BTreeMap::new(),
            candidates: Vec::new(),
        }
    }

    /// A scope for `entry`, at `level` under `parent`.
    pub(crate) fn enter_scope(&mut self, level: Level, parent: ScopeId, entry: Entry) -> ScopeId {
        self.scopes.push(Scope {
            level,
            parent: Some(parent),
            entry: Some(entry),
        });
        ScopeId(self.scopes.len() - 1)
    }

    /// The path `step` below `parent`, entered once.
    pub(crate) fn enter(&mut self, parent: Option<FieldPathId>, step: FieldStep) -> FieldPathId {
        if let Some(&path) = self.entered.get(&(parent, step.clone())) {
            return path;
        }
        let path = FieldPathId(self.steps.len());
        self.steps.push((parent, step.clone()));
        self.entered.insert((parent, step), path);
        path
    }

    pub(crate) fn push(&mut self, candidate: Candidate) -> CandidateId {
        self.candidates.push(candidate);
        CandidateId(self.candidates.len() - 1)
    }

    pub(crate) fn candidates(&self) -> &[Candidate] {
        &self.candidates
    }

    /// Each candidate with its ID, in the order the walk recorded them: a
    /// candidate comes after the one it needs.
    pub(crate) fn identified(&self) -> impl Iterator<Item = (CandidateId, &Candidate)> {
        self.candidates
            .iter()
            .enumerate()
            .map(|(id, candidate)| (CandidateId(id), candidate))
    }

    pub(crate) fn scope(&self, id: ScopeId) -> Scope {
        self.scopes[id.0]
    }

    /// The steps of `path`, from its scope's entry.
    pub(crate) fn steps(&self, path: Option<FieldPathId>) -> Vec<&FieldStep> {
        let mut steps = Vec::new();
        let mut next = path;
        while let Some(FieldPathId(id)) = next {
            let (parent, step) = &self.steps[id];
            steps.push(step);
            next = *parent;
        }
        steps.reverse();
        steps
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::sync::Arc;

    use super::{At, Check, Entry, Facts, FieldStep, Level, Outcome};
    use crate::materialize::{materialize_file, ByteSpan, NodeId, NodeKind};
    use crate::materialize_tests::limits;

    /// The nodes of the elements of a three-element array.
    fn nodes() -> Vec<NodeId> {
        let doc = materialize_file(Arc::from(&b"[0,1,2]"[..]), limits()).unwrap();
        let NodeKind::Array(items) = doc.node(doc.root()).kind() else {
            unreachable!("an array")
        };
        items.clone()
    }

    /// A path's steps as text, `*` for any element or entry.
    pub(crate) fn names(facts: &Facts, path: Option<super::FieldPathId>) -> Vec<String> {
        facts
            .steps(path)
            .into_iter()
            .map(|step| step.name().to_owned())
            .collect()
    }

    #[test]
    fn a_path_is_entered_once_and_lists_its_steps_from_the_entry() {
        let mut facts = Facts::new();
        let policies = facts.enter(None, FieldStep::Member("policies".into()));
        let trust = facts.enter(Some(policies), FieldStep::Member("trust".into()));
        let any = facts.enter(Some(policies), FieldStep::Any);
        assert_eq!(
            facts.enter(None, FieldStep::Member("policies".into())),
            policies
        );
        assert_eq!(
            facts.enter(Some(policies), FieldStep::Member("trust".into())),
            trust
        );
        assert_ne!(trust, any);
        assert_eq!(names(&facts, Some(trust)), ["policies", "trust"]);
        assert_eq!(names(&facts, Some(any)), ["policies", "*"]);
        assert!(names(&facts, None).is_empty());
        let other = facts.enter(None, FieldStep::Member("trust".into()));
        assert_ne!(other, trust, "the same name under another parent");
        assert_eq!(names(&facts, Some(other)), ["trust"]);
    }

    #[test]
    fn an_entry_scope_names_its_parent_and_the_root_has_none() {
        let mut facts = Facts::new();
        assert_eq!(facts.scope(Facts::ROOT).level, Level::Root);
        assert_eq!(facts.scope(Facts::ROOT).parent, None);
        assert_eq!(facts.scope(Facts::ROOT).entry, None);
        let nodes = nodes();
        let entry = |value: usize| Entry {
            map: nodes[0],
            value: nodes[value],
            key: ByteSpan::new(2, 7),
            admitted: true,
        };
        let profile = facts.enter_scope(Level::Profile, Facts::ROOT, entry(1));
        let target = facts.enter_scope(Level::Target, profile, entry(2));
        assert_eq!(facts.scope(profile).parent, Some(Facts::ROOT));
        assert_eq!(facts.scope(target).parent, Some(profile));
        assert_eq!(facts.scope(target).level, Level::Target);
        assert_eq!(facts.scope(target).entry, Some(entry(2)));
        assert_ne!(facts.scope(profile).entry, facts.scope(target).entry);
        let first = facts.push(super::Candidate {
            check: Check::Key,
            scope: profile,
            path: None,
            at: At::Key(target),
            outcome: Outcome::of(false),
            after: None,
        });
        let second = facts.push(super::Candidate {
            after: Some(first),
            ..facts.candidates()[0]
        });
        assert_eq!(facts.candidates().len(), 2);
        assert_eq!(
            facts.identified().map(|(id, _)| id).collect::<Vec<_>>(),
            [first, second]
        );
        assert_eq!(facts.candidates()[1].after, Some(first));
        assert_eq!(facts.candidates()[0].outcome, Outcome::Failed);
        assert_eq!(Outcome::of(true), Outcome::Passed);
    }
}
