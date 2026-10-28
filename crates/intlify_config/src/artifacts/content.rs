// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The closed contents design 017 defines: the minimum behavior of the four
//! Policy kinds that 021, 018, and 022 fix, and the dimensions of the Target
//! Profile that 024 fixes for the minimum Web text profile. 017 adds no
//! expression language, so a content is a closed object of fixed spellings,
//! exact tokens, and positive quantities.
//!
//! A content is read only after its envelope's integrity holds and its
//! specification revision is supported. The contents of a Resource Limit
//! Policy, a Trust Policy, and a Glossary Set are 018's and 022's to admit, so
//! they pass on as the objects they are. Whether the directions of a Target
//! Profile cover every requested locale depends on the targets that reference
//! it, so it is not decided here.

use std::num::NonZeroU64;

use intlify_shared_json::quantity::Quantity;
use intlify_shared_json::token::{Token, VersionedIdentity};
use serde_json::{Map, Value};

use super::envelope::ArtifactKind;
use super::{closed, exactly, token, SemanticDigest};

/// The members of each closed object, in the unsigned byte order of their
/// names.
const VERSIONED: [&str; 2] = ["identity", "revision"];
const PROVIDER_ROUTING: [&str; 2] = ["adapter", "configuration"];
const CONFIGURATION: [&str; 2] = ["identity", "semanticDigest"];
const TARGET_PROFILE: [&str; 9] = [
    "adapter",
    "codecs",
    "delivery",
    "execution",
    "host",
    "limits",
    "locale",
    "mf2Specification",
    "mode",
];
const HOST: [&str; 3] = ["grammar", "kind", "moduleProfile"];
const EXECUTION: [&str; 2] = ["profile", "specification"];
const CODECS: [&str; 4] = ["bindingTable", "descriptor", "localePayload", "sourceMap"];
const LOCALE: [&str; 3] = ["directions", "selection", "services"];
const DIRECTION: [&str; 2] = ["direction", "locale"];
const DELIVERY: [&str; 2] = ["loading", "unit"];
const LIMITS: [&str; 8] = [
    "maxFileBytes",
    "maxFiles",
    "maxLocales",
    "maxMessages",
    "maxOutputBytes",
    "maxParameterBytes",
    "maxTextBytes",
    "maxUses",
];

/// The values 017 fixes for the minimum Web text profile in this revision.
const HOST_KIND: &str = "javascript-esm-dom-text";
const EXECUTION_SPECIFICATION: (&str, &str) = ("intlify-design-023", "0");
const EXECUTION_PROFILE: (&str, &str) = ("intlify-text-interpolation-minimum", "0");
const SOURCE_MAP: (&str, &str) = ("ecma-426-source-map", "3");

/// An admitted content, typed by its kind. The values 017 fixes are not
/// kept, since every admitted content has them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Content<'a> {
    Approval(LocalizedApproval),
    /// The one selection mode, `explicit`.
    Selection,
    SourceAdmission(SourceApproval),
    ProviderRouting(ProviderRouting),
    TargetProfile(Box<TargetProfile>),
    /// The content of a Resource Limit Policy, a Trust Policy, or a Glossary
    /// Set, for 018 and 022 to admit.
    External(&'a Map<String, Value>),
}

/// Whether a localized message needs an approval record before it is selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalizedApproval {
    Required,
    NotRequired,
}

/// What admits a source message: an approval record, or its authentication
/// alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceApproval {
    Required,
    AuthenticatedSufficient,
}

/// The Provider adapter, and the configuration it is routed with, pinned by
/// its semantic digest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProviderRouting {
    pub(crate) adapter: VersionedIdentity,
    pub(crate) configuration: Token,
    pub(crate) configuration_digest: SemanticDigest,
}

/// The dimensions of a Target Profile that 017 leaves to the profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetProfile {
    pub(crate) grammar: VersionedIdentity,
    pub(crate) module_profile: VersionedIdentity,
    pub(crate) mode: TargetMode,
    pub(crate) adapter: VersionedIdentity,
    pub(crate) mf2_specification: VersionedIdentity,
    pub(crate) binding_table: VersionedIdentity,
    pub(crate) locale_payload: VersionedIdentity,
    pub(crate) descriptor: VersionedIdentity,
    /// Ordered by the unsigned bytes of the locales, one per locale.
    pub(crate) directions: Vec<LocaleDirection>,
    pub(crate) limits: TargetLimits,
}

/// The two execution modes of the minimum Web text profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TargetMode {
    RuntimeBacked,
    Aot,
}

/// The direction a locale's messages are written in. It is an input, never
/// inferred from the text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocaleDirection {
    pub(crate) locale: String,
    pub(crate) direction: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    Ltr,
    Rtl,
    Unknown,
}

/// The largest number of each that the profile allows: `maxMessages` is
/// `messages`, and so on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct TargetLimits {
    pub(crate) messages: NonZeroU64,
    pub(crate) uses: NonZeroU64,
    pub(crate) locales: NonZeroU64,
    pub(crate) files: NonZeroU64,
    pub(crate) file_bytes: NonZeroU64,
    pub(crate) output_bytes: NonZeroU64,
    pub(crate) parameter_bytes: NonZeroU64,
    pub(crate) text_bytes: NonZeroU64,
}

/// The content of `kind` that `content` is, or `None` when it is not the
/// kind's closed content.
pub(crate) fn read(kind: ArtifactKind, content: &Map<String, Value>) -> Option<Content<'_>> {
    match kind {
        ArtifactKind::ApprovalPolicy => approval(content).map(Content::Approval),
        ArtifactKind::SelectionPolicy => selection(content).then_some(Content::Selection),
        ArtifactKind::SourceAdmissionPolicy => {
            source_admission(content).map(Content::SourceAdmission)
        }
        ArtifactKind::ProviderRoutingPolicy => {
            provider_routing(content).map(Content::ProviderRouting)
        }
        ArtifactKind::TargetProfile => {
            target_profile(content).map(|profile| Content::TargetProfile(Box::new(profile)))
        }
        ArtifactKind::GlossarySet
        | ArtifactKind::ResourceLimitPolicy
        | ArtifactKind::TrustPolicy => Some(Content::External(content)),
    }
}

/// The string value of `content`'s one member, when `name` is all it has.
fn only<'a>(content: &'a Map<String, Value>, name: &str) -> Option<&'a str> {
    if exactly(content, &[name]) {
        content[name].as_str()
    } else {
        None
    }
}

fn approval(content: &Map<String, Value>) -> Option<LocalizedApproval> {
    match only(content, "localizedApproval")? {
        "required" => Some(LocalizedApproval::Required),
        "not-required" => Some(LocalizedApproval::NotRequired),
        _ => None,
    }
}

fn selection(content: &Map<String, Value>) -> bool {
    only(content, "mode") == Some("explicit")
}

fn source_admission(content: &Map<String, Value>) -> Option<SourceApproval> {
    match only(content, "sourceApproval")? {
        "required" => Some(SourceApproval::Required),
        "authenticated-sufficient" => Some(SourceApproval::AuthenticatedSufficient),
        _ => None,
    }
}

fn provider_routing(content: &Map<String, Value>) -> Option<ProviderRouting> {
    if !exactly(content, &PROVIDER_ROUTING) {
        return None;
    }
    let configuration = closed(&content["configuration"], &CONFIGURATION)?;
    Some(ProviderRouting {
        adapter: versioned(&content["adapter"])?,
        configuration: token(&configuration["identity"])?,
        configuration_digest: configuration["semanticDigest"]
            .as_str()
            .and_then(|digest| SemanticDigest::from_validated(digest).ok())?,
    })
}

fn target_profile(content: &Map<String, Value>) -> Option<TargetProfile> {
    if !exactly(content, &TARGET_PROFILE) {
        return None;
    }
    let host = closed(&content["host"], &HOST)?;
    let execution = closed(&content["execution"], &EXECUTION)?;
    let codecs = closed(&content["codecs"], &CODECS)?;
    let locale = closed(&content["locale"], &LOCALE)?;
    let delivery = closed(&content["delivery"], &DELIVERY)?;
    let fixed = host["kind"] == HOST_KIND
        && pinned(&execution["specification"], EXECUTION_SPECIFICATION)
        && pinned(&execution["profile"], EXECUTION_PROFILE)
        && pinned(&codecs["sourceMap"], SOURCE_MAP)
        && locale["selection"] == "direct-only"
        && locale["services"] == "absent"
        && delivery["unit"] == "main"
        && delivery["loading"] == "eager";
    if !fixed {
        return None;
    }
    let mode = match content["mode"].as_str()? {
        "runtime-backed" => TargetMode::RuntimeBacked,
        "aot" => TargetMode::Aot,
        _ => return None,
    };
    Some(TargetProfile {
        grammar: versioned(&host["grammar"])?,
        module_profile: versioned(&host["moduleProfile"])?,
        mode,
        adapter: versioned(&content["adapter"])?,
        mf2_specification: versioned(&content["mf2Specification"])?,
        binding_table: versioned(&codecs["bindingTable"])?,
        locale_payload: versioned(&codecs["localePayload"])?,
        descriptor: versioned(&codecs["descriptor"])?,
        directions: directions(&locale["directions"])?,
        limits: limits(&content["limits"])?,
    })
}

/// The closed `{ identity, revision }` pair `value` is.
fn versioned(value: &Value) -> Option<VersionedIdentity> {
    let pair = closed(value, &VERSIONED)?;
    VersionedIdentity::new(pair["identity"].as_str()?, pair["revision"].as_str()?).ok()
}

/// Whether `value` is exactly the pair 017 fixes.
fn pinned(value: &Value, (identity, revision): (&str, &str)) -> bool {
    closed(value, &VERSIONED)
        .is_some_and(|pair| pair["identity"] == identity && pair["revision"] == revision)
}

fn directions(value: &Value) -> Option<Vec<LocaleDirection>> {
    let directions = value
        .as_array()?
        .iter()
        .map(|entry| {
            let entry = closed(entry, &DIRECTION)?;
            let locale = entry["locale"]
                .as_str()
                .filter(|locale| !locale.is_empty())?;
            let direction = match entry["direction"].as_str()? {
                "LTR" => Direction::Ltr,
                "RTL" => Direction::Rtl,
                "unknown" => Direction::Unknown,
                _ => return None,
            };
            Some(LocaleDirection {
                locale: locale.to_owned(),
                direction,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    // Strictly ascending bytes are both the order and the absence of repeats,
    // whatever direction a repeated locale is given.
    directions
        .windows(2)
        .all(|pair| pair[0].locale.as_bytes() < pair[1].locale.as_bytes())
        .then_some(directions)
}

fn limits(value: &Value) -> Option<TargetLimits> {
    let limits = closed(value, &LIMITS)?;
    let bound = |name: &str| {
        let quantity = Quantity::parse(limits[name].as_str()?).ok()?;
        NonZeroU64::new(quantity.get())
    };
    Some(TargetLimits {
        messages: bound("maxMessages")?,
        uses: bound("maxUses")?,
        locales: bound("maxLocales")?,
        files: bound("maxFiles")?,
        file_bytes: bound("maxFileBytes")?,
        output_bytes: bound("maxOutputBytes")?,
        parameter_bytes: bound("maxParameterBytes")?,
        text_bytes: bound("maxTextBytes")?,
    })
}

/// An admitted content of `kind`, for tests, vectors, and measurements. The
/// values are test values, not artifacts any project admits.
#[cfg(any(test, feature = "vectors", feature = "benchmark"))]
pub(crate) fn sample(kind: ArtifactKind) -> Value {
    use serde_json::json;

    let versioned =
        |identity: &str, revision: &str| json!({"identity": identity, "revision": revision});
    match kind {
        ArtifactKind::ApprovalPolicy => json!({"localizedApproval": "required"}),
        ArtifactKind::GlossarySet | ArtifactKind::TrustPolicy => json!({}),
        ArtifactKind::ProviderRoutingPolicy => json!({
            "adapter": versioned("intlify-provider-fixture", "0"),
            "configuration": {
                "identity": "storefront-routing",
                "semanticDigest": SemanticDigest::from_hash([0x2a; 32]).as_str(),
            },
        }),
        ArtifactKind::ResourceLimitPolicy => json!({
            "projectProfileResolution": {"configurationInput": {"maxConfigurationBytes": "1048576"}},
        }),
        ArtifactKind::SelectionPolicy => json!({"mode": "explicit"}),
        ArtifactKind::SourceAdmissionPolicy => {
            json!({"sourceApproval": "authenticated-sufficient"})
        }
        ArtifactKind::TargetProfile => json!({
            "host": {
                "kind": HOST_KIND,
                "grammar": versioned("ecmascript", "2025"),
                "moduleProfile": versioned("intlify-esm-module", "0"),
            },
            "mode": "runtime-backed",
            "adapter": versioned("intlify-vanilla-dom", "0"),
            "execution": {
                "specification": versioned(EXECUTION_SPECIFICATION.0, EXECUTION_SPECIFICATION.1),
                "profile": versioned(EXECUTION_PROFILE.0, EXECUTION_PROFILE.1),
            },
            "mf2Specification": versioned("unicode-mf2", "48"),
            "codecs": {
                "bindingTable": versioned("intlify-binding-table", "0"),
                "localePayload": versioned("intlify-locale-payload", "0"),
                "descriptor": versioned("intlify-target-descriptor", "0"),
                "sourceMap": versioned(SOURCE_MAP.0, SOURCE_MAP.1),
            },
            "locale": {
                "selection": "direct-only",
                "services": "absent",
                "directions": [
                    {"locale": "en", "direction": "LTR"},
                    {"locale": "ja", "direction": "LTR"},
                ],
            },
            "delivery": {"unit": "main", "loading": "eager"},
            "limits": {
                "maxMessages": "1000",
                "maxUses": "10000",
                "maxLocales": "8",
                "maxFiles": "64",
                "maxFileBytes": "1048576",
                "maxOutputBytes": "8388608",
                "maxParameterBytes": "4096",
                "maxTextBytes": "65536",
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn read_value(kind: ArtifactKind, content: &Value) -> Option<Content<'_>> {
        read(kind, content.as_object().unwrap())
    }

    /// `value` with the member at `pointer` set, or removed for `None`.
    fn edited(value: &Value, pointer: &str, replacement: Option<Value>) -> Value {
        let mut edited = value.clone();
        let (parent, name) = pointer.rsplit_once('/').unwrap();
        let object = edited.pointer_mut(parent).unwrap().as_object_mut().unwrap();
        match replacement {
            Some(replacement) => object.insert(name.into(), replacement),
            None => object.remove(name),
        };
        edited
    }

    fn profile_of(content: &Value) -> Option<TargetProfile> {
        match read_value(ArtifactKind::TargetProfile, content)? {
            Content::TargetProfile(profile) => Some(*profile),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn each_policy_content_admits_exactly_its_closed_values() {
        let cases = [
            (
                ArtifactKind::ApprovalPolicy,
                "localizedApproval",
                ["required", "not-required"].as_slice(),
            ),
            (
                ArtifactKind::SelectionPolicy,
                "mode",
                ["explicit"].as_slice(),
            ),
            (
                ArtifactKind::SourceAdmissionPolicy,
                "sourceApproval",
                ["required", "authenticated-sufficient"].as_slice(),
            ),
        ];
        for (kind, name, values) in cases {
            for value in values {
                assert!(
                    read_value(kind, &json!({name: value})).is_some(),
                    "{name}: {value}"
                );
            }
            // Spellings are exact; other values and types are refused.
            for other in [
                json!("Required"),
                json!("not_required"),
                json!("Explicit"),
                json!("implicit"),
                json!("authenticated"),
                json!("required "),
                json!(""),
                json!(null),
                json!(true),
                json!(["required"]),
                json!({"value": "required"}),
            ] {
                assert!(
                    read_value(kind, &json!({name: other})).is_none(),
                    "{name}: {other}"
                );
            }
            // The one member is required, and no other is allowed.
            assert!(read_value(kind, &json!({})).is_none(), "{name}");
            assert!(
                read_value(kind, &json!({name: values[0], "note": "x"})).is_none(),
                "{name}"
            );
        }
        for (kind, name, value, expected) in [
            (
                ArtifactKind::ApprovalPolicy,
                "localizedApproval",
                "required",
                Content::Approval(LocalizedApproval::Required),
            ),
            (
                ArtifactKind::ApprovalPolicy,
                "localizedApproval",
                "not-required",
                Content::Approval(LocalizedApproval::NotRequired),
            ),
            (
                ArtifactKind::SelectionPolicy,
                "mode",
                "explicit",
                Content::Selection,
            ),
            (
                ArtifactKind::SourceAdmissionPolicy,
                "sourceApproval",
                "required",
                Content::SourceAdmission(SourceApproval::Required),
            ),
            (
                ArtifactKind::SourceAdmissionPolicy,
                "sourceApproval",
                "authenticated-sufficient",
                Content::SourceAdmission(SourceApproval::AuthenticatedSufficient),
            ),
        ] {
            assert_eq!(
                read_value(kind, &json!({name: value})),
                Some(expected),
                "{name}: {value}"
            );
        }
    }

    #[test]
    fn a_provider_routing_names_its_adapter_and_a_pinned_configuration() {
        let sample = sample(ArtifactKind::ProviderRoutingPolicy);
        let Some(Content::ProviderRouting(routing)) =
            read_value(ArtifactKind::ProviderRoutingPolicy, &sample)
        else {
            panic!("the sample routing is admitted");
        };
        assert_eq!(
            routing.adapter,
            VersionedIdentity::literal("intlify-provider-fixture", "0")
        );
        assert_eq!(routing.configuration.as_str(), "storefront-routing");
        assert_eq!(
            routing.configuration_digest,
            SemanticDigest::from_hash([0x2a; 32])
        );
        let digest = |digits: String| Some(json!(format!("sha256:{digits}")));
        for (pointer, replacement) in [
            ("/adapter", None),
            ("/adapter", Some(json!("intlify-provider-fixture"))),
            ("/adapter/revision", None),
            ("/adapter/revision", Some(json!("0-"))),
            ("/adapter/identity", Some(json!("Intlify-Provider"))),
            ("/adapter/note", Some(json!("x"))),
            ("/configuration", None),
            ("/configuration", Some(json!("storefront-routing"))),
            ("/configuration/identity", None),
            ("/configuration/identity", Some(json!("Storefront-Routing"))),
            ("/configuration/semanticDigest", None),
            ("/configuration/semanticDigest", digest("2A".repeat(32))),
            ("/configuration/semanticDigest", digest("2a".repeat(31))),
            (
                "/configuration/semanticDigest",
                Some(json!("2a".repeat(32))),
            ),
            ("/configuration/note", Some(json!("x"))),
            ("/fallback", Some(json!("none"))),
        ] {
            let edited = edited(&sample, pointer, replacement.clone());
            assert!(
                read_value(ArtifactKind::ProviderRoutingPolicy, &edited).is_none(),
                "{pointer} {replacement:?}"
            );
        }
    }

    #[test]
    fn the_target_profile_reads_into_the_dimensions_017_leaves_open() {
        let versioned = |identity, revision| VersionedIdentity::literal(identity, revision);
        let limit = |value| NonZeroU64::new(value).unwrap();
        let sample = sample(ArtifactKind::TargetProfile);
        let profile = profile_of(&sample).unwrap();
        assert_eq!(profile.grammar, versioned("ecmascript", "2025"));
        assert_eq!(profile.module_profile, versioned("intlify-esm-module", "0"));
        assert_eq!(profile.mode, TargetMode::RuntimeBacked);
        assert_eq!(profile.adapter, versioned("intlify-vanilla-dom", "0"));
        assert_eq!(profile.mf2_specification, versioned("unicode-mf2", "48"));
        assert_eq!(
            profile.binding_table,
            versioned("intlify-binding-table", "0")
        );
        assert_eq!(
            profile.locale_payload,
            versioned("intlify-locale-payload", "0")
        );
        assert_eq!(
            profile.descriptor,
            versioned("intlify-target-descriptor", "0")
        );
        let entry = |locale: &str| LocaleDirection {
            locale: locale.into(),
            direction: Direction::Ltr,
        };
        assert_eq!(profile.directions, [entry("en"), entry("ja")]);
        assert_eq!(
            profile.limits,
            TargetLimits {
                messages: limit(1000),
                uses: limit(10_000),
                locales: limit(8),
                files: limit(64),
                file_bytes: limit(1_048_576),
                output_bytes: limit(8_388_608),
                parameter_bytes: limit(4096),
                text_bytes: limit(65_536),
            }
        );
        let aot = profile_of(&edited(&sample, "/mode", Some(json!("aot")))).unwrap();
        assert_eq!(aot.mode, TargetMode::Aot);
    }

    #[test]
    fn every_value_017_fixes_is_pinned() {
        let sample = sample(ArtifactKind::TargetProfile);
        let versioned =
            |identity: &str, revision: &str| json!({"identity": identity, "revision": revision});
        let mut extra = versioned("intlify-design-023", "0");
        extra["note"] = json!("x");
        for (pointer, value) in [
            ("/host/kind", json!("javascript-cjs-dom-text")),
            ("/host/kind", json!("JavaScript-ESM-DOM-Text")),
            ("/mode", json!("jit")),
            ("/mode", json!("Runtime-Backed")),
            ("/mode", json!(["aot"])),
            (
                "/execution/specification",
                versioned("intlify-design-023", "1"),
            ),
            (
                "/execution/specification",
                versioned("intlify-design-024", "0"),
            ),
            ("/execution/specification", extra),
            (
                "/execution/profile",
                versioned("intlify-text-interpolation-minimum", "1"),
            ),
            (
                "/execution/profile",
                versioned("intlify-text-interpolation-extended", "0"),
            ),
            ("/codecs/sourceMap", versioned("ecma-426-source-map", "2")),
            ("/codecs/sourceMap", versioned("intlify-source-map", "3")),
            ("/locale/selection", json!("fallback")),
            ("/locale/services", json!("present")),
            ("/delivery/unit", json!("secondary")),
            ("/delivery/loading", json!("lazy")),
        ] {
            let edited = edited(&sample, pointer, Some(value.clone()));
            assert!(profile_of(&edited).is_none(), "{pointer} {value}");
        }
    }

    #[test]
    fn every_open_identity_is_a_closed_pair_of_tokens() {
        let sample = sample(ArtifactKind::TargetProfile);
        for pointer in [
            "/host/grammar",
            "/host/moduleProfile",
            "/adapter",
            "/mf2Specification",
            "/codecs/bindingTable",
            "/codecs/localePayload",
            "/codecs/descriptor",
        ] {
            for value in [
                json!("ecmascript"),
                json!({"identity": "ecmascript"}),
                json!({"identity": "ecmascript", "revision": "2025", "note": "x"}),
                json!({"identity": "ECMAScript", "revision": "2025"}),
                json!({"identity": "ecmascript", "revision": ""}),
            ] {
                let edited = edited(&sample, pointer, Some(value.clone()));
                assert!(profile_of(&edited).is_none(), "{pointer} {value}");
            }
            // Any other well-formed pair is admitted: which ones are supported
            // is 024's to decide.
            let other = edited(
                &sample,
                pointer,
                Some(json!({"identity": "other", "revision": "9"})),
            );
            assert!(profile_of(&other).is_some(), "{pointer}");
        }
    }

    #[test]
    fn every_object_of_the_target_profile_is_closed() {
        let sample = sample(ArtifactKind::TargetProfile);
        let objects: [(&str, &[&str]); 7] = [
            ("", &TARGET_PROFILE),
            ("/host", &HOST),
            ("/execution", &EXECUTION),
            ("/codecs", &CODECS),
            ("/locale", &LOCALE),
            ("/delivery", &DELIVERY),
            ("/limits", &LIMITS),
        ];
        for (object, members) in objects {
            for member in members {
                let pointer = format!("{object}/{member}");
                assert!(
                    profile_of(&edited(&sample, &pointer, None)).is_none(),
                    "{pointer}"
                );
            }
            let pointer = format!("{object}/note");
            let extra = edited(&sample, &pointer, Some(json!("x")));
            assert!(profile_of(&extra).is_none(), "{pointer}");
            if !object.is_empty() {
                let array = edited(&sample, object, Some(json!([])));
                assert!(profile_of(&array).is_none(), "{object}");
            }
        }
    }

    #[test]
    fn directions_are_ordered_by_the_bytes_of_their_locales_without_repeats() {
        let sample = sample(ArtifactKind::TargetProfile);
        let with = |directions: &Value| {
            profile_of(&edited(
                &sample,
                "/locale/directions",
                Some(directions.clone()),
            ))
        };
        let entry =
            |locale: &str, direction: &str| json!({"locale": locale, "direction": direction});
        // Whether they cover the requested locales is checked against the
        // targets, so none at all is a content.
        assert_eq!(with(&json!([])).unwrap().directions, []);
        let all = with(&json!([
            entry("ar", "RTL"),
            entry("en", "LTR"),
            entry("zxx", "unknown"),
        ]))
        .unwrap();
        assert_eq!(
            all.directions
                .iter()
                .map(|entry| entry.direction)
                .collect::<Vec<_>>(),
            [Direction::Rtl, Direction::Ltr, Direction::Unknown]
        );
        // The order is that of UTF-8 bytes, which UTF-16 code units reverse
        // for these two.
        let (fullwidth, mathematical) = ("\u{ff41}", "\u{1d41a}");
        assert!(with(&json!([
            entry(fullwidth, "LTR"),
            entry(mathematical, "LTR")
        ]))
        .is_some());
        for directions in [
            json!([entry(mathematical, "LTR"), entry(fullwidth, "LTR")]),
            json!([entry("ja", "LTR"), entry("en", "LTR")]),
            json!([entry("en", "LTR"), entry("en", "LTR")]),
            json!([entry("en", "LTR"), entry("en", "RTL")]),
            json!([entry("", "LTR")]),
            json!([entry("en", "ltr")]),
            json!([entry("en", "Unknown")]),
            json!([{"locale": "en"}]),
            json!([{"locale": "en", "direction": "LTR", "script": "Latn"}]),
            json!([{"locale": ["en"], "direction": "LTR"}]),
            json!([["en", "LTR"]]),
            json!({"en": "LTR"}),
        ] {
            assert!(with(&directions).is_none(), "{directions}");
        }
    }

    #[test]
    fn limits_are_positive_shortest_decimals_in_the_u64_range() {
        let sample = sample(ArtifactKind::TargetProfile);
        for name in LIMITS {
            let pointer = format!("/limits/{name}");
            let with = |value: Value| profile_of(&edited(&sample, &pointer, Some(value)));
            assert!(with(json!("1")).is_some(), "{name}");
            assert!(with(json!("18446744073709551615")).is_some(), "{name}");
            for value in [
                json!("0"),
                json!("01"),
                json!("+1"),
                json!("-1"),
                json!("1.0"),
                json!("1e3"),
                json!(" 1"),
                json!(""),
                json!("18446744073709551616"),
                json!(64),
                json!(null),
            ] {
                assert!(with(value.clone()).is_none(), "{name} {value}");
            }
        }
    }

    #[test]
    fn the_contents_of_018_and_022_pass_on_as_the_objects_they_are() {
        let external = [
            ArtifactKind::GlossarySet,
            ArtifactKind::ResourceLimitPolicy,
            ArtifactKind::TrustPolicy,
        ];
        for kind in external {
            for content in [
                json!({}),
                json!({"projectProfileResolution": "default"}),
                json!({"localizedApproval": "sometimes"}),
            ] {
                let Some(Content::External(object)) = read_value(kind, &content) else {
                    panic!("{kind:?} {content}");
                };
                assert!(std::ptr::eq(object, content.as_object().unwrap()));
            }
        }
        // The other five read only their own content.
        for kind in ArtifactKind::ALL {
            if external.contains(&kind) {
                continue;
            }
            assert!(read_value(kind, &sample(kind)).is_some(), "{kind:?}");
            assert!(read_value(kind, &json!({})).is_none(), "{kind:?}");
            for other in ArtifactKind::ALL.into_iter().filter(|other| *other != kind) {
                assert!(
                    read_value(kind, &sample(other)).is_none(),
                    "{kind:?} {other:?}"
                );
            }
        }
    }
}
