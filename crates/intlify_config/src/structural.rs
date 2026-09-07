// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private schema-guided admission for the minimum profile path. Test-owned
//! reference schemas are not formal 017 artifacts or construction authority.

use std::collections::BTreeSet;
use std::marker::PhantomData;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::de::DeserializeOwned;

use crate::input_limits::Bound;
use crate::materialize::{ByteSpan, MaterializedDocument, NodeId, NodeKind};
use crate::model::{
    valid_identity, IntlifyConfig, ProfileDeclaration, CONFIGURATION_SCHEMA_VERSION,
};
use eval::{EvaluationFailure, FragmentState, SchemaEvaluation};
use program::{ProgramError, SchemaId, SchemaProgram};

mod eval;
mod program;
pub(crate) mod selection;

/// Explicit bounds for this stage, not a default or a formal capability body.
#[derive(Debug, Clone, Copy)]
#[expect(clippy::struct_field_names, reason = "mirror the 015 bound IDs")]
pub(crate) struct StructuralLimits {
    pub(crate) max_profiles: Bound,
    pub(crate) max_profile_id_bytes: Bound,
    pub(crate) max_structural_analysis_units: Bound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StructuralFailure {
    RootTypeInvalid,
    SchemaVersionMissing,
    SchemaVersionTypeInvalid,
    SchemaVersionUnsupported,
    ProfilesLimit { limit: Bound, actual: u64 },
    ProfileIdLimit { limit: Bound, actual: u64 },
    StructuralWorkLimit { limit: Bound, actual: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdmissionIssue {
    pub(crate) reason: StructuralFailure,
    pub(crate) span: ByteSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionInvariant {
    SchemaGeneration,
    SchemaCompilation,
    ModelLayout,
    WorkAccounting,
    AuthoringTypeMismatch,
}

struct BoundSchema {
    body: serde_json::Value,
    program: SchemaProgram,
    declaration_schema: SchemaId,
}

/// A type-bound generated schema for the internal minimum path. Only test-owned
/// reference types instantiate it today. Not a formal Schema Authority Set/RCI.
pub(crate) struct AuthoringSchema<Policy, Target> {
    binding: Arc<BoundSchema>,
    model: PhantomData<fn() -> (Policy, Target)>,
}

impl<Policy, Target> Clone for AuthoringSchema<Policy, Target> {
    fn clone(&self) -> Self {
        Self {
            binding: Arc::clone(&self.binding),
            model: PhantomData,
        }
    }
}

impl<Policy: JsonSchema, Target: JsonSchema> AuthoringSchema<Policy, Target> {
    pub(crate) fn for_model() -> Result<Self, AdmissionInvariant> {
        let body = crate::schema::draft7_schema::<IntlifyConfig<Policy, Target>>()
            .map_err(|_| AdmissionInvariant::SchemaGeneration)?;
        let program = SchemaProgram::compile(&body)
            .map_err(|_: ProgramError| AdmissionInvariant::SchemaCompilation)?;
        let profiles_schema = program
            .property(program.root(), "profiles")
            .ok_or(AdmissionInvariant::ModelLayout)?;
        let declaration_schema = program
            .identity_values(profiles_schema)
            .ok_or(AdmissionInvariant::ModelLayout)?;
        Ok(Self {
            binding: Arc::new(BoundSchema {
                body,
                program,
                declaration_schema,
            }),
            model: PhantomData,
        })
    }
}

impl<Policy, Target> AuthoringSchema<Policy, Target> {
    pub(crate) fn schema_body(&self) -> &serde_json::Value {
        &self.binding.body
    }

    pub(crate) fn analyze(
        &self,
        doc: Arc<MaterializedDocument>,
        limits: StructuralLimits,
    ) -> Result<StructuralAnalysis<Policy, Target>, AdmissionInvariant> {
        let mut result = StructuralAnalysis {
            doc,
            binding: Arc::clone(&self.binding),
            limits,
            issues: Vec::new(),
            selected_version: false,
            profiles: None,
            excluded: BTreeSet::new(),
            evaluation: None,
            model: PhantomData,
        };
        result.admit_version();
        if !result.selected_version {
            return Ok(result);
        }
        result.preflight_profiles();
        let program = &self.binding.program;
        match eval::evaluate_with_exclusions(
            program,
            &result.doc,
            program.root(),
            result.doc.root(),
            limits.max_structural_analysis_units,
            &result.excluded,
        ) {
            Ok(evaluation) => result.evaluation = Some(evaluation),
            Err(EvaluationFailure::WorkLimit { limit, actual }) => {
                result.issues.push(AdmissionIssue {
                    reason: StructuralFailure::StructuralWorkLimit { limit, actual },
                    span: result.doc.node(result.doc.root()).span(),
                });
            }
            Err(EvaluationFailure::AccountingOverflow | EvaluationFailure::AccountingMismatch) => {
                return Err(AdmissionInvariant::WorkAccounting)
            }
        }
        Ok(result)
    }
}

/// Owns both input and schema binding. Fragment indices cannot outlive or borrow
/// another invocation's tree, and no mutable workspace is retained.
pub(crate) struct StructuralAnalysis<Policy, Target> {
    doc: Arc<MaterializedDocument>,
    binding: Arc<BoundSchema>,
    limits: StructuralLimits,
    issues: Vec<AdmissionIssue>,
    selected_version: bool,
    profiles: Option<NodeId>,
    excluded: BTreeSet<NodeId>,
    evaluation: Option<SchemaEvaluation>,
    model: PhantomData<fn() -> (Policy, Target)>,
}

/// Its fields and construction are private to the admission module. It is not
/// obtainable from a bool, a standalone schema evaluation, or raw Deserialize.
pub(crate) struct CompleteRoot<'analysis, Policy, Target> {
    doc: &'analysis MaterializedDocument,
    model: PhantomData<fn() -> (Policy, Target)>,
}

impl<Policy, Target> CompleteRoot<'_, Policy, Target> {
    pub(crate) const fn document(&self) -> &MaterializedDocument {
        self.doc
    }
}

impl<Policy, Target> StructuralAnalysis<Policy, Target> {
    pub(crate) fn issues(&self) -> &[AdmissionIssue] {
        &self.issues
    }
    pub(crate) fn schema_issue_count(&self) -> usize {
        self.evaluation.as_ref().map_or(0, |e| e.issues.len())
    }
    pub(crate) fn structural_units(&self) -> Option<u64> {
        self.evaluation.as_ref().map(|e| e.units)
    }
    pub(crate) const fn version_selected(&self) -> bool {
        self.selected_version
    }
    pub(crate) fn is_complete(&self) -> bool {
        self.selected_version
            && self.issues.is_empty()
            && self
                .evaluation
                .as_ref()
                .is_some_and(|e| e.admitted && e.issues.is_empty())
    }

    fn admit_version(&mut self) {
        let root = self.doc.node(self.doc.root());
        let NodeKind::Object(fields) = root.kind() else {
            self.issues.push(AdmissionIssue {
                reason: StructuralFailure::RootTypeInvalid,
                span: root.span(),
            });
            return;
        };
        let Some(version) = fields.get("schemaVersion") else {
            self.issues.push(AdmissionIssue {
                reason: StructuralFailure::SchemaVersionMissing,
                span: root.span(),
            });
            return;
        };
        let value = self.doc.node(version.value());
        match value.kind() {
            NodeKind::String(version) if version == CONFIGURATION_SCHEMA_VERSION => {
                self.selected_version = true;
            }
            NodeKind::String(_) => self.issues.push(AdmissionIssue {
                reason: StructuralFailure::SchemaVersionUnsupported,
                span: value.span(),
            }),
            _ => self.issues.push(AdmissionIssue {
                reason: StructuralFailure::SchemaVersionTypeInvalid,
                span: value.span(),
            }),
        }
    }

    fn preflight_profiles(&mut self) {
        let NodeKind::Object(root) = self.doc.node(self.doc.root()).kind() else {
            unreachable!()
        };
        let Some(member) = root.get("profiles") else {
            return;
        };
        let profile_node = self.doc.node(member.value());
        let NodeKind::Object(profiles) = profile_node.kind() else {
            return;
        };
        self.profiles = Some(member.value());
        let count = u64::try_from(profiles.len()).expect("materialized collection count fits u64");
        if count > self.limits.max_profiles.get() {
            self.issues.push(AdmissionIssue {
                reason: StructuralFailure::ProfilesLimit {
                    limit: self.limits.max_profiles,
                    actual: count,
                },
                span: profile_node.span(),
            });
            self.excluded.insert(member.value());
        }
        // Length metadata is already available in the bounded materialized tree.
        // Keep every independently provable ID overrun, even when count also fails.
        for (id, member) in profiles {
            let length = u64::try_from(id.len()).expect("materialized key length fits u64");
            if length > self.limits.max_profile_id_bytes.get() {
                self.issues.push(AdmissionIssue {
                    reason: StructuralFailure::ProfileIdLimit {
                        limit: self.limits.max_profile_id_bytes,
                        actual: length,
                    },
                    span: member.key_span(),
                });
                self.excluded.insert(member.value());
            }
        }
    }

    fn admitted_fragment(&self, schema: SchemaId, subject: NodeId) -> bool {
        self.evaluation.as_ref().is_some_and(|e| {
            e.fragments.iter().any(|f| {
                f.schema == schema && f.subject == subject && f.state == FragmentState::Admitted
            })
        })
    }

    fn profile_node(&self, id: &str) -> Option<NodeId> {
        if !self.selected_version
            || u64::try_from(id.len()).ok()? > self.limits.max_profile_id_bytes.get()
            || !valid_identity(id)
        {
            return None;
        }
        let container = self.profiles?;
        if self.excluded.contains(&container) {
            return None;
        }
        let NodeKind::Object(profiles) = self.doc.node(container).kind() else {
            return None;
        };
        let node = profiles.get(id)?.value();
        (!self.excluded.contains(&node)).then_some(node)
    }

    /// A field proof uses its exact owned-schema edge, never an arbitrary passing
    /// anyOf alternative at the same value. Independent descendants may survive
    /// a sibling failure. This does not construct an `IntlifyConfig`.
    pub(crate) fn profile_field<T: DeserializeOwned>(
        &self,
        id: &str,
        fields: &[&str],
    ) -> Result<Option<T>, AdmissionInvariant> {
        let Some(mut subject) = self.profile_node(id) else {
            return Ok(None);
        };
        let mut schema = self.binding.declaration_schema;
        for field in fields {
            let Some(next_schema) = self.binding.program.property(schema, field) else {
                return Ok(None);
            };
            let NodeKind::Object(values) = self.doc.node(subject).kind() else {
                return Ok(None);
            };
            let Some(value) = values.get(*field) else {
                return Ok(None);
            };
            subject = value.value();
            schema = next_schema;
        }
        if !self.admitted_fragment(schema, subject) {
            return Ok(None);
        }
        self.doc
            .decode(subject)
            .map(Some)
            .map_err(|_| AdmissionInvariant::AuthoringTypeMismatch)
    }
}

impl<Policy: DeserializeOwned, Target: DeserializeOwned> StructuralAnalysis<Policy, Target> {
    pub(crate) fn construct(
        &self,
    ) -> Result<Option<IntlifyConfig<Policy, Target>>, AdmissionInvariant> {
        if !self.is_complete() {
            return Ok(None);
        }
        IntlifyConfig::from_complete(&CompleteRoot {
            doc: &self.doc,
            model: PhantomData,
        })
        .map(Some)
        .map_err(|_| AdmissionInvariant::AuthoringTypeMismatch)
    }

    pub(crate) fn profile(
        &self,
        id: &str,
    ) -> Result<Option<ProfileDeclaration<Policy, Target>>, AdmissionInvariant> {
        self.profile_field(id, &[])
    }
}

#[cfg(test)]
pub(crate) mod admission_tests;
#[cfg(test)]
mod eval_tests;

#[cfg(test)]
pub(crate) fn test_schema_accepts(value: &serde_json::Value) -> bool {
    use crate::fixtures::FixtureConfig;
    use crate::input_limits::Bound;
    use std::sync::{Arc, OnceLock};
    static PROGRAM: OnceLock<program::SchemaProgram> = OnceLock::new();
    let program = PROGRAM.get_or_init(|| {
        program::SchemaProgram::compile(&crate::schema::draft7_schema::<FixtureConfig>().unwrap())
            .unwrap()
    });
    let raw = serde_json::to_vec(value).unwrap();
    let doc =
        crate::materialize::materialize_file(Arc::from(raw), crate::materialize_tests::limits())
            .unwrap();
    eval::evaluate(
        program,
        &doc,
        program.root(),
        doc.root(),
        Bound::new(1_000_000).unwrap(),
    )
    .unwrap()
    .admitted
}
