// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite owner case declarations for the six active minimum boundaries.
//! This is not a common Run Plan, case-identity codec, or a complete 015 suite.
//! Preparation yields an unadmitted candidate; the separate fixture registry
//! binds its exact input/result/work before method-bound collection can use it.

use serde::{Deserialize, Serialize};

use super::operation::Operation;

mod inputs;
pub(super) use inputs::Recipe;
mod core_inputs;
pub(super) use core_inputs::LocaleCoreRecipe;
mod context;
pub(super) mod prepare;
pub(super) mod registry;

// Revision 1 adopts the formal 015/017 configuration references across the
// complete fixed inventory. Locale-only spellings and result/work codecs retain
// their meaning; no old synthetic row may be silently reused as this revision.
const FIXTURE_REVISION: &str = "1";

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
    LocaleRawIdentifierBytes,
    LocaleCanonicalIdentifierBytes,
    CoreActiveOccurrences,
    CoreRequestedCardinality,
    CoreRawIdentifierBytes,
    CoreCanonicalIdentifierBytes,
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
    LocaleCanonicalized,
    LocaleRejected,
    LocaleCoreResolved,
    LocaleCoreRejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum LocaleRecipe {
    Language,
    Region,
    Casing,
    Alias,
    CanonicalAlias,
    ExtensionOrdering,
    CanonicalExtensions,
    ExpandingAlias,
    CanonicalExpandedAlias,
    InvalidLegacy,
    InvalidValidity,
    InvalidExtension,
    InvalidPrivate,
}

impl LocaleRecipe {
    const ALL: [Self; 13] = [
        Self::Language,
        Self::Region,
        Self::Casing,
        Self::Alias,
        Self::CanonicalAlias,
        Self::ExtensionOrdering,
        Self::CanonicalExtensions,
        Self::ExpandingAlias,
        Self::CanonicalExpandedAlias,
        Self::InvalidLegacy,
        Self::InvalidValidity,
        Self::InvalidExtension,
        Self::InvalidPrivate,
    ];

    pub(super) const fn spelling(self) -> &'static str {
        match self {
            Self::Language => "en",
            Self::Region => "en-US",
            Self::Casing => "EN-us",
            Self::Alias => "iw-IL",
            Self::CanonicalAlias => "he-IL",
            Self::ExtensionOrdering => "en-u-nu-latn-ca-gregory",
            Self::CanonicalExtensions => "en-u-ca-gregory-nu-latn",
            Self::ExpandingAlias => "und-u-ca-islamicc",
            Self::CanonicalExpandedAlias => "und-u-ca-islamic-civil",
            Self::InvalidLegacy => "en_US",
            Self::InvalidValidity => "zz",
            Self::InvalidExtension => "en-u-ca-madeup",
            Self::InvalidPrivate => "en-x-brand",
        }
    }

    /// Owner-declared expected result, not learned from a provider invocation.
    pub(super) const fn canonical(self) -> Option<&'static str> {
        match self {
            Self::Language => Some("en"),
            Self::Region | Self::Casing => Some("en-US"),
            Self::Alias | Self::CanonicalAlias => Some("he-IL"),
            Self::ExtensionOrdering | Self::CanonicalExtensions => Some("en-u-ca-gregory-nu-latn"),
            Self::ExpandingAlias | Self::CanonicalExpandedAlias => Some("und-u-ca-islamic-civil"),
            _ => None,
        }
    }
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
        fixture_revision: FIXTURE_REVISION.into(),
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
    for operation in [
        Operation::FileMaterialization,
        Operation::StructuralAnalysis,
        Operation::AuthoringConstruction,
        Operation::ProfileSelection,
    ] {
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
                Operation::LocaleCanonicalization | Operation::LocaleCoreResolution => {
                    unreachable!()
                }
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
    // Append the new operation without changing the existing 77 declarations.
    for recipe in LocaleRecipe::ALL {
        cases.push(declaration(
            Operation::LocaleCanonicalization,
            Recipe::Locale(recipe),
            Selector::Absent,
            if recipe.canonical().is_some() {
                ExpectedKind::LocaleCanonicalized
            } else {
                ExpectedKind::LocaleRejected
            },
        ));
    }
    for (limit, recipe) in [
        (LimitKind::LocaleRawIdentifierBytes, LocaleRecipe::Region),
        (
            LimitKind::LocaleCanonicalIdentifierBytes,
            LocaleRecipe::ExpandingAlias,
        ),
    ] {
        for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
            let mut case = declaration(
                Operation::LocaleCanonicalization,
                Recipe::Locale(recipe),
                Selector::Absent,
                if edge == LimitEdge::Exact {
                    ExpectedKind::LocaleCanonicalized
                } else {
                    ExpectedKind::LocaleRejected
                },
            );
            case.limit = Some((limit, edge));
            cases.push(case);
        }
    }
    // Append the project locale-core slice without changing any of the 94 rows.
    for recipe in LocaleCoreRecipe::ALL {
        cases.push(declaration(
            Operation::LocaleCoreResolution,
            Recipe::LocaleCore(recipe),
            Selector::App,
            if recipe.resolves() {
                ExpectedKind::LocaleCoreResolved
            } else {
                ExpectedKind::LocaleCoreRejected
            },
        ));
    }
    for (limit, recipe) in [
        (LimitKind::CoreActiveOccurrences, LocaleCoreRecipe::Multi),
        (LimitKind::CoreRequestedCardinality, LocaleCoreRecipe::Multi),
        (LimitKind::CoreRawIdentifierBytes, LocaleCoreRecipe::Minimal),
        (
            LimitKind::CoreCanonicalIdentifierBytes,
            LocaleCoreRecipe::ExpandingAlias,
        ),
    ] {
        for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
            let mut case = declaration(
                Operation::LocaleCoreResolution,
                Recipe::LocaleCore(recipe),
                Selector::App,
                if edge == LimitEdge::Exact {
                    ExpectedKind::LocaleCoreResolved
                } else {
                    ExpectedKind::LocaleCoreRejected
                },
            );
            case.limit = Some((limit, edge));
            cases.push(case);
        }
    }
    cases
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod locale_tests;

#[cfg(test)]
mod core_tests;
