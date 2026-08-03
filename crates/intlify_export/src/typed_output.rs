// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Immutable language-neutral argument signatures paired with typed-key models.

use intlify_contract::CatalogKey;
use intlify_linker::{LinkOutcome, MessageBundlePlan, TypedKeyModel};
use ox_mf2_parser::{DeclarationKind, SemanticModel};

/// Fully checked preparation input borrowed by platform exporters.
#[derive(Debug)]
pub struct ValidatedExportBatch<'a> {
    _outcome: &'a LinkOutcome,
    plans: &'a [MessageBundlePlan],
    typed_output: Box<[ValidatedTypedOutput<'a>]>,
}

impl<'a> ValidatedExportBatch<'a> {
    pub(crate) fn new(
        outcome: &'a LinkOutcome,
        plans: &'a [MessageBundlePlan],
        typed_output: Vec<ValidatedTypedOutput<'a>>,
    ) -> Self {
        Self {
            _outcome: outcome,
            plans,
            typed_output: typed_output.into_boxed_slice(),
        }
    }

    #[must_use]
    pub const fn plans(&self) -> &[MessageBundlePlan] {
        self.plans
    }

    #[must_use]
    pub const fn typed_output(&self) -> &[ValidatedTypedOutput<'a>] {
        &self.typed_output
    }
}

/// One canonical typed-key model and its per-key signatures.
#[derive(Debug)]
pub struct ValidatedTypedOutput<'a> {
    pub(crate) model: &'a TypedKeyModel,
    pub(crate) messages: Box<[ValidatedMessageSignature<'a>]>,
}

impl<'a> ValidatedTypedOutput<'a> {
    #[must_use]
    pub const fn model(&self) -> &'a TypedKeyModel {
        self.model
    }

    #[must_use]
    pub const fn messages(&self) -> &[ValidatedMessageSignature<'a>] {
        &self.messages
    }
}

/// One canonical model key and its complete required argument set.
#[derive(Debug)]
pub struct ValidatedMessageSignature<'a> {
    pub(crate) key: &'a CatalogKey,
    pub(crate) arguments: Box<[MessageArgumentSignature]>,
}

impl<'a> ValidatedMessageSignature<'a> {
    #[must_use]
    pub const fn key(&self) -> &'a CatalogKey {
        self.key
    }

    #[must_use]
    pub const fn arguments(&self) -> &[MessageArgumentSignature] {
        &self.arguments
    }
}

/// One required external argument with optional direct input annotation evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageArgumentSignature {
    name: Box<str>,
    input_annotation: Option<Box<str>>,
}

impl MessageArgumentSignature {
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn input_annotation(&self) -> Option<&str> {
        self.input_annotation.as_deref()
    }
}

pub(crate) fn derive_arguments(model: &SemanticModel) -> Vec<MessageArgumentSignature> {
    let mut arguments = std::collections::BTreeMap::<Box<str>, Option<Box<str>>>::new();

    for declaration in model
        .semantic_declarations()
        .iter()
        .filter(|declaration| declaration.kind() == DeclarationKind::Input)
    {
        arguments.insert(
            declaration.name().into(),
            declaration.input_annotation().map(Into::into),
        );
    }

    for reference in model
        .semantic_references()
        .iter()
        .filter(|reference| reference.resolved_declaration().is_none())
    {
        arguments.entry(reference.name().into()).or_insert(None);
    }

    arguments
        .into_iter()
        .map(|(name, input_annotation)| MessageArgumentSignature {
            name,
            input_annotation,
        })
        .collect()
}
