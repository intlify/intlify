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

use std::collections::BTreeSet;

use crate::input_limits::Bound;
use crate::materialize::{ByteSpan, MaterializedDocument, NodeId, NodeKind};
use crate::model::valid_identity;

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
    /// Only a known required field copied from the generated schema, never an
    /// arbitrary configuration key, selector, or rejected scalar.
    pub(super) missing_field: Option<String>,
    pub(super) alternatives: Vec<SchemaIssue>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FragmentState {
    Admitted,
    Invalid,
    TypeUnavailable,
    ResourceUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct FragmentAdmission {
    pub(super) schema: SchemaId,
    pub(super) subject: NodeId,
    pub(super) state: FragmentState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EvaluationFailure {
    WorkLimit { limit: Bound, actual: u64 },
    AccountingOverflow,
    AccountingMismatch,
}

pub(super) struct SchemaEvaluation {
    pub(super) admitted: bool,
    pub(super) units: u64,
    pub(super) issues: Vec<SchemaIssue>,
    pub(super) fragments: Vec<FragmentAdmission>,
}

pub(super) fn evaluate(
    program: &SchemaProgram,
    doc: &MaterializedDocument,
    schema: SchemaId,
    subject: NodeId,
    max_units: Bound,
) -> Result<SchemaEvaluation, EvaluationFailure> {
    evaluate_with_exclusions(program, doc, schema, subject, max_units, &BTreeSet::new())
}

pub(super) fn evaluate_with_exclusions(
    program: &SchemaProgram,
    doc: &MaterializedDocument,
    schema: SchemaId,
    subject: NodeId,
    max_units: Bound,
    excluded: &BTreeSet<NodeId>,
) -> Result<SchemaEvaluation, EvaluationFailure> {
    // The immutable schema graph is finite/non-cyclic; materialization already
    // bounded the whole value domain. This pass counts the complete applicable
    // domain without constructing issues, fragments, or owned authoring values.
    let mut preflight = Walker::new(program, doc, false, excluded);
    preflight.walk(schema, subject)?;
    let expected = preflight.units;
    if expected > max_units.get() {
        return Err(EvaluationFailure::WorkLimit {
            limit: max_units,
            actual: expected,
        });
    }
    let mut evaluation = Walker::new(program, doc, true, excluded);
    let admitted = evaluation.walk(schema, subject)?;
    if evaluation.units != expected {
        return Err(EvaluationFailure::AccountingMismatch);
    }
    Ok(SchemaEvaluation {
        admitted,
        units: expected,
        issues: evaluation.issues,
        fragments: evaluation.fragments,
    })
}

struct Walker<'input> {
    program: &'input SchemaProgram,
    doc: &'input MaterializedDocument,
    collect: bool,
    units: u64,
    issues: Vec<SchemaIssue>,
    fragments: Vec<FragmentAdmission>,
    excluded: &'input BTreeSet<NodeId>,
}

impl<'input> Walker<'input> {
    fn new(
        program: &'input SchemaProgram,
        doc: &'input MaterializedDocument,
        collect: bool,
        excluded: &'input BTreeSet<NodeId>,
    ) -> Self {
        Self {
            program,
            doc,
            collect,
            units: 0,
            issues: Vec::new(),
            fragments: Vec::new(),
            excluded,
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

    fn walk(&mut self, id: SchemaId, subject: NodeId) -> Result<bool, EvaluationFailure> {
        if self.excluded.contains(&subject) {
            self.fragment(id, subject, FragmentState::ResourceUnavailable);
            return Ok(false);
        }
        let program = self.program;
        let doc = self.doc;
        let schema = program.node(id);
        let value = doc.node(subject).kind();
        if let Some(reference) = schema.reference {
            self.unit()?;
            let admitted = self.walk(reference, subject)?;
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
            if !type_matches(expected, value) {
                self.value_issue(IssueKind::TypeInvalid, id, subject);
                self.fragment(id, subject, FragmentState::TypeUnavailable);
                return Ok(false);
            }
        }
        let mut admitted = true;
        if let Some(allowed) = &schema.enum_strings {
            self.unit()?;
            if self.collect && !matches!(value, NodeKind::String(text) if allowed.contains(text)) {
                self.value_issue(IssueKind::ValueInvalid, id, subject);
                admitted = false;
            }
        }
        if let (Some(pattern), NodeKind::String(text)) = (schema.string_pattern, value) {
            self.unit()?;
            if self.collect && !pattern.matches(text) {
                self.value_issue(IssueKind::ValueInvalid, id, subject);
                admitted = false;
            }
        }
        match value {
            NodeKind::Array(items) => {
                if let Some(minimum) = schema.min_items {
                    self.unit()?;
                    if self.collect
                        && u64::try_from(items.len()).expect("bounded array length") < minimum
                    {
                        self.value_issue(IssueKind::ValueInvalid, id, subject);
                        admitted = false;
                    }
                }
                if let Some(child) = schema.items {
                    self.unit()?;
                    for &item in items {
                        admitted &= self.walk(child, item)?;
                    }
                }
            }
            NodeKind::Object(_) => admitted &= self.object(schema, id, subject)?,
            _ => {}
        }
        if let Some(branches) = &schema.any_of {
            self.unit()?;
            let before = self.issues.len();
            let mut matched = false;
            for &branch in branches {
                matched |= self.walk(branch, subject)?;
            }
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

    fn object(
        &mut self,
        schema: &SchemaNode,
        id: SchemaId,
        subject: NodeId,
    ) -> Result<bool, EvaluationFailure> {
        let doc = self.doc;
        let NodeKind::Object(members) = doc.node(subject).kind() else {
            unreachable!()
        };
        let mut admitted = true;
        if let Some(minimum) = schema.min_properties {
            self.unit()?;
            if self.collect
                && u64::try_from(members.len()).expect("bounded object length") < minimum
            {
                self.value_issue(IssueKind::ValueInvalid, id, subject);
                admitted = false;
            }
        }
        if let Some(required) = &schema.required {
            self.unit()?;
            if self.collect {
                for name in required {
                    if !members.contains_key(name) {
                        self.issue(
                            IssueKind::RequiredFieldMissing,
                            id,
                            subject,
                            doc.node(subject).span(),
                            LocationRole::MissingField,
                            Some(name),
                        );
                        admitted = false;
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
        // Materialized object iteration is unsigned UTF-8 key order. The value
        // tree's physical node IDs and source member order never choose checks.
        for (name, member) in members {
            // Admission has independently proved this member or domain exceeds
            // a bound. Do not scan its key pattern or inspect its descendants.
            if self.excluded.contains(&member.value()) {
                self.fragment(id, member.value(), FragmentState::ResourceUnavailable);
                admitted = false;
                continue;
            }
            let mut matched = false;
            if let Some(child) = schema.properties.as_ref().and_then(|known| known.get(name)) {
                admitted &= self.walk(*child, member.value())?;
                matched = true;
            }
            if let Some(child) = schema.identity_values.filter(|_| valid_identity(name)) {
                admitted &= self.walk(child, member.value())?;
                matched = true;
            }
            if !matched {
                match schema.additional {
                    Some(Additional::Schema(child)) => {
                        admitted &= self.walk(child, member.value())?;
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
                    }
                    None => {}
                }
            }
        }
        Ok(admitted)
    }
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
