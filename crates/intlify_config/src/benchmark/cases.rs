// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite owner case declarations for the four active minimum boundaries.
//! This is not a common Run Plan, case-identity codec, or a complete 015 suite.
//! Preparation yields an unadmitted candidate; the separate fixture registry
//! binds its exact input/result/work before method-bound collection can use it.

use serde::{Deserialize, Serialize};

use super::operation::Operation;

mod inputs;
pub(super) use inputs::Recipe;
mod context;
pub(super) mod prepare;
pub(super) mod registry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum LimitKind {
    FileBytes,
    ParserTokens,
    Nodes,
    Depth,
    CollectionEntries,
    TotalStringBytes,
    SingleStringBytes,
    Profiles,
    ProfileIdBytes,
    StructuralUnits,
}

impl LimitKind {
    const ENTRY: [Self; 7] = [
        Self::FileBytes,
        Self::ParserTokens,
        Self::Nodes,
        Self::Depth,
        Self::CollectionEntries,
        Self::TotalStringBytes,
        Self::SingleStringBytes,
    ];
    const STRUCTURAL: [Self; 3] = [Self::Profiles, Self::ProfileIdBytes, Self::StructuralUnits];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum LimitEdge {
    Exact,
    FirstOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Selector {
    Absent,
    App,
    Unknown,
    InvalidSyntax,
    InvalidType,
    ExactByteLimit,
    FirstOverByteLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum ExpectedKind {
    Materialized,
    EntryFailure,
    StructuralComplete,
    StructuralBlocked,
    AuthoringComplete,
    Selected,
    SelectionRejected,
    SelectionUnavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct Declaration {
    pub(super) operation: Operation,
    pub(super) fixture: Recipe,
    pub(super) fixture_revision: String,
    #[serde(deserialize_with = "Option::deserialize")]
    pub(super) limit: Option<(LimitKind, LimitEdge)>,
    pub(super) selector: Selector,
    pub(super) expected_kind: ExpectedKind,
}

fn declaration(
    operation: Operation,
    fixture: Recipe,
    selector: Selector,
    expected_kind: ExpectedKind,
) -> Declaration {
    Declaration {
        operation,
        fixture,
        fixture_revision: "0".into(),
        limit: None,
        selector,
        expected_kind,
    }
}

/// Order is explicit and stable. All declarations are required by the initial
/// internal profile; unsupported acquisition does not delete a declaration.
pub(super) fn declarations() -> Vec<Declaration> {
    let mut cases = Vec::new();
    // Independently scale bytes, strings, declarations, and locale occurrences;
    // the latter are authoring input only, not canonicalized locale work.
    for operation in Operation::ALL {
        for fixture in [
            Recipe::Minimal,
            Recipe::ReversedMembers,
            Recipe::PaddedBytes,
            Recipe::LongString,
            Recipe::ManyProfiles,
            Recipe::ManyLocaleOccurrences,
        ] {
            let expected = match operation {
                Operation::FileMaterialization => ExpectedKind::Materialized,
                Operation::StructuralAnalysis => ExpectedKind::StructuralComplete,
                Operation::AuthoringConstruction => ExpectedKind::AuthoringComplete,
                Operation::ProfileSelection => ExpectedKind::Selected,
            };
            let selector = if operation == Operation::ProfileSelection {
                Selector::App
            } else {
                Selector::Absent
            };
            cases.push(declaration(operation, fixture, selector, expected));
        }
    }
    for fixture in [
        Recipe::NestedArray,
        Recipe::ManyValues,
        Recipe::PortableMaximum,
        Recipe::NegativeZero,
    ] {
        cases.push(declaration(
            Operation::FileMaterialization,
            fixture,
            Selector::Absent,
            ExpectedKind::Materialized,
        ));
    }
    for fixture in [
        Recipe::InvalidUtf8,
        Recipe::InvalidJson,
        Recipe::TrailingToken,
        Recipe::DuplicateKey,
        Recipe::EscapedDuplicateKey,
        Recipe::InvalidSurrogate,
        Recipe::NonPortableNumber,
    ] {
        cases.push(declaration(
            Operation::FileMaterialization,
            fixture,
            Selector::Absent,
            ExpectedKind::EntryFailure,
        ));
    }
    for fixture in [
        Recipe::RootNull,
        Recipe::MissingVersion,
        Recipe::WrongVersionType,
        Recipe::UnsupportedVersion,
        Recipe::EmptyProfiles,
        Recipe::UnknownMember,
        Recipe::MissingNullablePolicy,
        Recipe::NullSourceDefault,
        Recipe::InvalidSibling,
        Recipe::InvalidResourceReference,
        Recipe::DenseInvalid,
    ] {
        cases.push(declaration(
            Operation::StructuralAnalysis,
            fixture,
            Selector::Absent,
            ExpectedKind::StructuralBlocked,
        ));
    }
    for (fixture, selector, expected) in [
        (Recipe::Minimal, Selector::Absent, ExpectedKind::Selected),
        (
            Recipe::ManyProfiles,
            Selector::Absent,
            ExpectedKind::SelectionRejected,
        ),
        (
            Recipe::Minimal,
            Selector::Unknown,
            ExpectedKind::SelectionRejected,
        ),
        (
            Recipe::Minimal,
            Selector::InvalidSyntax,
            ExpectedKind::SelectionRejected,
        ),
        (
            Recipe::Minimal,
            Selector::InvalidType,
            ExpectedKind::SelectionRejected,
        ),
        (
            Recipe::Minimal,
            Selector::ExactByteLimit,
            ExpectedKind::SelectionRejected,
        ),
        (
            Recipe::Minimal,
            Selector::FirstOverByteLimit,
            ExpectedKind::SelectionRejected,
        ),
        (
            Recipe::InvalidSibling,
            Selector::App,
            ExpectedKind::Selected,
        ),
        (
            Recipe::InvalidResourceReference,
            Selector::App,
            ExpectedKind::SelectionUnavailable,
        ),
        (
            Recipe::MissingVersion,
            Selector::App,
            ExpectedKind::SelectionUnavailable,
        ),
        (
            Recipe::EmptyProfiles,
            Selector::Absent,
            ExpectedKind::SelectionUnavailable,
        ),
    ] {
        cases.push(declaration(
            Operation::ProfileSelection,
            fixture,
            selector,
            expected,
        ));
    }
    for (operation, limits) in [
        (Operation::FileMaterialization, &LimitKind::ENTRY[..]),
        (Operation::StructuralAnalysis, &LimitKind::STRUCTURAL[..]),
    ] {
        for &limit in limits {
            for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
                let expected = match (operation, edge) {
                    (Operation::FileMaterialization, LimitEdge::Exact) => {
                        ExpectedKind::Materialized
                    }
                    (Operation::FileMaterialization, LimitEdge::FirstOver) => {
                        ExpectedKind::EntryFailure
                    }
                    (_, LimitEdge::Exact) => ExpectedKind::StructuralComplete,
                    (_, LimitEdge::FirstOver) => ExpectedKind::StructuralBlocked,
                };
                // Two profiles make first-over representable with a positive
                // bound. No case fabricates a zero ResourceBoundValue.
                let fixture = if limit == LimitKind::Profiles {
                    Recipe::TwoProfiles
                } else {
                    Recipe::Minimal
                };
                let mut case = declaration(operation, fixture, Selector::Absent, expected);
                case.limit = Some((limit, edge));
                cases.push(case);
            }
        }
    }
    cases
}

#[cfg(test)]
mod tests;
