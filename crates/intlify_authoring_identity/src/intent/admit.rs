// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Reading a `message-intent` or `message-reference` artifact and deciding
//! whether to admit it.
//!
//! The order is the registry kinds' order: design 017's shared read, then
//! this reader's bounds, then the body's structural rules.
//!
//! What admission establishes is narrow. An admitted Intent or reference is
//! well formed and unaltered since sealing; that is not proof that it is what
//! compiling its inventory against its registry gives. A reader that needs
//! that compiles the same scope and compares.

use intlify_authoring::{read_sealed, AuthoringArtifact, AuthoringArtifactReference};

use super::artifact::{MessageIntentArtifact, MessageReferenceArtifact};
use super::model::{IntentFailure, MessageIntentBody, MessageReferenceBody, ReferenceFailure};
use crate::admission::{within, IdentityAdmissionFailure};
use crate::limits::{IdentityLimitKind, IdentityLimits};
use crate::registry::{ContinuationBasis, SourceEdit};

/// One `message-intent` artifact that passed admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedIntent {
    artifact: MessageIntentArtifact,
}

impl AdmittedIntent {
    /// Borrow the admitted artifact.
    #[must_use]
    pub const fn artifact(&self) -> &MessageIntentArtifact {
        &self.artifact
    }

    /// Borrow the admitted body.
    #[must_use]
    pub fn body(&self) -> &MessageIntentBody {
        self.artifact.body()
    }

    /// Return the reference another artifact uses to name this one.
    #[must_use]
    pub fn reference(&self) -> AuthoringArtifactReference {
        self.artifact.reference()
    }
}

/// One `message-reference` artifact that passed admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmittedReference {
    artifact: MessageReferenceArtifact,
}

impl AdmittedReference {
    /// Borrow the admitted artifact.
    #[must_use]
    pub const fn artifact(&self) -> &MessageReferenceArtifact {
        &self.artifact
    }

    /// Borrow the admitted body.
    #[must_use]
    pub fn body(&self) -> &MessageReferenceBody {
        self.artifact.body()
    }

    /// Return the reference another artifact uses to name this one.
    #[must_use]
    pub fn reference(&self) -> AuthoringArtifactReference {
        self.artifact.reference()
    }
}

/// Admit one `message-intent` artifact.
pub fn admit_intent(
    bytes: &[u8],
    limits: &IdentityLimits,
) -> Result<AdmittedIntent, IdentityAdmissionFailure<IntentFailure>> {
    let artifact: MessageIntentArtifact =
        read_sealed(bytes).map_err(IdentityAdmissionFailure::Read)?;
    let body = artifact.body();
    let changes = match body.continuity().map(super::IntentContinuity::basis) {
        Some(ContinuationBasis::VerifiedEdit(edit)) => edit.changes(),
        _ => &[],
    };
    within(
        changes.len(),
        limits.source_edits,
        IdentityLimitKind::SourceEdits,
    )?;
    within(
        changes.iter().map(|edit| edit.replacements().len()).sum(),
        limits.replacements,
        IdentityLimitKind::Replacements,
    )?;
    within(
        changes
            .iter()
            .flat_map(SourceEdit::replacements)
            .map(|replacement| replacement.text().len())
            .sum(),
        limits.replacement_bytes,
        IdentityLimitKind::ReplacementBytes,
    )?;
    body.validate()
        .map_err(IdentityAdmissionFailure::Structure)?;
    Ok(AdmittedIntent { artifact })
}

/// Admit one `message-reference` artifact.
pub fn admit_reference(
    bytes: &[u8],
    limits: &IdentityLimits,
) -> Result<AdmittedReference, IdentityAdmissionFailure<ReferenceFailure>> {
    let artifact: MessageReferenceArtifact =
        read_sealed(bytes).map_err(IdentityAdmissionFailure::Read)?;
    let body = artifact.body();
    within(
        body.targets().len(),
        limits.targets,
        IdentityLimitKind::Targets,
    )?;
    body.validate()
        .map_err(IdentityAdmissionFailure::Structure)?;
    Ok(AdmittedReference { artifact })
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{
        ByteRange, Completeness, IntegrityDigest, ReadFailure, VersionedIdentity,
    };
    use serde_json::{json, Value};

    use super::*;
    use crate::continuity::{EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION};
    use crate::intent::{IntentContinuity, ReferenceTarget};
    use crate::registry::fixtures::{declaration, id, inventory_of, limits, Base, Unit};
    use crate::registry::Replacement;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn bytes(value: &impl serde::Serialize) -> Vec<u8> {
        serde_json::to_vec(value).expect("serializable")
    }

    /// An Intent carried across one edit with two replacements, and a
    /// reference naming two targets.
    struct Sealed {
        intent: MessageIntentArtifact,
        reference: MessageReferenceArtifact,
    }

    fn sealed() -> Sealed {
        let first = Unit::new("app.js", "1", "intent('Save')\n");
        let second = Unit::new("app.js", "2", "// a\nintent('Save')\n// b\n");
        let base = Base::of(&[&first], &[A]);
        let current = inventory_of(&[&second], Completeness::Complete);
        let edit = SourceEdit::new(
            Some(first.snapshot()),
            Some(second.snapshot()),
            vec![
                Replacement::new(ByteRange::new(0, 0).unwrap(), "// a\n"),
                Replacement::new(ByteRange::new(15, 15).unwrap(), "// b\n"),
            ],
        );
        let body = MessageIntentBody::new(
            id(A),
            IntegrityDigest::from_hash([7; 32]),
            current.reference(),
            declaration(&current, &second, 0),
            base.registry.reference(),
            Some(IntentContinuity::new(
                declaration(&base.inventory, &first, 0),
                ContinuationBasis::verified_edit(
                    VersionedIdentity::literal(EDIT_REPLAY_PROFILE, EDIT_REPLAY_REVISION),
                    vec![edit],
                ),
            )),
        )
        .expect("a well-formed body");
        let intent = MessageIntentArtifact::seal(body).unwrap();
        let target = |value| {
            ReferenceTarget::new(
                id(value),
                IntegrityDigest::from_hash([7; 32]),
                intent.reference(),
            )
        };
        let reference = MessageReferenceArtifact::seal(
            MessageReferenceBody::new(
                current.reference(),
                current.inventory().references()[0].occurrence().clone(),
                vec![target(A), target(B)],
            )
            .expect("a well-formed body"),
        )
        .unwrap();
        Sealed { intent, reference }
    }

    /// Seal a decoded body as it is, the way a forger with the specification
    /// would: the digest matches, and the body still breaks its rule.
    fn forged<A: AuthoringArtifact>(value: Value) -> Vec<u8>
    where
        A::Body: serde::de::DeserializeOwned,
    {
        let body: A::Body = serde_json::from_value(value).expect("the closed shape");
        bytes(&A::seal(body).expect("sealable"))
    }

    #[test]
    fn a_sealed_intent_and_reference_are_admitted_as_they_are() {
        let sealed = sealed();
        let intent = admit_intent(&bytes(&sealed.intent), &limits()).unwrap();
        assert_eq!(intent.artifact(), &sealed.intent);
        assert_eq!(intent.body(), sealed.intent.body());
        assert_eq!(intent.reference(), sealed.intent.reference());
        let reference = admit_reference(&bytes(&sealed.reference), &limits()).unwrap();
        assert_eq!(reference.artifact(), &sealed.reference);
        assert_eq!(reference.body(), sealed.reference.body());
        assert_eq!(reference.reference(), sealed.reference.reference());
    }

    #[test]
    fn each_kind_is_read_only_as_itself_and_only_unaltered() {
        let sealed = sealed();
        assert_eq!(
            admit_intent(&bytes(&sealed.reference), &limits()),
            Err(IdentityAdmissionFailure::Read(ReadFailure::Unsupported))
        );
        assert_eq!(
            admit_reference(&bytes(&sealed.intent), &limits()),
            Err(IdentityAdmissionFailure::Read(ReadFailure::Unsupported))
        );
        let mut altered = serde_json::to_value(&sealed.intent).unwrap();
        altered["body"]["intentRevision"] = json!(IntegrityDigest::from_hash([8; 32]).as_str());
        assert_eq!(
            admit_intent(&bytes(&altered), &limits()),
            Err(IdentityAdmissionFailure::Read(ReadFailure::Integrity))
        );
        let mut altered = serde_json::to_value(&sealed.reference).unwrap();
        altered["body"]["targets"].as_array_mut().unwrap().pop();
        assert_eq!(
            admit_reference(&bytes(&altered), &limits()),
            Err(IdentityAdmissionFailure::Read(ReadFailure::Integrity))
        );
    }

    #[test]
    fn every_bound_admits_its_exact_value_and_refuses_one_less() {
        let sealed = sealed();
        let intent = bytes(&sealed.intent);
        // One edit, two replacements, ten bytes of replacement text.
        for (count, kind) in [
            (1, IdentityLimitKind::SourceEdits),
            (2, IdentityLimitKind::Replacements),
            (10, IdentityLimitKind::ReplacementBytes),
        ] {
            let bound = |value: u64| {
                let mut limits = limits();
                *match kind {
                    IdentityLimitKind::SourceEdits => &mut limits.source_edits,
                    IdentityLimitKind::Replacements => &mut limits.replacements,
                    _ => &mut limits.replacement_bytes,
                } = value;
                limits
            };
            assert!(admit_intent(&intent, &bound(count)).is_ok(), "{kind:?}");
            assert_eq!(
                admit_intent(&intent, &bound(count - 1)),
                Err(IdentityAdmissionFailure::Limit(kind)),
                "{kind:?}"
            );
        }
        let reference = bytes(&sealed.reference);
        let targets = |targets| IdentityLimits {
            targets,
            ..limits()
        };
        assert!(admit_reference(&reference, &targets(2)).is_ok());
        assert_eq!(
            admit_reference(&reference, &targets(1)),
            Err(IdentityAdmissionFailure::Limit(IdentityLimitKind::Targets))
        );
    }

    #[test]
    fn a_body_that_breaks_a_rule_is_refused_after_its_digest_and_bounds() {
        let sealed = sealed();
        let mut body = serde_json::to_value(sealed.intent.body()).unwrap();
        body["registry"] = body["inventory"].clone();
        assert_eq!(
            admit_intent(&forged::<MessageIntentArtifact>(body), &limits()),
            Err(IdentityAdmissionFailure::Structure(
                IntentFailure::ReferenceKind
            ))
        );
        let mut body = serde_json::to_value(sealed.reference.body()).unwrap();
        body["targets"] = json!([]);
        assert_eq!(
            admit_reference(&forged::<MessageReferenceArtifact>(body), &limits()),
            Err(IdentityAdmissionFailure::Structure(
                ReferenceFailure::EmptyTargets
            ))
        );
    }
}
