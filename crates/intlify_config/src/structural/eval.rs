// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Dependency-aware evaluation of the generated owned-schema subset.
//!
//! One unit is one applicable schema-keyword occurrence at one logical subject.
//! Annotation keywords are excluded. A type mismatch suppresses that schema's
//! dependent constraints/descendants; it never suppresses independent siblings.
//! All anyOf branches are visited, so finding a match does not change accounting.
//! This is an internal schema operation, not configuration-version admission or
//! final Finding/Evidence construction. Consumers cannot obtain a checked Profile.
//!
//! The evaluation also records a candidate for each applicable evaluation of a
//! structural check (D15:2173), keyed by the identity map entry that scopes it
//! and its logical field path from that entry (D15:2184). An `anyOf` is one
//! closed-value evaluation of its value: what its branches evaluate records no
//! candidate. The value of an entry whose key lacks the identifier syntax is
//! evaluated as well, under that entry (D15:2184, 2190).

use crate::input_limits::Bound;
use crate::materialize::{ByteSpan, MaterializedDocument, NodeId, NodeKind, ObjectMember};
use crate::model::valid_identity;

use super::facts::{
    At, Candidate, CandidateId, Check, Entry, Facts, FieldPathId, FieldStep, Level, Outcome,
    ScopeId,
};
use super::program::{Additional, SchemaId, SchemaNode, SchemaProgram, ValueType};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum IssueKind {
    TypeInvalid,
    RequiredFieldMissing,
    ValueInvalid,
    UnknownField,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum LocationRole {
    Value,
    MemberKey,
    MissingField,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SchemaIssue {
    pub(super) kind: IssueKind,
    pub(super) schema: SchemaId,
    pub(super) subject: NodeId,
    pub(super) span: ByteSpan,
    pub(super) role: LocationRole,
    /// Only a known required field copied from the compiled schema, never an
    /// arbitrary configuration key, selector, or rejected scalar.
    pub(super) missing_field: Option<String>,
    pub(super) alternatives: Vec<SchemaIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FragmentState {
    Admitted,
    Invalid,
    TypeUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FragmentAdmission {
    pub(super) schema: SchemaId,
    pub(super) subject: NodeId,
    pub(super) state: FragmentState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvaluationFailure {
    WorkLimit {
        limit: Bound,
        actual: u64,
    },
    AccountingOverflow,
    AccountingMismatch,
    /// An identity map outside the positions of 015's scope levels.
    ScopeLayout,
}

pub(super) struct SchemaEvaluation {
    pub(super) admitted: bool,
    pub(super) units: u64,
    pub(super) issues: Vec<SchemaIssue>,
    pub(super) fragments: Vec<FragmentAdmission>,
    pub(super) facts: Facts,
}

pub(super) fn evaluate(
    program: &SchemaProgram,
    doc: &MaterializedDocument,
    schema: SchemaId,
    subject: NodeId,
    max_units: Bound,
) -> Result<SchemaEvaluation, EvaluationFailure> {
    // The immutable schema graph is finite/non-cyclic; materialization already
    // bounded the whole value domain. This pass counts the complete applicable
    // domain without constructing issues, fragments, or owned authoring values.
    let mut preflight = Walker::new(program, doc, false);
    preflight.walk(schema, subject, Position::ROOT)?;
    let expected = preflight.units;
    if expected > max_units.get() {
        return Err(EvaluationFailure::WorkLimit {
            limit: max_units,
            actual: expected,
        });
    }
    let mut evaluation = Walker::new(program, doc, true);
    let admitted = evaluation.walk(schema, subject, Position::ROOT)?;
    if evaluation.units != expected {
        return Err(EvaluationFailure::AccountingMismatch);
    }
    Ok(SchemaEvaluation {
        admitted,
        units: expected,
        issues: evaluation.issues,
        fragments: evaluation.fragments,
        facts: evaluation.facts,
    })
}

/// Where the walk is: the scope whose entry a path starts from, the path,
/// and the type check of the value that contains the position, which an
/// evaluation there needs; none at the root.
#[derive(Debug, Clone, Copy)]
struct Position {
    scope: ScopeId,
    path: Option<FieldPathId>,
    typed: Option<CandidateId>,
}

impl Position {
    const ROOT: Self = Self {
        scope: Facts::ROOT,
        path: None,
        typed: None,
    };
}

struct Walker<'input> {
    program: &'input SchemaProgram,
    doc: &'input MaterializedDocument,
    collect: bool,
    units: u64,
    issues: Vec<SchemaIssue>,
    fragments: Vec<FragmentAdmission>,
    facts: Facts,
    /// How many `anyOf` branches the walk is inside.
    branches: usize,
}

impl<'input> Walker<'input> {
    fn new(
        program: &'input SchemaProgram,
        doc: &'input MaterializedDocument,
        collect: bool,
    ) -> Self {
        Self {
            program,
            doc,
            collect,
            units: 0,
            issues: Vec::new(),
            fragments: Vec::new(),
            facts: Facts::new(),
            branches: 0,
        }
    }

    fn unit(&mut self) -> Result<(), EvaluationFailure> {
        self.units = self
            .units
            .checked_add(1)
            .ok_or(EvaluationFailure::AccountingOverflow)?;
        Ok(())
    }

    fn issue(
        &mut self,
        kind: IssueKind,
        schema: SchemaId,
        subject: NodeId,
        span: ByteSpan,
        role: LocationRole,
        missing_field: Option<&str>,
    ) {
        if self.collect {
            self.issues.push(SchemaIssue {
                kind,
                schema,
                subject,
                span,
                role,
                missing_field: missing_field.map(str::to_owned),
                alternatives: Vec::new(),
            });
        }
    }

    fn value_issue(&mut self, kind: IssueKind, schema: SchemaId, subject: NodeId) {
        self.issue(
            kind,
            schema,
            subject,
            self.doc.node(subject).span(),
            LocationRole::Value,
            None,
        );
    }

    fn fragment(&mut self, schema: SchemaId, subject: NodeId, state: FragmentState) {
        if self.collect {
            self.fragments.push(FragmentAdmission {
                schema,
                subject,
                state,
            });
        }
    }

    /// Whether this walk records candidates: the collecting walk, outside
    /// every `anyOf` branch.
    const fn recording(&self) -> bool {
        self.collect && self.branches == 0
    }

    /// Records a candidate that needs the type check `position` names, and
    /// gives its ID when this walk records candidates.
    fn record(
        &mut self,
        check: Check,
        position: Position,
        at: At,
        outcome: Outcome,
    ) -> Option<CandidateId> {
        self.recording().then(|| {
            self.facts.push(Candidate {
                check,
                scope: position.scope,
                path: position.path,
                at,
                outcome,
                after: position.typed,
            })
        })
    }

    /// The position of the member `name` the schema defines.
    fn member(&mut self, position: Position, name: &str) -> Position {
        self.step(position, || FieldStep::Member(name.into()))
    }

    /// The position of an array element, or of an entry of a map whose keys
    /// are not identities.
    fn any(&mut self, position: Position) -> Position {
        self.step(position, || FieldStep::Any)
    }

    fn step(&mut self, position: Position, step: impl FnOnce() -> FieldStep) -> Position {
        if !self.recording() {
            return position;
        }
        Position {
            path: Some(self.facts.enter(position.path, step())),
            ..position
        }
    }

    /// The level of the entries of the identity map at `position`: 015's
    /// scope levels are the root, an entry of its `profiles`, and an entry of
    /// a declaration's `targetProfiles` or `deploymentGroups` (D15:2173).
    fn level_below(&self, position: Position) -> Result<Level, EvaluationFailure> {
        let level = self.facts.scope(position.scope).level;
        let steps = self.facts.steps(position.path);
        let [FieldStep::Member(name)] = steps.as_slice() else {
            return Err(EvaluationFailure::ScopeLayout);
        };
        match (level, &**name) {
            (Level::Root, "profiles") => Ok(Level::Profile),
            (Level::Profile, "targetProfiles") => Ok(Level::Target),
            (Level::Profile, "deploymentGroups") => Ok(Level::Group),
            _ => Err(EvaluationFailure::ScopeLayout),
        }
    }

    /// Records the key of an identity map entry, whose map is at `position`,
    /// and gives the position of the entry's value: the empty path under the
    /// entry (D15:2184).
    fn entry(
        &mut self,
        position: Position,
        level: Option<Level>,
        map: NodeId,
        member: &ObjectMember,
        admitted: bool,
    ) -> Position {
        let Some(level) = level else {
            return position;
        };
        let scope = self.facts.enter_scope(
            level,
            position.scope,
            Entry {
                map,
                value: member.value(),
                key: member.key_span(),
                admitted,
            },
        );
        let key = self.any(position);
        self.record(Check::Key, key, At::Key(scope), Outcome::of(admitted));
        Position {
            scope,
            path: None,
            ..position
        }
    }

    fn walk(
        &mut self,
        id: SchemaId,
        subject: NodeId,
        mut position: Position,
    ) -> Result<bool, EvaluationFailure> {
        let program = self.program;
        let doc = self.doc;
        let schema = program.node(id);
        let value = doc.node(subject).kind();
        if let Some(reference) = schema.reference {
            self.unit()?;
            let admitted = self.walk(reference, subject, position)?;
            self.fragment(
                id,
                subject,
                if admitted {
                    FragmentState::Admitted
                } else {
                    FragmentState::Invalid
                },
            );
            return Ok(admitted);
        }
        if let Some(expected) = schema.value_type {
            self.unit()?;
            let matches = type_matches(expected, value);
            // The value's other checks, and its members, need its type.
            if let Some(typed) = self.record(
                Check::Type,
                position,
                At::Value(subject),
                Outcome::of(matches),
            ) {
                position.typed = Some(typed);
            }
            if !matches {
                self.value_issue(IssueKind::TypeInvalid, id, subject);
                self.fragment(id, subject, FragmentState::TypeUnavailable);
                // The checks of the same path that need the type leave this
                // value to the type check (D15:2184).
                if constrains_value(schema) {
                    self.record(
                        Check::Value,
                        position,
                        At::Value(subject),
                        Outcome::Excluded,
                    );
                }
                if closes_members(schema) {
                    self.record(
                        Check::Known,
                        position,
                        At::Value(subject),
                        Outcome::Excluded,
                    );
                }
                return Ok(false);
            }
        }
        let mut admitted = true;
        // Whether every closed-value constraint that applied passed; none when
        // none applied.
        let mut constrained = None;
        if let Some(allowed) = &schema.enum_strings {
            self.unit()?;
            let passed =
                !self.collect || matches!(value, NodeKind::String(text) if allowed.contains(text));
            if !passed {
                self.value_issue(IssueKind::ValueInvalid, id, subject);
                admitted = false;
            }
            meet(&mut constrained, passed);
        }
        if let (Some(pattern), NodeKind::String(text)) = (schema.string_pattern, value) {
            self.unit()?;
            let passed = !self.collect || pattern.matches(text);
            if !passed {
                self.value_issue(IssueKind::ValueInvalid, id, subject);
                admitted = false;
            }
            meet(&mut constrained, passed);
        }
        match value {
            NodeKind::Array(items) => {
                if let Some(minimum) = schema.min_items {
                    self.unit()?;
                    let passed = !self.collect
                        || u64::try_from(items.len()).expect("bounded array length") >= minimum;
                    if !passed {
                        self.value_issue(IssueKind::ValueInvalid, id, subject);
                        admitted = false;
                    }
                    meet(&mut constrained, passed);
                }
                if let Some(child) = schema.items {
                    self.unit()?;
                    let element = self.any(position);
                    for &item in items {
                        admitted &= self.walk(child, item, element)?;
                    }
                }
            }
            NodeKind::Object(_) => {
                let (members_admitted, minimum) = self.object(schema, id, subject, position)?;
                admitted &= members_admitted;
                if let Some(passed) = minimum {
                    meet(&mut constrained, passed);
                }
            }
            _ => {}
        }
        if let Some(branches) = &schema.any_of {
            self.unit()?;
            let before = self.issues.len();
            let mut matched = false;
            self.branches += 1;
            for &branch in branches {
                matched |= self.walk(branch, subject, position)?;
            }
            self.branches -= 1;
            if self.collect {
                if matched {
                    self.issues.truncate(before);
                } else {
                    let alternatives = self.issues.split_off(before);
                    self.value_issue(IssueKind::ValueInvalid, id, subject);
                    self.issues
                        .last_mut()
                        .expect("recorded alternative failure")
                        .alternatives = alternatives;
                }
            }
            admitted &= matched;
            meet(&mut constrained, matched);
        }
        if let Some(passed) = constrained {
            self.record(
                Check::Value,
                position,
                At::Value(subject),
                Outcome::of(passed),
            );
        }
        self.fragment(
            id,
            subject,
            if admitted {
                FragmentState::Admitted
            } else {
                FragmentState::Invalid
            },
        );
        Ok(admitted)
    }

    /// The object's own constraints and members: whether they are admitted,
    /// and whether its minimum member count, when it has one, is met.
    fn object(
        &mut self,
        schema: &SchemaNode,
        id: SchemaId,
        subject: NodeId,
        position: Position,
    ) -> Result<(bool, Option<bool>), EvaluationFailure> {
        let doc = self.doc;
        let NodeKind::Object(members) = doc.node(subject).kind() else {
            unreachable!()
        };
        let mut admitted = true;
        let mut minimum_met = None;
        if let Some(minimum) = schema.min_properties {
            self.unit()?;
            let passed = !self.collect
                || u64::try_from(members.len()).expect("bounded object length") >= minimum;
            if !passed {
                self.value_issue(IssueKind::ValueInvalid, id, subject);
                admitted = false;
            }
            minimum_met = Some(passed);
        }
        if let Some(required) = &schema.required {
            self.unit()?;
            if self.collect {
                for name in required {
                    let field = self.member(position, name);
                    if let Some(member) = members.get(name) {
                        self.record(
                            Check::Required,
                            field,
                            At::Value(member.value()),
                            Outcome::Passed,
                        );
                    } else {
                        self.issue(
                            IssueKind::RequiredFieldMissing,
                            id,
                            subject,
                            doc.node(subject).span(),
                            LocationRole::MissingField,
                            Some(name),
                        );
                        admitted = false;
                        self.record(
                            Check::Required,
                            field,
                            At::Missing { object: subject },
                            Outcome::Failed,
                        );
                    }
                }
            }
        }
        if schema.properties.is_some() {
            self.unit()?;
        }
        if schema.identity_values.is_some() {
            self.unit()?;
        }
        if schema.additional.is_some() {
            self.unit()?;
        }
        // The scope level of the entries, for an identity map.
        let level = match schema.identity_values {
            Some(_) if self.recording() => Some(self.level_below(position)?),
            _ => None,
        };
        let mut undeclared = false;
        // Materialized object iteration is unsigned UTF-8 key order. The value
        // tree's physical node IDs and source member order never choose checks.
        for (name, member) in members {
            let mut matched = false;
            if let Some(child) = schema.properties.as_ref().and_then(|known| known.get(name)) {
                let field = self.member(position, name);
                admitted &= self.walk(*child, member.value(), field)?;
                matched = true;
            }
            if let Some(child) = schema.identity_values.filter(|_| valid_identity(name)) {
                let entry = self.entry(position, level, subject, member, true);
                admitted &= self.walk(child, member.value(), entry)?;
                matched = true;
            }
            if !matched {
                match schema.additional {
                    Some(Additional::Schema(child)) => {
                        let entry = self.any(position);
                        admitted &= self.walk(child, member.value(), entry)?;
                    }
                    Some(Additional::Forbidden) => {
                        let kind = if schema.identity_values.is_some() {
                            IssueKind::ValueInvalid
                        } else {
                            IssueKind::UnknownField
                        };
                        self.issue(
                            kind,
                            id,
                            subject,
                            member.key_span(),
                            LocationRole::MemberKey,
                            None,
                        );
                        admitted = false;
                        if let Some(child) = schema.identity_values {
                            // A key without the identifier syntax still
                            // scopes its value, which structural admission
                            // evaluates, while later phases have no subject
                            // for the entry (D15:2184, 2190).
                            let entry = self.entry(position, level, subject, member, false);
                            self.walk(child, member.value(), entry)?;
                        } else {
                            self.record(
                                Check::Known,
                                position,
                                At::Member {
                                    object: subject,
                                    key: member.key_span(),
                                },
                                Outcome::Failed,
                            );
                            undeclared = true;
                        }
                    }
                    None => {}
                }
            }
        }
        if closes_members(schema) && !undeclared {
            self.record(Check::Known, position, At::Value(subject), Outcome::Passed);
        }
        Ok((admitted, minimum_met))
    }
}

/// Folds one closed-value constraint's result into the value's.
fn meet(constrained: &mut Option<bool>, passed: bool) {
    *constrained = Some(constrained.unwrap_or(true) && passed);
}

/// Whether `schema` has a closed-value constraint: an enumeration, a pattern,
/// a minimum item or member count, or a set of alternatives.
const fn constrains_value(schema: &SchemaNode) -> bool {
    schema.enum_strings.is_some()
        || schema.string_pattern.is_some()
        || schema.min_items.is_some()
        || schema.min_properties.is_some()
        || schema.any_of.is_some()
}

/// Whether `schema` closes an object's members, other than an identity map,
/// whose closedness is its keys' check.
const fn closes_members(schema: &SchemaNode) -> bool {
    schema.identity_values.is_none() && matches!(schema.additional, Some(Additional::Forbidden))
}

fn type_matches(expected: ValueType, value: &NodeKind) -> bool {
    match (expected, value) {
        (ValueType::Null, NodeKind::Null)
        | (ValueType::Boolean, NodeKind::Boolean(_))
        | (ValueType::Number, NodeKind::Number(_))
        | (ValueType::String, NodeKind::String(_))
        | (ValueType::Array, NodeKind::Array(_))
        | (ValueType::Object, NodeKind::Object(_)) => true,
        (ValueType::Integer, NodeKind::Number(number)) => number.get().fract() == 0.0,
        _ => false,
    }
}
