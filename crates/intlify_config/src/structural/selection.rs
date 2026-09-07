// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private, provisional profile selection. No policy recheck, retained selector
//! projection, final Finding, or checked Profile is produced here.

use serde::de::{value::BorrowedStrDeserializer, DeserializeOwned};
use serde::Deserialize;

use crate::input_limits::Bound;
use crate::materialize::{ByteSpan, DecodeError, NodeKind};
use crate::model::{valid_identity, ProfileId};

use super::{AdmissionInvariant, FragmentState, StructuralAnalysis};

/// A binding supplies only a safely established top-level tag, never a host
/// object or container contents. Rust string inputs are already Unicode-scalar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InvalidSelectorType {
    Null,
    Boolean,
    Number,
    Array,
    Object,
}

enum SelectorKind {
    Absent,
    String(String),
    OverLimitString,
    InvalidType(InvalidSelectorType),
}

#[cfg(feature = "benchmark")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectorByteObservation {
    NotApplicable,
    Exact(u64),
    AtLeast(u64),
}

/// No Debug/Serialize: a bounded string can still contain an arbitrary secret.
/// The explicit bound belongs to the same bootstrap context as structural input.
pub(crate) struct SelectorInput {
    bound: Bound,
    kind: SelectorKind,
}

impl SelectorInput {
    #[cfg(feature = "benchmark")]
    pub(crate) fn benchmark_id_bytes(&self) -> SelectorByteObservation {
        match &self.kind {
            SelectorKind::Absent | SelectorKind::InvalidType(_) => {
                SelectorByteObservation::NotApplicable
            }
            SelectorKind::String(value) => SelectorByteObservation::Exact(
                u64::try_from(value.len()).expect("bounded selector length"),
            ),
            // Keep the admitted marker's witness, not the discarded raw length.
            SelectorKind::OverLimitString => SelectorByteObservation::AtLeast(self.bound.get() + 1),
        }
    }

    pub(crate) const fn absent(bound: Bound) -> Self {
        Self {
            bound,
            kind: SelectorKind::Absent,
        }
    }

    pub(crate) fn string(value: &str, bound: Bound) -> Self {
        let length = u64::try_from(value.len()).expect("Rust string length fits u64");
        Self {
            bound,
            kind: if length > bound.get() {
                // Do not copy an over-limit value, retain its final length, or
                // scan its syntax. The bound implies the smallest first-over witness.
                SelectorKind::OverLimitString
            } else {
                SelectorKind::String(value.to_owned())
            },
        }
    }

    pub(crate) const fn invalid_type(kind: InvalidSelectorType, bound: Bound) -> Self {
        Self {
            bound,
            kind: SelectorKind::InvalidType(kind),
        }
    }
}

/// Content-free internal observations, not final selector Evidence or Findings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionFailure {
    Required,
    InvalidType(InvalidSelectorType),
    InvalidSyntax { bytes: u64 },
    OverLimit { limit: Bound, first_over: u64 },
    Unknown { bytes: u64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionPrerequisite {
    ConfigurationVersion,
    StructuralWork,
    ProfilesShape,
    ProfilesLimit,
    DeclarationIdentity,
    DeclarationBoundary,
    ResourceLimitsReference,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SelectionInvariant {
    BootstrapBoundMismatch,
    Admission(AdmissionInvariant),
}

/// Only a declared, independently admitted ID is available through this type.
/// It is not a complete declaration, a confirmed policy selection, or a Profile.
pub(crate) struct ProvisionalSelection<Policy> {
    id: ProfileId,
    resource_limits: Policy,
    declaration_key_span: ByteSpan,
}

impl<Policy> ProvisionalSelection<Policy> {
    pub(crate) const fn id(&self) -> &ProfileId {
        &self.id
    }
    pub(crate) const fn resource_limits(&self) -> &Policy {
        &self.resource_limits
    }
    pub(crate) const fn declaration_key_span(&self) -> ByteSpan {
        self.declaration_key_span
    }
}

pub(crate) enum Selection<Policy> {
    Selected(ProvisionalSelection<Policy>),
    Rejected(SelectionFailure),
    Unavailable(SelectionPrerequisite),
}

impl<Policy: DeserializeOwned, Target> StructuralAnalysis<Policy, Target> {
    pub(crate) fn select(
        &self,
        input: &SelectorInput,
    ) -> Result<Selection<Policy>, SelectionInvariant> {
        if input.bound != self.limits.max_profile_id_bytes {
            // A marker observed under one bound cannot be reinterpreted as a
            // complete string under another. Policy recheck is not implemented.
            return Err(SelectionInvariant::BootstrapBoundMismatch);
        }
        let name = match &input.kind {
            SelectorKind::Absent => None,
            SelectorKind::InvalidType(kind) => {
                return Ok(Selection::Rejected(SelectionFailure::InvalidType(*kind)));
            }
            SelectorKind::OverLimitString => {
                return Ok(Selection::Rejected(SelectionFailure::OverLimit {
                    limit: input.bound,
                    first_over: input.bound.get() + 1,
                }));
            }
            SelectorKind::String(text) => {
                if !valid_identity(text) {
                    return Ok(Selection::Rejected(SelectionFailure::InvalidSyntax {
                        bytes: u64::try_from(text.len()).expect("bounded string length"),
                    }));
                }
                Some(text.as_str())
            }
        };
        // Type/length/syntax checks above need no declaration. Matching and
        // omission cardinality below need independently admitted structural data.
        if !self.selected_version {
            return Ok(Selection::Unavailable(
                SelectionPrerequisite::ConfigurationVersion,
            ));
        }
        let Some(evaluation) = &self.evaluation else {
            return Ok(Selection::Unavailable(
                SelectionPrerequisite::StructuralWork,
            ));
        };
        let Some(container) = self.profiles else {
            return Ok(Selection::Unavailable(SelectionPrerequisite::ProfilesShape));
        };
        if self.excluded.contains(&container) {
            return Ok(Selection::Unavailable(SelectionPrerequisite::ProfilesLimit));
        }
        let NodeKind::Object(profiles) = self.doc.node(container).kind() else {
            return Ok(Selection::Unavailable(SelectionPrerequisite::ProfilesShape));
        };
        if profiles.is_empty() {
            return Ok(Selection::Unavailable(SelectionPrerequisite::ProfilesShape));
        }
        let (id, member) = if let Some(name) = name {
            let Some(declaration) = profiles.get_key_value(name) else {
                return Ok(Selection::Rejected(SelectionFailure::Unknown {
                    bytes: u64::try_from(name.len()).expect("bounded selector length"),
                }));
            };
            declaration
        } else {
            // Count the original object, never a filtered valid-profile prefix.
            if profiles.len() != 1 {
                return Ok(Selection::Rejected(SelectionFailure::Required));
            }
            profiles.first_key_value().expect("non-empty profile map")
        };
        if self.excluded.contains(&member.value()) || !valid_identity(id) {
            return Ok(Selection::Unavailable(
                SelectionPrerequisite::DeclarationIdentity,
            ));
        }
        let subject = member.value();
        // A declaration boundary is the immediate object shape: type, required
        // fields, and closedness. Descendant failures do not erase that boundary.
        // Consult the existing evaluation rather than maintaining another field list.
        let boundary_evaluated = evaluation.fragments.iter().any(|fragment| {
            fragment.subject == subject
                && fragment.schema == self.binding.declaration_schema
                && matches!(
                    fragment.state,
                    FragmentState::Admitted | FragmentState::Invalid
                )
        });
        if !boundary_evaluated
            || !matches!(self.doc.node(subject).kind(), NodeKind::Object(_))
            || evaluation
                .issues
                .iter()
                .any(|issue| issue.subject == subject)
        {
            return Ok(Selection::Unavailable(
                SelectionPrerequisite::DeclarationBoundary,
            ));
        }
        let Some(resource_limits) = self
            .profile_field(id, &["policies", "resourceLimits"])
            .map_err(SelectionInvariant::Admission)?
        else {
            return Ok(Selection::Unavailable(
                SelectionPrerequisite::ResourceLimitsReference,
            ));
        };
        let id = ProfileId::deserialize(BorrowedStrDeserializer::<DecodeError>::new(id)).map_err(
            |_| SelectionInvariant::Admission(AdmissionInvariant::AuthoringTypeMismatch),
        )?;
        Ok(Selection::Selected(ProvisionalSelection {
            id,
            resource_limits,
            declaration_key_span: member.key_span(),
        }))
    }
}

#[cfg(test)]
mod tests;
