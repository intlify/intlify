// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private schema-guided admission for the minimum profile path. A resolver
//! analyzes a configuration with the configuration schema its construction
//! admitted, for the formal 017 references; tests also analyze with schemas
//! generated from a model's types, isolated synthetic fixtures included, by
//! the same admission mechanism.

use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::{value::BorrowedStrDeserializer, DeserializeOwned};
use serde::Deserialize;

use crate::input_limits::{Bound, ValueCounts};
use crate::materialize::{ByteSpan, DecodeError, MaterializedDocument, NodeId, NodeKind};
use crate::model::{valid_identity, IntlifyConfig, ProfileDeclaration, ProfileId};
use eval::{EvaluationFailure, FragmentState, SchemaEvaluation};
pub(crate) use place::{Edge, Place};
use program::{ProgramError, SchemaId, SchemaProgram};

mod eval;
pub(crate) mod facts;
mod place;
mod program;
pub(crate) mod selection;

#[cfg(feature = "dev-example")]
pub mod example;

#[cfg(feature = "benchmark")]
mod observation;

/// Explicit bounds for this stage, not a default or a formal capability body.
/// Structural admission applies `max_structural_analysis_units`; provisional
/// selection applies `max_profiles` and `max_profile_id_bytes`, which 015
/// gives `profile-selection` (D15:1885, 1886, Decision 015-162).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    StructuralWorkLimit { limit: Bound, actual: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AdmissionIssue {
    pub(crate) reason: StructuralFailure,
    pub(crate) span: ByteSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AdmissionInvariant {
    #[cfg(test)]
    SchemaGeneration,
    SchemaCompilation,
    ModelLayout,
    WorkAccounting,
    AuthoringTypeMismatch,
}

struct BoundSchema {
    /// The `schemaVersion` the schema is mapped to.
    version: String,
    body: serde_json::Value,
    program: SchemaProgram,
    declaration_schema: SchemaId,
}

/// A configuration schema compiled for the internal minimum path, bound to
/// the model types it describes. A resolver compiles the configuration schema
/// its construction admitted, for the formal reference types; tests also
/// compile the schema generated from a model's types, isolated fixtures
/// included.
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

impl<Policy, Target> fmt::Debug for AuthoringSchema<Policy, Target> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringSchema")
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
impl<Policy: schemars::JsonSchema, Target: schemars::JsonSchema> AuthoringSchema<Policy, Target> {
    /// Compile the schema generated from the model's types, for the
    /// `schemaVersion` the model reads.
    pub(crate) fn for_model() -> Result<Self, AdmissionInvariant> {
        let body = crate::schema::draft7_schema::<IntlifyConfig<Policy, Target>>()
            .map_err(|_| AdmissionInvariant::SchemaGeneration)?;
        Self::compile(crate::model::CONFIGURATION_SCHEMA_VERSION.to_owned(), body)
    }
}

impl<Policy, Target> AuthoringSchema<Policy, Target> {
    /// Compile `body`, the schema mapped to `version`, for the model it
    /// describes, `IntlifyConfig<Policy, Target>`. Nothing here checks that
    /// it describes that model: the caller compiles a schema it knows does.
    ///
    /// Admission selects the schema only for a configuration whose root
    /// `schemaVersion` is `version`, and refuses any other version as
    /// unsupported.
    pub(crate) fn compile(
        version: String,
        body: serde_json::Value,
    ) -> Result<Self, AdmissionInvariant> {
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
                version,
                body,
                program,
                declaration_schema,
            }),
            model: PhantomData,
        })
    }

    pub(crate) fn schema_body(&self) -> &serde_json::Value {
        &self.binding.body
    }

    /// Where each edge of `edges`, an internal path from the configuration
    /// root, sits in this schema: `None` for an index (Decision 015-184).
    pub(crate) fn places(&self, edges: &[Edge<'_>]) -> Vec<Option<Place>> {
        place::places(&self.binding.program, edges)
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
            evaluation: None,
            model: PhantomData,
        };
        result.admit_version();
        if !result.selected_version {
            return Ok(result);
        }
        result.locate_profiles();
        let program = &self.binding.program;
        // Every applicable evaluation counts, the declarations past
        // `maxProfiles` or with an ID past `maxProfileIdBytes` included: those
        // bounds belong to profile selection (D15:1885-1887).
        match eval::evaluate(
            program,
            &result.doc,
            program.root(),
            result.doc.root(),
            limits.max_structural_analysis_units,
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
            Err(EvaluationFailure::ScopeLayout) => return Err(AdmissionInvariant::ModelLayout),
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
    /// The candidates of the schema-guided analysis, when it ran.
    pub(crate) fn facts(&self) -> Option<&facts::Facts> {
        self.evaluation.as_ref().map(|e| &e.facts)
    }
    /// The declarations of the root's `profiles` object as the policy
    /// rechecks them: how many there are, every declaration counted, and the
    /// Profile ID of each whose key has the identifier syntax, in their
    /// unsigned byte order. A later phase has no subject for a declaration
    /// whose key lacks the syntax (D15:1885, 1886, 2184). None without a
    /// selected version or a `profiles` object.
    ///
    /// # Errors
    ///
    /// Returns the defect of a key with the syntax that is no Profile ID.
    pub(crate) fn declarations(&self) -> Result<Option<(u64, Vec<ProfileId>)>, AdmissionInvariant> {
        if !self.selected_version {
            return Ok(None);
        }
        let Some(container) = self.profiles else {
            return Ok(None);
        };
        let NodeKind::Object(profiles) = self.doc.node(container).kind() else {
            return Ok(None);
        };
        let count = u64::try_from(profiles.len()).expect("materialized collection count fits u64");
        let ids = profiles
            .keys()
            .filter(|name| valid_identity(name))
            .map(|name| {
                ProfileId::deserialize(BorrowedStrDeserializer::<DecodeError>::new(name))
                    .map_err(|_| AdmissionInvariant::AuthoringTypeMismatch)
            })
            .collect::<Result<_, _>>()?;
        Ok(Some((count, ids)))
    }
    /// The key of the identity map entry `entry` when it has the identifier
    /// syntax: the admitted canonical identity that keys the entry's
    /// subjects (D15:2181, 2184). None for a key without the syntax.
    pub(crate) fn admitted_key(&self, entry: &facts::Entry) -> Option<&str> {
        if !entry.admitted {
            return None;
        }
        let NodeKind::Object(members) = self.doc.node(entry.map).kind() else {
            return None;
        };
        members
            .iter()
            .find(|(_, member)| member.value() == entry.value)
            .map(|(name, _)| name.as_str())
    }
    /// The materialized document and the value of the root's `profiles`
    /// member in it, which a later phase's Evidence locates (D15:790). None
    /// without a selected version or the member.
    pub(crate) fn profiles_value(&self) -> Option<(&MaterializedDocument, NodeId)> {
        self.profiles.map(|value| (&*self.doc, value))
    }
    /// The materialized root's value counts, which Phase 4 rechecks.
    pub(crate) fn value_counts(&self) -> ValueCounts {
        self.doc.counts().value
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
            NodeKind::String(version) if *version == self.binding.version => {
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

    /// Finds the root `profiles` member, which selection reads; whoever reads
    /// it checks that it is an object.
    fn locate_profiles(&mut self) {
        let NodeKind::Object(root) = self.doc.node(self.doc.root()).kind() else {
            unreachable!("a selected version has an object root")
        };
        self.profiles = root
            .get("profiles")
            .map(crate::materialize::ObjectMember::value);
    }

    fn admitted_fragment(&self, schema: SchemaId, subject: NodeId) -> bool {
        self.evaluation.as_ref().is_some_and(|e| {
            e.fragments.iter().any(|f| {
                f.schema == schema && f.subject == subject && f.state == FragmentState::Admitted
            })
        })
    }

    /// The value of the declaration `id`. A key without the identifier syntax
    /// has none: structural admission evaluates its value, but later phases
    /// have no subject for the entry (D15:2184).
    fn profile_node(&self, id: &str) -> Option<NodeId> {
        if !self.selected_version || !valid_identity(id) {
            return None;
        }
        let NodeKind::Object(profiles) = self.doc.node(self.profiles?).kind() else {
            return None;
        };
        Some(profiles.get(id)?.value())
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

    /// The member `name` of the declaration `id` as the analysis left it:
    /// absent, admitted with its value, or present without an admitted
    /// fragment. A declaration without an object value, or under a key
    /// without the identifier syntax, has no member.
    ///
    /// # Errors
    ///
    /// Returns the defect of an admitted member its type cannot read, or of a
    /// name the declaration's schema does not define.
    pub(crate) fn profile_member<T: DeserializeOwned>(
        &self,
        id: &str,
        name: &str,
    ) -> Result<Member<T>, AdmissionInvariant> {
        let schema = self
            .binding
            .program
            .property(self.binding.declaration_schema, name)
            .ok_or(AdmissionInvariant::ModelLayout)?;
        let Some(subject) = self.profile_node(id) else {
            return Ok(Member::Unadmitted);
        };
        let NodeKind::Object(values) = self.doc.node(subject).kind() else {
            return Ok(Member::Unadmitted);
        };
        let Some(value) = values.get(name) else {
            return Ok(Member::Absent);
        };
        if !self.admitted_fragment(schema, value.value()) {
            return Ok(Member::Unadmitted);
        }
        self.doc
            .decode(value.value())
            .map(Member::Admitted)
            .map_err(|_| AdmissionInvariant::AuthoringTypeMismatch)
    }
}

/// A member of a declaration as structural analysis left it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Member<T> {
    Absent,
    Admitted(T),
    /// Present, but its fragment was not admitted, or the declaration has no
    /// object to hold it.
    Unadmitted,
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
