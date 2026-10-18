// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Unstable display-only support for the `resolve_config` contributor example.
//! No file discovery, reference-body acquisition, product defaults, Profile,
//! formal diagnostics, or serializable resolver artifact is implemented here.
//! The optional feature returns only a presentation value over ordinary calls.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::input_limits::{Bound, InputLimits, RawInputLimits, ValueLimits};
use crate::locale::fixtures::{fixture_pins, FixtureProvider};
use crate::locale::{CanonicalLocale, CanonicalizationFailure, Canonicalizer, ProviderFailure};
use crate::materialize::{
    materialize_file, ByteSpan, InputFailure, MaterializedDocument, NodeKind,
};
use crate::model::ProfileDeclaration;
use crate::references::{PolicyReference, TargetProfileReference};
use crate::resolver::evaluation::{Actual, Code, Evaluation, IssueKey, Subject};
use crate::resolver::locales::{
    fixture_policy, resolve_locales, LocaleFields, LocaleInvariant, LocaleKey, LocaleSubject,
    Location, ResolvedLocales,
};

use super::eval::IssueKind;
use super::selection::{Selection, SelectionFailure, SelectorInput};
use super::{AuthoringSchema, StructuralLimits};

/// Example-owned capacity, not an admitted Resource Limit Policy or a product default.
pub const MAX_FILE_BYTES: u64 = 1_000_000;

const NOTICE: &str = "Developer example only: finite fixture locale data; not a LocalizationProjectProfile. Locale spellings are counted and, except coverage selectors, canonicalized; Policy/Target bodies, fallback, negotiation and coverage are not resolved.";

type Declaration = ProfileDeclaration<PolicyReference, TargetProfileReference>;

fn bound(value: u64) -> Bound {
    Bound::new(value).expect("fixed positive example capacity")
}

fn input_limits() -> InputLimits {
    InputLimits {
        raw: RawInputLimits {
            max_file_bytes: bound(MAX_FILE_BYTES),
            max_parser_tokens: bound(100_000),
        },
        value: ValueLimits {
            max_nodes: bound(100_000),
            max_depth: bound(64),
            max_collection_entries: bound(100_000),
            max_total_string_bytes: bound(1_000_000),
            max_single_string_bytes: bound(100_000),
        },
    }
}

fn diagnostic(code: &str, message: impl Into<String>, pointer: Option<&str>) -> Value {
    json!({"code": code, "message": message.into(), "pointer": pointer, "position": null})
}

fn position(source: &[u8], span: ByteSpan) -> Value {
    let start = usize::try_from(span.start_byte()).expect("source span is addressable");
    let prefix = &source[..start];
    let mut line = 1;
    let mut line_start = 0;
    for (index, byte) in prefix.iter().enumerate() {
        if *byte == b'\n' {
            line += 1;
            line_start = index + 1;
        }
    }
    json!({"startByte": span.start_byte(), "endByte": span.end_byte(),
        "line": line, "byteColumn": start - line_start + 1})
}

fn located(mut item: Value, source: &[u8], span: Option<ByteSpan>) -> Value {
    if let Some(span) = span {
        item["position"] = position(source, span);
    }
    item
}

/// The span of the value `pointer` names, or of its member's key when `key`.
fn pointer_span(doc: &MaterializedDocument, pointer: &str, key: bool) -> Option<ByteSpan> {
    let mut node = doc.root();
    let mut tokens = pointer.strip_prefix('/')?.split('/').peekable();
    while let Some(token) = tokens.next() {
        let name = token.replace("~1", "/").replace("~0", "~");
        node = match doc.node(node).kind() {
            NodeKind::Object(members) => {
                let member = members.get(&name)?;
                if key && tokens.peek().is_none() {
                    return Some(member.key_span());
                }
                member.value()
            }
            NodeKind::Array(items) => *items.get(name.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(doc.node(node).span())
}

/// What the pointer of the spelling at `location` names. A JSON Pointer
/// names a member's value and has no form for its key, so a map key's
/// pointer names its member and the display marks the key as the target.
fn target(location: &Location) -> &'static str {
    if matches!(
        location,
        Location::NegotiationAliasKey(_) | Location::MessageFallbackKey(_)
    ) {
        "key"
    } else {
        "value"
    }
}

/// The JSON Pointer of the spelling at `location`. A map key has no pointer
/// of its own, so a key's location points at its member (see [`target`]).
fn locale_pointer(profile: &str, declaration: &Declaration, location: &Location) -> String {
    let escape = |token: &str| token.replace('~', "~0").replace('/', "~1");
    let alias = |rank: usize| {
        declaration
            .locale_negotiation
            .as_option()
            .and_then(|negotiation| negotiation.aliases.as_option())
            .and_then(|aliases| aliases.keys().nth(rank))
            .map_or_else(String::new, |key| escape(key))
    };
    let fallback = |rank: usize| {
        declaration
            .message_fallback
            .as_option()
            .and_then(|fallback| fallback.keys().nth(rank))
            .map_or_else(String::new, |key| escape(key))
    };
    let field = match location {
        Location::RequestedLocale(index) => format!("requestedLocales/{index}"),
        Location::DefaultSourceLocale => "defaultSourceLocale".to_owned(),
        Location::DefaultRequestedLocale => "defaultRequestedLocale".to_owned(),
        Location::NegotiationAliasKey(rank) | Location::NegotiationAliasValue(rank) => {
            format!("localeNegotiation/aliases/{}", alias(*rank))
        }
        Location::MessageFallbackKey(rank) => format!("messageFallback/{}", fallback(*rank)),
        Location::MessageFallbackCandidate(rank, position) => {
            format!("messageFallback/{}/{position}", fallback(*rank))
        }
        Location::TargetRequestedLocale(id, index) => {
            format!(
                "targetProfiles/{}/requestedLocales/{index}",
                escape(id.as_str())
            )
        }
        Location::TargetDefaultRequestedLocale(id) => {
            format!(
                "targetProfiles/{}/defaultRequestedLocale",
                escape(id.as_str())
            )
        }
    };
    format!("/profiles/{}/{field}", escape(profile))
}

/// The canonical locale a subject names, when it names one.
fn canonical(subject: &LocaleSubject) -> Option<&str> {
    let key = match subject {
        LocaleSubject::Occurrences | LocaleSubject::RequestedLocales => return None,
        LocaleSubject::RequestedLocale(key)
        | LocaleSubject::DefaultSourceLocale(key)
        | LocaleSubject::DefaultRequestedLocale(key)
        | LocaleSubject::NegotiationAliasKey(key)
        | LocaleSubject::NegotiationAliasValue(_, key)
        | LocaleSubject::MessageFallbackKey(key)
        | LocaleSubject::MessageFallbackCandidate(_, _, key)
        | LocaleSubject::TargetRequestedLocale(_, key)
        | LocaleSubject::TargetDefaultRequestedLocale(_, key) => key,
    };
    match key {
        LocaleKey::Canonical(locale) => Some(locale.as_str()),
        LocaleKey::Content(_) => None,
    }
}

/// One blocking result as a display item, pointing at every spelling of its
/// subject. A rejected spelling is never echoed.
fn locale_issue(
    issue: &IssueKey,
    evaluation: &Evaluation,
    resolved: &ResolvedLocales,
    profile: &str,
    declaration: &Declaration,
) -> Value {
    let Subject::Locale(subject) = &issue.unit().subject else {
        return diagnostic(issue.code().as_str(), "Locale resolution failed.", None);
    };
    let locations = resolved
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.subject == *subject)
        .map(|occurrence| &occurrence.location)
        .collect::<Vec<_>>();
    let pointers = locations
        .iter()
        .map(|location| locale_pointer(profile, declaration, location))
        .collect::<Vec<_>>();
    let locale = canonical(subject).unwrap_or("(none)");
    let message = match issue.code() {
        Code::LocaleInvalid => {
            "This spelling is not a valid locale identifier in the finite example provider."
                .to_owned()
        }
        Code::LocaleDuplicate => format!(
            "These spellings name the same canonical locale {locale}. Remove the duplicate explicitly."
        ),
        Code::DefaultRequestedLocaleNotInProject => {
            format!("Default locale {locale} is not one of the project's requested locales.")
        }
        Code::ResourceLimitExceeded => evaluation.limit(issue).map_or_else(
            || "An example locale limit was exceeded.".to_owned(),
            |exceeded| {
                let actual = match exceeded.actual {
                    Actual::Exact(value) => value.to_string(),
                    Actual::AtLeast(value) => format!("at least {value}"),
                };
                format!(
                    "The example allows {} for {}; observed {actual}.",
                    exceeded.limit.get(),
                    exceeded.bound.name
                )
            },
        ),
        code => format!("Locale resolution reported {}.", code.as_str()),
    };
    let mut item = diagnostic(
        issue.code().as_str(),
        message,
        pointers.first().map(String::as_str),
    );
    // The spellings of one subject share one role, so one target covers
    // every pointer.
    if let Some(location) = locations.first() {
        item["target"] = json!(target(location));
    }
    if pointers.len() > 1 {
        item["relatedPointers"] = json!(pointers);
    }
    item
}

/// Run the existing private path through 015 Phase 7 and return an unstable
/// display object.
/// This function is deliberately feature-gated and hidden from normal API docs.
/// Configuration failures are values; errors indicate an internal invariant.
pub fn resolve(source: &[u8], selector: Option<&str>) -> Result<Value, String> {
    let mut output = json!({
        "example": "intlify-config/resolve_config", "notice": NOTICE,
        "provider": "finite-test-fixture", "status": "not-resolved", "stage": "materialize",
        "selectedProfile": null, "locales": null, "corrections": [], "diagnostics": []
    });
    let doc = match materialize_file(Arc::from(source), input_limits()) {
        Ok(doc) => Arc::new(doc),
        Err(error) => {
            let code = match &error.reason {
                InputFailure::InvalidUtf8 => "InvalidUtf8",
                InputFailure::Syntax => "InvalidJson",
                InputFailure::DuplicateMember => "DuplicateMember",
                InputFailure::NonScalarString => "NonScalarString",
                InputFailure::NonPortableNumber => "NonPortableNumber",
                InputFailure::ResourceLimits(_) => "InputLimit",
                InputFailure::AccountingOverflow => "AccountingOverflow",
            };
            let mut item = located(
                diagnostic(
                    code,
                    format!("Strict JSON input was rejected: {:?}", error.reason),
                    None,
                ),
                source,
                error.span,
            );
            if let Some(span) = error.related_span {
                item["relatedPosition"] = position(source, span);
            }
            output["diagnostics"] = json!([item]);
            return Ok(output);
        }
    };
    output["stage"] = json!("structure");
    let limits = StructuralLimits {
        max_profiles: bound(8),
        max_profile_id_bytes: bound(64),
        max_structural_analysis_units: bound(1_000_000),
    };
    let schema = AuthoringSchema::<PolicyReference, TargetProfileReference>::for_model()
        .map_err(|e| format!("Example schema invariant: {e:?}"))?;
    let analysis = schema
        .analyze(Arc::clone(&doc), limits)
        .map_err(|e| format!("Example analysis invariant: {e:?}"))?;
    if !analysis.is_complete() {
        let mut errors = analysis
            .issues()
            .iter()
            .map(|issue| {
                located(
                    diagnostic(
                        "StructuralAdmission",
                        format!("Configuration admission failed: {:?}", issue.reason),
                        None,
                    ),
                    source,
                    Some(issue.span),
                )
            })
            .collect::<Vec<_>>();
        if let Some(evaluation) = &analysis.evaluation {
            errors.extend(evaluation.issues.iter().map(|issue| {
                let (code, message) = match issue.kind {
                    IssueKind::RequiredFieldMissing => ("RequiredFieldMissing", format!("Missing required field: {}.", issue.missing_field.as_deref().unwrap_or("(unknown)"))),
                    IssueKind::UnknownField => ("UnknownField", "Unknown configuration field at this position.".to_owned()),
                    IssueKind::TypeInvalid => ("TypeInvalid", "Value has the wrong type for this configuration field.".to_owned()),
                    IssueKind::ValueInvalid => ("ValueInvalid", "Value does not satisfy the configuration schema. Check the generated schema for this field.".to_owned()),
                };
                located(diagnostic(code, message, None), source, Some(issue.span))
            }));
        }
        output["diagnostics"] = json!(errors);
        return Ok(output);
    }
    // Never select a valid fragment out of an incomplete root.
    let config = analysis
        .construct()
        .map_err(|e| format!("Example construction invariant: {e:?}"))?
        .ok_or("Complete example analysis did not construct a configuration")?;
    output["stage"] = json!("select");
    let selector = selector.map_or_else(
        || SelectorInput::absent(limits.max_profile_id_bytes),
        |id| SelectorInput::string(id, limits.max_profile_id_bytes),
    );
    let selected = match analysis
        .select(&selector)
        .map_err(|e| format!("Example selection invariant: {e:?}"))?
    {
        Selection::Selected(selected) => selected,
        Selection::Rejected(reason) => {
            let (code, message) = match reason {
                SelectionFailure::Required => (
                    "ProfileRequired",
                    "Several profiles are declared. Pass --profile NAME.".to_owned(),
                ),
                SelectionFailure::Unknown { .. } => (
                    "UnknownProfile",
                    "The requested profile is not declared in this configuration.".to_owned(),
                ),
                _ => (
                    "InvalidProfileSelector",
                    format!("Profile selector was rejected: {reason:?}"),
                ),
            };
            output["diagnostics"] = json!([diagnostic(code, message, None)]);
            return Ok(output);
        }
        Selection::Unavailable(reason) => {
            return Err(format!(
                "Complete example root cannot be selected: {reason:?}"
            ))
        }
    };
    let id = selected.id().as_str();
    output["selectedProfile"] = json!(id);
    output["stage"] = json!("locale");
    let policy = fixture_policy(bound(128), bound(16), bound(8));
    let provider = Canonicalizer::bind(
        &fixture_pins(),
        Some(FixtureProvider::new()),
        policy
            .limits()
            .locale_resolution
            .max_locale_identifier_bytes,
    )
    .map_err(|e| format!("Example provider invariant: {e:?}"))?;
    let declaration = config
        .profiles()
        .get(selected.id())
        .ok_or("Selected example profile is missing")?;
    let mut evaluation = Evaluation::default();
    let resolved = match resolve_locales(
        &mut evaluation,
        Ok(&policy),
        Ok(&provider),
        &LocaleFields::declared(declaration),
    ) {
        Ok(resolved) => resolved,
        // The finite provider answers only for its own table. A spelling it
        // lacks stops resolution without telling where; it may be valid.
        Err(LocaleInvariant::Canonicalization(CanonicalizationFailure::Provider(
            ProviderFailure::UnsupportedInput,
        ))) => {
            output["diagnostics"] = json!([diagnostic(
                "UnsupportedLocaleInput",
                "The finite example provider has no entry for a locale in this profile; it may still be valid.",
                None
            )]);
            return Ok(output);
        }
        Err(invariant) => {
            return Err(format!(
                "Example locale resolution invariant: {invariant:?}"
            ))
        }
    };
    output["corrections"] = json!(resolved
        .occurrences
        .iter()
        .filter(|occurrence| occurrence.corrected)
        .map(|occurrence| json!({
            "pointer": locale_pointer(id, declaration, &occurrence.location),
            "target": target(&occurrence.location),
            "replacement": canonical(&occurrence.subject)
        }))
        .collect::<Vec<_>>());
    let errors = evaluation
        .issues()
        .map(|issue| locale_issue(issue, &evaluation, &resolved, id, declaration))
        .collect::<Vec<_>>();
    if errors.is_empty() {
        let project = resolved
            .project
            .as_ref()
            .map_err(|_| "Example locales have no blocking result but no project locales")?;
        output["status"] = json!("resolved");
        output["locales"] = json!({
            "defaultSourceLocale": project.source_default().map(CanonicalLocale::as_str),
            "requestedLocales": project.requested().as_slice().iter().map(CanonicalLocale::as_str).collect::<Vec<_>>(),
            "defaultRequestedLocale": project.requested_default().as_str()
        });
    } else {
        output["diagnostics"] = json!(errors
            .into_iter()
            .map(|item| {
                let key = item["target"] == "key";
                let span = item["pointer"]
                    .as_str()
                    .and_then(|p| pointer_span(&doc, p, key));
                located(item, source, span)
            })
            .collect::<Vec<_>>());
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
