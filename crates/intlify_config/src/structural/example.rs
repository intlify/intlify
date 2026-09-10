// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Unstable display-only support for the `resolve_config` contributor example.
//! No file discovery, reference-body acquisition, product defaults, Profile,
//! formal diagnostics, or serializable resolver artifact is implemented here.
//! The optional feature returns only a presentation value over ordinary calls.

use std::sync::Arc;

use serde_json::{json, Value};

use crate::input_limits::{Bound, InputLimits, RawInputLimits, ValueLimits};
use crate::locale::core::{Failure, Input, Issue, Limits, Location};
use crate::locale::fixtures::{fixture_binding, FixtureProvider};
use crate::locale::{CanonicalLocale, CanonicalizationFailure, Canonicalizer, ProviderFailure};
use crate::materialize::{
    materialize_file, ByteSpan, InputFailure, MaterializedDocument, NodeKind,
};
use crate::references::{PolicyReference, TargetProfileReference};

use super::eval::IssueKind;
use super::selection::{Selection, SelectionFailure, SelectorInput};
use super::{AuthoringSchema, StructuralLimits};

/// Example-owned capacity, not an admitted Resource Limit Policy or a product default.
pub const MAX_FILE_BYTES: u64 = 1_000_000;

const NOTICE: &str = "Developer example only: finite fixture locale data; not a LocalizationProjectProfile. Policy/Target bodies, fallback and negotiation are not resolved.";

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

fn pointer_span(doc: &MaterializedDocument, pointer: &str) -> Option<ByteSpan> {
    let mut node = doc.root();
    for token in pointer.strip_prefix('/')?.split('/') {
        let key = token.replace("~1", "/").replace("~0", "~");
        node = match doc.node(node).kind() {
            NodeKind::Object(members) => members.get(&key)?.value(),
            NodeKind::Array(items) => *items.get(key.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(doc.node(node).span())
}

fn locale_pointer(profile: &str, location: Location) -> String {
    let profile = profile.replace('~', "~0").replace('/', "~1");
    let field = match location {
        Location::SourceDefault => "defaultSourceLocale".to_owned(),
        Location::Requested(index) => format!("requestedLocales/{index}"),
        Location::RequestedDefault => "defaultRequestedLocale".to_owned(),
    };
    format!("/profiles/{profile}/{field}")
}

fn locale_issue(issue: &Issue, profile: &str) -> Value {
    match issue {
        Issue::Canonicalization { location, reason } => {
            let (code, message) = match reason {
                CanonicalizationFailure::Provider(ProviderFailure::UnsupportedInput) => (
                    "UnsupportedLocaleInput",
                    "The finite example provider has no entry for this locale; it may still be valid.".to_owned(),
                ),
                CanonicalizationFailure::Provider(ProviderFailure::InvalidIdentifier) => (
                    "InvalidLocaleIdentifier", "This input is explicitly invalid in the finite example provider.".to_owned(),
                ),
                CanonicalizationFailure::ByteLimit { .. } => (
                    "LocaleIdentifierLimit", format!("Example locale identifier limit exceeded: {reason:?}"),
                ),
                _ => ("LocaleProviderFailure", format!("Locale provider could not resolve this input: {reason:?}")),
            };
            diagnostic(code, message, Some(&locale_pointer(profile, *location)))
        }
        Issue::Duplicate {
            locale,
            occurrences,
        } => {
            let pointers = occurrences
                .iter()
                .map(|&i| locale_pointer(profile, Location::Requested(i)))
                .collect::<Vec<_>>();
            let mut item = diagnostic("CanonicalLocaleDuplicate", format!("Requested locales resolve to the same canonical locale: {}. Remove the duplicate explicitly.", locale.as_str()), pointers.first().map(String::as_str));
            item["relatedPointers"] = json!(pointers);
            item
        }
        Issue::DefaultNotRequested { locale } => diagnostic(
            "DefaultNotRequested",
            format!(
                "Default locale {} is not in the canonical requested set.",
                locale.as_str()
            ),
            Some(&locale_pointer(profile, Location::RequestedDefault)),
        ),
        Issue::RequestedLimit { limit, actual } => diagnostic(
            "RequestedLocaleLimit",
            format!(
                "The example allows {} requested locales; observed {actual}.",
                limit.get()
            ),
            None,
        ),
    }
}

/// Run the existing private minimum path and return an unstable display object.
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
    let provider =
        Canonicalizer::bind(&fixture_binding(), Some(FixtureProvider::new()), bound(128))
            .map_err(|e| format!("Example provider invariant: {e:?}"))?;
    let input = Input::from_selected(&config, selected.id())
        .ok_or("Selected example profile is missing")?;
    let resolution = input.resolve(
        &provider,
        Limits {
            max_active_occurrences: bound(16),
            max_requested_locales: bound(8),
        },
    );
    output["corrections"] = json!(resolution
        .corrections()
        .iter()
        .map(|c| json!({
            "pointer": locale_pointer(id, c.location), "replacement": c.replacement.as_str()
        }))
        .collect::<Vec<_>>());
    match resolution.value() {
        Ok(core) => {
            output["status"] = json!("resolved");
            output["locales"] = json!({
                "defaultSourceLocale": core.source_default().map(CanonicalLocale::as_str),
                "requestedLocales": core.requested().iter().map(CanonicalLocale::as_str).collect::<Vec<_>>(),
                "defaultRequestedLocale": core.requested_default().as_str()
            });
        }
        Err(failure) => {
            let errors = match failure {
                Failure::Issues(issues) => {
                    issues.iter().map(|issue| locale_issue(issue, id)).collect()
                }
                _ => vec![diagnostic(
                    "LocaleCoreLimit",
                    format!("Locale core cannot be resolved: {failure:?}"),
                    None,
                )],
            };
            output["diagnostics"] = json!(errors
                .into_iter()
                .map(|item| {
                    let span = item["pointer"].as_str().and_then(|p| pointer_span(&doc, p));
                    located(item, source, span)
                })
                .collect::<Vec<_>>());
        }
    }
    Ok(output)
}

#[cfg(test)]
mod tests;
