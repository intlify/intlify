// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Finite owner case declarations for the seven active boundaries.
//! This is not a common Run Plan, case-identity codec, or a complete 015 suite.
//! Preparation yields an unadmitted candidate; the separate fixture registry
//! binds its exact input/result/work before method-bound collection can use it.

use serde::{Deserialize, Serialize};

use super::operation::Operation;

mod inputs;
pub(super) use inputs::Recipe;
mod resolution_inputs;
pub(super) use resolution_inputs::LocaleResolutionRecipe;
mod policy_inputs;
pub(super) use policy_inputs::ResourcePolicyRecipe;
mod context;
pub(super) mod prepare;
pub(super) mod registry;

// The fixed inventory uses the formal 015/017 configuration references. Until
// Intlify is released, a changed recipe rewrites its expectations in place at
// this revision rather than starting a new one.
const FIXTURE_REVISION: &str = "0";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    ResolutionLocaleOccurrences,
    ResolutionRequestedLocales,
    ResolutionRawIdentifierBytes,
    ResolutionCanonicalIdentifierBytes,
    BootstrapArtifacts,
    RecheckArtifacts,
    RecheckProfileIdBytes,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub(super) enum LimitEdge {
    Exact,
    FirstOver,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
    LocaleResolutionComplete,
    LocaleResolutionBlocked,
    ResourcePolicyConfirmed,
    ResourcePolicyBlocked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
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

/// Inputs canonicalized through the ICU4X provider over revision "0"'s
/// artifact: small, scaled to a long identifier, and invalid at its first and
/// its last subtag.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Icu4xRecipe {
    Small,
    Alias,
    Correction,
    Scaled,
    InvalidSyntax,
    InvalidValidity,
    ScaledInvalid,
}

impl Icu4xRecipe {
    const ALL: [Self; 7] = [
        Self::Small,
        Self::Alias,
        Self::Correction,
        Self::Scaled,
        Self::InvalidSyntax,
        Self::InvalidValidity,
        Self::ScaledInvalid,
    ];

    pub(super) const fn spelling(self) -> &'static str {
        match self {
            Self::Small => "en-US",
            Self::Alias => "iw-IL",
            Self::Correction => "und-u-ca-islamicc",
            Self::Scaled => concat!(
                "EN-latn-us-scotland-fonipa-u-nu-latn-ms-metric-hc-h23-fw-mon-",
                "cu-usd-co-phonebk-ca-gregory-t-ja-m0-ungegn"
            ),
            Self::InvalidSyntax => "en_US",
            Self::InvalidValidity => "zz",
            // The scaled input, except that its last subtag is no `m0` type.
            Self::ScaledInvalid => concat!(
                "EN-latn-us-scotland-fonipa-u-nu-latn-ms-metric-hc-h23-fw-mon-",
                "cu-usd-co-phonebk-ca-gregory-t-ja-m0-ungegx"
            ),
        }
    }

    /// Owner-declared expected result, not learned from a provider invocation:
    /// ICU4X replaces the alias, revision "0"'s layer corrects the calendar
    /// type, and the scaled input is reordered and recased at its full length.
    pub(super) const fn canonical(self) -> Option<&'static str> {
        match self {
            Self::Small => Some("en-US"),
            Self::Alias => Some("he-IL"),
            Self::Correction => Some("und-u-ca-islamic-civil"),
            Self::Scaled => Some(concat!(
                "en-Latn-US-fonipa-scotland-t-ja-m0-ungegn-u-ca-gregory-",
                "co-phonebk-cu-usd-fw-mon-hc-h23-ms-metric-nu-latn"
            )),
            Self::InvalidSyntax | Self::InvalidValidity | Self::ScaledInvalid => None,
        }
    }
}

/// 026's engine state of a canonicalization case: one admitted engine reused
/// by every invocation, or a fresh engine admitted before each.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(super) enum Engine {
    Reused,
    Fresh,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
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
                Operation::LocaleCanonicalization
                | Operation::LocaleResolution
                | Operation::ResourcePolicyBootstrap => unreachable!(),
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
    // Append 015 Phase 7 without changing any of the 94 rows.
    let (before, after) =
        LocaleResolutionRecipe::ALL.split_at(LocaleResolutionRecipe::BEFORE_ICU4X);
    resolution_cases(
        &mut cases,
        before,
        &[
            (
                LimitKind::ResolutionLocaleOccurrences,
                LocaleResolutionRecipe::Multi,
            ),
            (
                LimitKind::ResolutionRequestedLocales,
                LocaleResolutionRecipe::Multi,
            ),
            (
                LimitKind::ResolutionRawIdentifierBytes,
                LocaleResolutionRecipe::Minimal,
            ),
            (
                LimitKind::ResolutionCanonicalIdentifierBytes,
                LocaleResolutionRecipe::ExpandingAlias,
            ),
        ],
    );
    // Append canonicalization through the ICU4X provider without changing any
    // of the 127 rows. Each input runs once reused and once fresh, so the two
    // engine states can be compared on the same input.
    for input in Icu4xRecipe::ALL {
        for engine in [Engine::Reused, Engine::Fresh] {
            cases.push(declaration(
                Operation::LocaleCanonicalization,
                Recipe::Icu4xLocale { input, engine },
                Selector::Absent,
                if input.canonical().is_some() {
                    ExpectedKind::LocaleCanonicalized
                } else {
                    ExpectedKind::LocaleRejected
                },
            ));
        }
    }
    // Append Phase 7 over every occurrence role without changing any of the
    // 141 rows.
    resolution_cases(
        &mut cases,
        after,
        &[(
            LimitKind::ResolutionLocaleOccurrences,
            LocaleResolutionRecipe::EveryRole,
        )],
    );
    // Append Phase 7 through the ICU4X provider without changing any of the
    // 150 rows. Each input runs once reused and once fresh.
    for input in LocaleResolutionRecipe::ICU4X {
        for engine in [Engine::Reused, Engine::Fresh] {
            cases.push(declaration(
                Operation::LocaleResolution,
                Recipe::Icu4xLocaleResolution { input, engine },
                Selector::App,
                if input.completes() {
                    ExpectedKind::LocaleResolutionComplete
                } else {
                    ExpectedKind::LocaleResolutionBlocked
                },
            ));
        }
    }
    // Append 015 Phase 4 without changing any of the 160 rows: every recipe,
    // then the exact and first-over edges of the capability's and the
    // policy's `maxArtifacts` and the policy's `maxProfileIdBytes`.
    let expected = |confirms| {
        if confirms {
            ExpectedKind::ResourcePolicyConfirmed
        } else {
            ExpectedKind::ResourcePolicyBlocked
        }
    };
    for recipe in ResourcePolicyRecipe::ALL {
        cases.push(declaration(
            Operation::ResourcePolicyBootstrap,
            Recipe::ResourcePolicy(recipe),
            Selector::App,
            expected(recipe.confirms()),
        ));
    }
    for (limit, recipe) in [
        (LimitKind::BootstrapArtifacts, ResourcePolicyRecipe::Mixed),
        (LimitKind::RecheckArtifacts, ResourcePolicyRecipe::Mixed),
        (
            LimitKind::RecheckProfileIdBytes,
            ResourcePolicyRecipe::PolicyOnly,
        ),
    ] {
        for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
            let mut case = declaration(
                Operation::ResourcePolicyBootstrap,
                Recipe::ResourcePolicy(recipe),
                Selector::App,
                expected(edge == LimitEdge::Exact),
            );
            case.limit = Some((limit, edge));
            cases.push(case);
        }
    }
    cases
}

/// One Phase 7 case per recipe, then the exact and first-over edges of each
/// bound on its recipe.
fn resolution_cases(
    cases: &mut Vec<Declaration>,
    recipes: &[LocaleResolutionRecipe],
    edges: &[(LimitKind, LocaleResolutionRecipe)],
) {
    for &recipe in recipes {
        cases.push(declaration(
            Operation::LocaleResolution,
            Recipe::LocaleResolution(recipe),
            Selector::App,
            if recipe.completes() {
                ExpectedKind::LocaleResolutionComplete
            } else {
                ExpectedKind::LocaleResolutionBlocked
            },
        ));
    }
    for &(limit, recipe) in edges {
        for edge in [LimitEdge::Exact, LimitEdge::FirstOver] {
            let mut case = declaration(
                Operation::LocaleResolution,
                Recipe::LocaleResolution(recipe),
                Selector::App,
                if edge == LimitEdge::Exact {
                    ExpectedKind::LocaleResolutionComplete
                } else {
                    ExpectedKind::LocaleResolutionBlocked
                },
            );
            case.limit = Some((limit, edge));
            cases.push(case);
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod locale_tests;

#[cfg(test)]
mod resolution_tests;

#[cfg(test)]
mod policy_tests;
