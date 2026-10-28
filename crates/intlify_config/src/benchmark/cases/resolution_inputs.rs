// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::profile_fixtures::{complete_config, minimal_config, reference};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum LocalePolicyResolutionRecipe {
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
    OccurrenceScale,
    CanonicalScale,
    AliasConflictingKeys,
    AliasRepeatedKeys,
    AliasUnrequestedDestination,
    FallbackUnrequestedSource,
    FallbackSelfReference,
    FallbackRepeatedLiteral,
    FallbackRepeatedIntentSource,
    CoverageUnrequestedLocale,
    CoverageRepeatedLocale,
    CoverageUnknownSurface,
    CoverageRepeatedSurface,
    CoverageRepeatedDomain,
    CoverageConflict,
    PolicyDense,
    PolicyReordered,
    AliasScale,
    FallbackSourceScale,
    FallbackSequenceScale,
    CoverageRuleScale,
}

impl LocalePolicyResolutionRecipe {
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

    /// The recipes resolved through the ICU4X provider: three the finite
    /// provider also answers, then the two scales only ICU4X has data for.
    pub(super) const ICU4X: [Self; 5] = [
        Self::EveryRole,
        Self::EveryRoleAliased,
        Self::EveryRoleInvalid,
        Self::OccurrenceScale,
        Self::CanonicalScale,
    ];

    /// Phase 8's workloads through the finite provider, after the Phase 9
    /// rows: one change to every role each, a dense declaration, and every
    /// role with its coverage rules and selector items in another order.
    pub(super) const POLICY: [Self; 15] = [
        Self::AliasConflictingKeys,
        Self::AliasRepeatedKeys,
        Self::AliasUnrequestedDestination,
        Self::FallbackUnrequestedSource,
        Self::FallbackSelfReference,
        Self::FallbackRepeatedLiteral,
        Self::FallbackRepeatedIntentSource,
        Self::CoverageUnrequestedLocale,
        Self::CoverageRepeatedLocale,
        Self::CoverageUnknownSurface,
        Self::CoverageRepeatedSurface,
        Self::CoverageRepeatedDomain,
        Self::CoverageConflict,
        Self::PolicyDense,
        Self::PolicyReordered,
    ];

    /// Phase 8's scales, one dimension each, through the ICU4X provider,
    /// which has the languages for them. Their Phase 7 counts fit the other
    /// recipes' capacity.
    pub(super) const POLICY_ICU4X: [Self; 4] = [
        Self::AliasScale,
        Self::FallbackSourceScale,
        Self::FallbackSequenceScale,
        Self::CoverageRuleScale,
    ];

    /// Whether the recipe needs more room than the other recipes' capacity.
    pub(super) const fn scales(self) -> bool {
        matches!(self, Self::OccurrenceScale | Self::CanonicalScale)
    }

    /// Whether no Phase 7 or Phase 8 unit finds a blocking result.
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
                | Self::CoverageUncanonicalized
                | Self::AliasConflictingKeys
                | Self::AliasRepeatedKeys
                | Self::AliasUnrequestedDestination
                | Self::FallbackUnrequestedSource
                | Self::FallbackSelfReference
                | Self::FallbackRepeatedLiteral
                | Self::FallbackRepeatedIntentSource
                | Self::CoverageUnrequestedLocale
                | Self::CoverageRepeatedLocale
                | Self::CoverageUnknownSurface
                | Self::CoverageRepeatedSurface
                | Self::CoverageRepeatedDomain
                | Self::CoverageConflict
                | Self::PolicyDense
        )
    }

    /// Whether the source file lists every object's members in reverse.
    pub(super) const fn reverses_members(self) -> bool {
        matches!(self, Self::Reordered | Self::EveryRoleReordered)
    }

    /// Complete structural inputs; Phases 7 and 8 resolve every locale
    /// spelling and locale policy of the selected declaration. No ambient
    /// configuration or data.
    pub(super) fn value(self) -> Value {
        match self {
            Self::EveryRole
            | Self::EveryRoleReordered
            | Self::EveryRoleAliased
            | Self::EveryRoleInvalid
            | Self::TargetDuplicate
            | Self::CoverageUncanonicalized
            | Self::AliasConflictingKeys
            | Self::AliasRepeatedKeys
            | Self::AliasUnrequestedDestination
            | Self::FallbackUnrequestedSource
            | Self::FallbackSelfReference
            | Self::FallbackRepeatedLiteral
            | Self::FallbackRepeatedIntentSource
            | Self::CoverageUnrequestedLocale
            | Self::CoverageRepeatedLocale
            | Self::CoverageUnknownSurface
            | Self::CoverageRepeatedSurface
            | Self::CoverageRepeatedDomain
            | Self::CoverageConflict
            | Self::PolicyDense
            | Self::PolicyReordered => return self.every_role_value(),
            Self::ManyTargets => return many_targets(32, 2),
            Self::EveryFixtureLocale => return every_fixture_locale(),
            Self::OccurrenceScale => return many_targets(256, 3),
            Self::CanonicalScale => return canonical_scale(),
            Self::AliasScale => return alias_scale(),
            Self::FallbackSourceScale => return fallback_source_scale(),
            Self::FallbackSequenceScale => return fallback_sequence_scale(),
            Self::CoverageRuleScale => return coverage_rule_scale(),
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
    /// 21 spellings, three of them coverage selectors. Each alias leads to a
    /// requested locale, so Phase 8 resolves the policies whole.
    fn every_role_value(self) -> Value {
        let mut value = complete_config();
        let app = &mut value["profiles"]["app"];
        app["requestedLocales"] = json!(["en", "ja", "he-IL"]);
        app["defaultRequestedLocale"] = json!("ja");
        app["defaultSourceLocale"] = json!("en");
        app["localeNegotiation"] = json!({"aliases": {"en-US": "en", "fr": "en"}});
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
                app["localeNegotiation"] = json!({"aliases": {"EN-us": "EN", "fr": "EN"}});
                app["messageFallback"] = json!({
                    "iw-IL": ["EN"],
                    "ja": ["EN", {"kind": "intent-source-locale"}]
                });
                app["targetProfiles"]["browser"]["requestedLocales"] = json!(["ja", "EN"]);
                app["targetProfiles"]["server"]["requestedLocales"] = json!(["iw-IL", "EN"]);
            }
            // An invalid spelling in each role beyond the project's own.
            Self::EveryRoleInvalid => {
                app["localeNegotiation"] = json!({"aliases": {"root": "zz", "fr": "en"}});
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
            // Phase 7 counts a coverage selector and leaves it to Phase 8,
            // which rejects it.
            Self::CoverageUncanonicalized => {
                app["coverage"]["rules"][0]["requestedLocales"] = json!(["zz"]);
            }
            // Two spellings of one key lead to different requested locales.
            Self::AliasConflictingKeys => {
                app["localeNegotiation"] = json!({"aliases": {"en-US": "en", "EN-us": "ja"}});
            }
            // Two spellings of one key lead to the same locale.
            Self::AliasRepeatedKeys => {
                app["localeNegotiation"] = json!({"aliases": {"en-US": "en", "EN-us": "en"}});
            }
            // The complete configuration's alias, to a locale not requested.
            Self::AliasUnrequestedDestination => {
                app["localeNegotiation"] = json!({"aliases": {"en-US": "en", "fr": "fr-FR"}});
            }
            Self::FallbackUnrequestedSource => {
                app["messageFallback"]["de"] = json!(["en"]);
                app["messageFallback"]
                    .as_object_mut()
                    .unwrap()
                    .remove("he-IL");
            }
            Self::FallbackSelfReference => app["messageFallback"]["he-IL"] = json!(["he-IL"]),
            // `EN` is another spelling of `en`.
            Self::FallbackRepeatedLiteral => app["messageFallback"]["he-IL"] = json!(["en", "EN"]),
            Self::FallbackRepeatedIntentSource => {
                app["messageFallback"]["ja"] = json!([
                    "en",
                    {"kind": "intent-source-locale"},
                    {"kind": "intent-source-locale"}
                ]);
            }
            Self::CoverageUnrequestedLocale => {
                app["coverage"]["rules"][0]["requestedLocales"] = json!(["fr"]);
            }
            Self::CoverageRepeatedLocale => {
                app["coverage"]["rules"][0]["requestedLocales"] = json!(["ja", "ja"]);
            }
            Self::CoverageUnknownSurface => {
                app["coverage"]["rules"][1]["intentSurfaceClasses"] = json!(["footer"]);
            }
            Self::CoverageRepeatedSurface => {
                app["coverage"]["rules"][1]["intentSurfaceClasses"] =
                    json!(["checkout", "checkout"]);
            }
            // The `ja` rule again, its surface classes spelled out.
            Self::CoverageRepeatedDomain => push_rule(
                app,
                json!({
                    "requestedLocales": ["ja"],
                    "intentSurfaceClasses": ["checkout", "internal-tool", "nav"],
                    "mode": "direct-required"
                }),
            ),
            // Neither this rule nor the `ja` rule is narrower, and their modes
            // differ at `ja` / `nav`.
            Self::CoverageConflict => push_rule(app, conflicting_rule()),
            // A key two spellings name, a sequence naming its source and
            // repeating the Intent source, a surface class the vocabulary
            // lacks, and a conflicting rule, at once. The unknown surface class
            // leaves every cell undecided, the conflicting one included.
            Self::PolicyDense => {
                app["localeNegotiation"] = json!({"aliases": {"en-US": "en", "EN-us": "ja"}});
                app["messageFallback"] = json!({
                    "he-IL": ["he-IL"],
                    "ja": ["en", {"kind": "intent-source-locale"}, {"kind": "intent-source-locale"}]
                });
                app["coverage"]["rules"][1]["intentSurfaceClasses"] = json!(["footer"]);
                push_rule(app, conflicting_rule());
            }
            // The rules and each selector in another order: the order they were
            // authored in decides nothing.
            Self::PolicyReordered => {
                let rules = app["coverage"]["rules"].as_array_mut().unwrap();
                rules.reverse();
                rules[0]["requestedLocales"] = json!(["he-IL", "ja"]);
            }
            _ => unreachable!("only every role recipes reach here"),
        }
        value
    }
}

/// `count` targets requesting one locale each, named with `width` digits:
/// occurrences scale, the canonical locales do not.
fn many_targets(count: usize, width: usize) -> Value {
    let mut value = minimal_config();
    let app = &mut value["profiles"]["app"];
    let ids = (0..count)
        .map(|index| format!("t{index:0width$}"))
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

/// The first 128 two-letter ISO 639-1 languages but `bh`, which CLDR
/// replaces: each is its own canonical form in revision "0". They are codes,
/// not words, so the spell checker skips the lines it would misread.
pub(super) const SCALE_LANGUAGES: &str = concat!(
    "aa ab ae af ak am an ar as av ay az ba be bg bi bm bn bo br bs ca ce ch ",
    "co cr cs cu cv cy da de dv dz ee el en eo es et eu fa ff fi fj fo fr fy ", // spellchecker:disable-line
    "ga gd gl gn gu gv ha he hi ho hr ht hu hy hz ia id ie ig ii ik io is it ",
    "iu ja jv ka kg ki kj kk kl km kn ko kr ks ku kv kw ky la lb lg li ln lo ",
    "lt lu lv mg mh mi mk ml mn mr ms mt my na nb nd ne ng nl nn no nr nv ny ", // spellchecker:disable-line
    "oc oj om or os pa pi pl"
);

fn push_rule(app: &mut Value, rule: Value) {
    app["coverage"]["rules"].as_array_mut().unwrap().push(rule);
}

/// A rule over `en` and `ja` for `nav` alone, in the mode the `ja` rule does
/// not use.
fn conflicting_rule() -> Value {
    json!({
        "requestedLocales": ["en", "ja"],
        "intentSurfaceClasses": ["nav"],
        "mode": "direct-required"
    })
}

/// The minimal declaration requesting `requested`, the first its default and
/// its target's request.
fn requesting(requested: &[&str]) -> Value {
    let mut value = minimal_config();
    let app = &mut value["profiles"]["app"];
    app["requestedLocales"] = json!(requested);
    app["defaultRequestedLocale"] = json!(requested[0]);
    app["targetProfiles"]["browser"]["requestedLocales"] = json!([requested[0]]);
    value
}

fn scale_languages() -> Vec<&'static str> {
    SCALE_LANGUAGES.split(' ').collect()
}

/// 24 aliases, each from one of the first languages to `en`.
fn alias_scale() -> Value {
    let mut value = requesting(&["en"]);
    value["profiles"]["app"]["localeNegotiation"] = json!({
        "aliases": scale_languages()[..24]
            .iter()
            .map(|language| ((*language).to_owned(), json!("en")))
            .collect::<serde_json::Map<_, _>>()
    });
    value
}

/// 40 requested sources, each falling back to `ja`.
fn fallback_source_scale() -> Value {
    let languages = scale_languages();
    let mut value = requesting(&languages[..40]);
    value["profiles"]["app"]["messageFallback"] = languages[..40]
        .iter()
        .map(|language| ((*language).to_owned(), json!(["ja"])))
        .collect::<serde_json::Map<_, _>>()
        .into();
    value
}

/// One source, `ja`, falling back through 48 other languages in turn.
fn fallback_sequence_scale() -> Value {
    let candidates = scale_languages()
        .into_iter()
        .filter(|language| *language != "ja")
        .take(48)
        .collect::<Vec<_>>();
    let mut value = requesting(&["ja"]);
    value["profiles"]["app"]["messageFallback"] = json!({"ja": candidates});
    value
}

/// 60 requested languages, each with a rule of its own: 180 cells, each
/// compared with every rule.
fn coverage_rule_scale() -> Value {
    let languages = scale_languages();
    let mut value = requesting(&languages[..60]);
    value["profiles"]["app"]["coverage"] = json!({
        "defaultMode": "direct-required",
        "rules": languages[..60]
            .iter()
            .map(|language| json!({"requestedLocales": [language], "mode": "fallback-allowed"}))
            .collect::<Vec<_>>()
    });
    value
}

/// The 128 languages, requested by the project and by its target: 257
/// spellings, and as many canonical locales as requested ones.
fn canonical_scale() -> Value {
    let locales = SCALE_LANGUAGES.split(' ').collect::<Vec<_>>();
    let mut value = minimal_config();
    let app = &mut value["profiles"]["app"];
    app["requestedLocales"] = json!(locales);
    app["targetProfiles"]["browser"]["requestedLocales"] = json!(locales);
    value
}
