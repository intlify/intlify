// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::profile_fixtures::{complete_config, minimal_config};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "kebab-case")]
pub(in crate::benchmark) enum LocaleCoreRecipe {
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
    UnrelatedFields,
    OtherProfile,
    DuplicateReordered,
    InvalidReordered,
    ExpandingAlias,
    ExpandedAlias,
}

impl LocaleCoreRecipe {
    pub(super) const ALL: [Self; 25] = [
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
        Self::UnrelatedFields,
        Self::OtherProfile,
        Self::DuplicateReordered,
        Self::InvalidReordered,
        Self::ExpandingAlias,
        Self::ExpandedAlias,
    ];

    pub(super) const fn resolves(self) -> bool {
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
        )
    }

    /// Complete structural inputs; only the three project locale roles are
    /// semantically resolved by this boundary. No ambient configuration or data.
    pub(super) fn value(self) -> Value {
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
            Self::UnrelatedFields => value = complete_config(),
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
        }
        value
    }
}
