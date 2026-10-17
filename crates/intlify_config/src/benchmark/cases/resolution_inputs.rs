// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::profile_fixtures::{complete_config, minimal_config, reference};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum LocaleResolutionRecipe {
    Minimal,
    SourceUnd,
    SourceEn,
    Multi,
    Reordered,
    Aliased,
    ChangedSet,
    ChangedDefault,
    ChangedSource,
    AbsentSource,
    ExactDuplicate,
    AliasDuplicate,
    MultipleDuplicates,
    DuplicateHeavy,
    InvalidSource,
    InvalidRequested,
    InvalidDefault,
    InvalidAndDuplicate,
    DefaultNotMember,
    EveryRole,
    OtherProfile,
    DuplicateReordered,
    InvalidReordered,
    ExpandingAlias,
    ExpandedAlias,
    EveryRoleReordered,
    EveryRoleAliased,
    EveryRoleInvalid,
    TargetDuplicate,
    CoverageUncanonicalized,
    ManyTargets,
    EveryFixtureLocale,
}

impl LocaleResolutionRecipe {
    pub(super) const ALL: [Self; 32] = [
        Self::Minimal,
        Self::SourceUnd,
        Self::SourceEn,
        Self::Multi,
        Self::Reordered,
        Self::Aliased,
        Self::ChangedSet,
        Self::ChangedDefault,
        Self::ChangedSource,
        Self::AbsentSource,
        Self::ExactDuplicate,
        Self::AliasDuplicate,
        Self::MultipleDuplicates,
        Self::DuplicateHeavy,
        Self::InvalidSource,
        Self::InvalidRequested,
        Self::InvalidDefault,
        Self::InvalidAndDuplicate,
        Self::DefaultNotMember,
        Self::EveryRole,
        Self::OtherProfile,
        Self::DuplicateReordered,
        Self::InvalidReordered,
        Self::ExpandingAlias,
        Self::ExpandedAlias,
        Self::EveryRoleReordered,
        Self::EveryRoleAliased,
        Self::EveryRoleInvalid,
        Self::TargetDuplicate,
        Self::CoverageUncanonicalized,
        Self::ManyTargets,
        Self::EveryFixtureLocale,
    ];

    /// How many recipes have rows before the ICU4X rows; the rest follow them.
    pub(super) const BEFORE_ICU4X: usize = 25;

    /// Whether no Phase 7 unit finds a blocking result.
    pub(super) const fn completes(self) -> bool {
        !matches!(
            self,
            Self::ExactDuplicate
                | Self::AliasDuplicate
                | Self::MultipleDuplicates
                | Self::DuplicateHeavy
                | Self::InvalidSource
                | Self::InvalidRequested
                | Self::InvalidDefault
                | Self::InvalidAndDuplicate
                | Self::DefaultNotMember
                | Self::DuplicateReordered
                | Self::InvalidReordered
                | Self::EveryRoleInvalid
                | Self::TargetDuplicate
        )
    }

    /// Whether the source file lists every object's members in reverse.
    pub(super) const fn reverses_members(self) -> bool {
        matches!(self, Self::Reordered | Self::EveryRoleReordered)
    }

    /// Complete structural inputs; Phase 7 resolves every locale spelling of
    /// the selected declaration. No ambient configuration or data.
    pub(super) fn value(self) -> Value {
        match self {
            Self::EveryRole
            | Self::EveryRoleReordered
            | Self::EveryRoleAliased
            | Self::EveryRoleInvalid
            | Self::TargetDuplicate
            | Self::CoverageUncanonicalized => return self.every_role_value(),
            Self::ManyTargets => return many_targets(),
            Self::EveryFixtureLocale => return every_fixture_locale(),
            _ => {}
        }
        let mut value = minimal_config();
        if matches!(
            self,
            Self::Multi
                | Self::Reordered
                | Self::Aliased
                | Self::ChangedSet
                | Self::ChangedDefault
                | Self::ChangedSource
                | Self::AbsentSource
        ) {
            let app = &mut value["profiles"]["app"];
            app["requestedLocales"] = json!(["en", "en-US", "he-IL", "ja"]);
            app["defaultRequestedLocale"] = json!("en-US");
            app["defaultSourceLocale"] = json!("en");
        }
        let app = &mut value["profiles"]["app"];
        match self {
            Self::Minimal | Self::Multi => {}
            Self::SourceUnd => app["defaultSourceLocale"] = json!("und"),
            Self::SourceEn => app["defaultSourceLocale"] = json!("en"),
            Self::Reordered => app["requestedLocales"] = json!(["ja", "he-IL", "en-US", "en"]),
            Self::Aliased => {
                app["requestedLocales"] = json!(["ja", "iw-IL", "EN-us", "EN"]);
                app["defaultRequestedLocale"] = json!("EN-us");
                app["defaultSourceLocale"] = json!("EN");
            }
            Self::ChangedSet => app["requestedLocales"] = json!(["en-US", "ja"]),
            Self::ChangedDefault => app["defaultRequestedLocale"] = json!("ja"),
            Self::ChangedSource => app["defaultSourceLocale"] = json!("fr"),
            Self::AbsentSource => {
                app.as_object_mut().unwrap().remove("defaultSourceLocale");
            }
            Self::ExactDuplicate => app["requestedLocales"] = json!(["en", "en"]),
            Self::AliasDuplicate => app["requestedLocales"] = json!(["EN", "en"]),
            Self::MultipleDuplicates | Self::DuplicateReordered => {
                app["requestedLocales"] = if self == Self::MultipleDuplicates {
                    json!(["ja", "EN-us", "EN", "en-US", "ja", "en"])
                } else {
                    json!(["en", "ja", "en-US", "EN", "EN-us", "ja"])
                };
                app["defaultSourceLocale"] = json!("en");
            }
            Self::DuplicateHeavy => app["requestedLocales"] = json!(vec!["en"; 32]),
            Self::InvalidSource => app["defaultSourceLocale"] = json!("en_US"),
            Self::InvalidRequested => app["requestedLocales"] = json!(["zz"]),
            Self::InvalidDefault => app["defaultRequestedLocale"] = json!("en_US"),
            Self::InvalidAndDuplicate | Self::InvalidReordered => {
                app["defaultSourceLocale"] = json!("en_US");
                app["requestedLocales"] = if self == Self::InvalidAndDuplicate {
                    json!(["zz", "EN-us", "en-US"])
                } else {
                    json!(["en-US", "zz", "EN-us"])
                };
                app["defaultRequestedLocale"] = json!("fr");
            }
            Self::DefaultNotMember => app["defaultRequestedLocale"] = json!("fr"),
            Self::OtherProfile => {
                let mut other = app.clone();
                other["requestedLocales"] = json!(["zz"]);
                other["defaultRequestedLocale"] = json!("zz");
                value["profiles"]["other"] = other;
            }
            Self::ExpandingAlias | Self::ExpandedAlias => {
                let locale = if self == Self::ExpandingAlias {
                    "und-u-ca-islamicc"
                } else {
                    "und-u-ca-islamic-civil"
                };
                app["requestedLocales"] = json!([locale]);
                app["defaultRequestedLocale"] = json!(locale);
            }
            _ => unreachable!("the other recipes return early"),
        }
        value
    }

    /// A declaration with a spelling in each of 015's ten occurrence roles:
    /// 21 spellings, three of them coverage selectors.
    fn every_role_value(self) -> Value {
        let mut value = complete_config();
        let app = &mut value["profiles"]["app"];
        app["requestedLocales"] = json!(["en", "ja", "he-IL"]);
        app["defaultRequestedLocale"] = json!("ja");
        app["defaultSourceLocale"] = json!("en");
        app["localeNegotiation"] = json!({"aliases": {"en-US": "en", "fr": "fr-FR"}});
        app["messageFallback"] = json!({
            "he-IL": ["en"],
            "ja": ["en", {"kind": "intent-source-locale"}]
        });
        app["coverage"]["rules"][2]["requestedLocales"] = json!(["ja", "he-IL"]);
        let browser = &mut app["targetProfiles"]["browser"];
        browser["requestedLocales"] = json!(["ja", "en"]);
        browser["defaultRequestedLocale"] = json!("ja");
        app["targetProfiles"]["server"]["requestedLocales"] = json!(["he-IL", "en"]);
        match self {
            Self::EveryRole => {}
            // Every set in another order; the fallback sequences keep theirs.
            Self::EveryRoleReordered => {
                app["requestedLocales"] = json!(["he-IL", "en", "ja"]);
                app["coverage"]["rules"][2]["requestedLocales"] = json!(["he-IL", "ja"]);
                app["targetProfiles"]["browser"]["requestedLocales"] = json!(["en", "ja"]);
                app["targetProfiles"]["server"]["requestedLocales"] = json!(["en", "he-IL"]);
            }
            // The same locales, each canonicalized role spelled by an alias.
            Self::EveryRoleAliased => {
                app["requestedLocales"] = json!(["EN", "ja", "iw-IL"]);
                app["defaultSourceLocale"] = json!("EN");
                app["localeNegotiation"] = json!({"aliases": {"EN-us": "EN", "fr": "fr-FR"}});
                app["messageFallback"] = json!({
                    "iw-IL": ["EN"],
                    "ja": ["EN", {"kind": "intent-source-locale"}]
                });
                app["targetProfiles"]["browser"]["requestedLocales"] = json!(["ja", "EN"]);
                app["targetProfiles"]["server"]["requestedLocales"] = json!(["iw-IL", "EN"]);
            }
            // An invalid spelling in each role beyond the project's own.
            Self::EveryRoleInvalid => {
                app["localeNegotiation"] = json!({"aliases": {"root": "zz", "fr": "fr-FR"}});
                app["messageFallback"] = json!({
                    "en_US": ["en-x-brand"],
                    "ja": ["en", {"kind": "intent-source-locale"}]
                });
                let browser = &mut app["targetProfiles"]["browser"];
                browser["requestedLocales"] = json!(["ja", "Latn"]);
                browser["defaultRequestedLocale"] = json!("en-u-ca-madeup");
            }
            Self::TargetDuplicate => {
                app["targetProfiles"]["server"]["requestedLocales"] =
                    json!(["he-IL", "iw-IL", "en"]);
            }
            // Phase 7 counts a coverage selector and leaves it to its owner.
            Self::CoverageUncanonicalized => {
                app["coverage"]["rules"][0]["requestedLocales"] = json!(["zz"]);
            }
            _ => unreachable!("only every role recipes reach here"),
        }
        value
    }
}

/// 32 targets requesting one locale each: occurrences scale, the canonical
/// locales do not.
fn many_targets() -> Value {
    let mut value = minimal_config();
    let app = &mut value["profiles"]["app"];
    let ids = (0..32)
        .map(|index| format!("t{index:02}"))
        .collect::<Vec<_>>();
    app["targetProfiles"] = ids
        .iter()
        .map(|id| {
            let target = json!({
                "profile": reference("target-profile"),
                "requestedLocales": ["en"]
            });
            (id.clone(), target)
        })
        .collect();
    app["deploymentGroups"]["web"]["members"] = json!(ids);
    value
}

/// Every canonical locale of the finite provider, requested by the project in
/// reverse and by its target in order: the canonical locales scale.
fn every_fixture_locale() -> Value {
    let locales = [
        "ar-EG-u-nu-latn",
        "de",
        "de-DE",
        "en",
        "en-US",
        "en-u-ca-gregory-nu-latn",
        "fr",
        "fr-FR",
        "he-IL",
        "ja",
        "und",
        "und-u-ca-islamic-civil",
        "zh-Hant-TW",
    ];
    let mut value = minimal_config();
    let app = &mut value["profiles"]["app"];
    app["requestedLocales"] = json!(locales.iter().rev().collect::<Vec<_>>());
    app["targetProfiles"]["browser"]["requestedLocales"] = json!(locales);
    value
}
