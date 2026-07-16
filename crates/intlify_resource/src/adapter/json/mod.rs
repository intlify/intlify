// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

mod frontend;
mod string;

use std::any::Any;
use std::sync::Arc;

use self::frontend::{parse_json, JsonFrontendError};
use self::string::{
    build_json_pointer, decode_json_message, measure_json_string, JsonMessageDecodeError,
    JsonStringError,
};
use super::{AdapterArtifactState, AdapterReescapePlan, HostAdapter};
use crate::artifact::{AdapterMessageEntry, ArtifactBuilder, MessageEntry};
use crate::registry::{HostFormat, ResolvedHostFormat};
use crate::{
    CatalogKeyDomain, EntryKey, EntryUnsupportedReason, InternalResourceErrorReason, ResourceError,
    ResourceErrorSite, ResourceLimit, ResourcePhase, Utf8ByteSpan, MAX_OFFSET_MAP_SEGMENTS,
};

pub(crate) struct JsonAdapter;

struct JsonArtifactState;

#[derive(Debug, Clone, Copy)]
struct JsonReescapePlan;

impl HostAdapter for JsonAdapter {
    fn extract(
        &self,
        resolved: &ResolvedHostFormat,
        source: &Arc<str>,
        builder: &mut ArtifactBuilder,
        phase: ResourcePhase,
    ) -> Result<AdapterArtifactState, ResourceError> {
        if resolved.format() != HostFormat::Json {
            return Err(adapter_invariant(phase, None));
        }

        let tape = parse_json(source).map_err(|error| frontend_error(error, phase))?;
        for (index, node) in tape.string_nodes() {
            let structural_path = build_json_pointer(source, &tape, index).map_err(|error| {
                unsupported_string(
                    error,
                    EntryUnsupportedReason::StructuralPathUnsupported,
                    phase,
                )
            })?;
            let raw_value_span = node.span();
            let measured_len = measure_json_string(source, raw_value_span).map_err(|error| {
                unsupported_string(
                    error,
                    EntryUnsupportedReason::MessageTextUnrepresentable,
                    phase,
                )
            })?;

            let occurrence = builder.preflight_entry(
                &structural_path,
                &structural_path,
                None,
                raw_value_span,
                u128::from(measured_len),
            )?;
            let segment_base = builder.offset_map_segment_count();
            let available_segments = usize::try_from(
                u128::from(MAX_OFFSET_MAP_SEGMENTS)
                    .checked_sub(segment_base)
                    .ok_or_else(|| adapter_invariant(phase, Some(raw_value_span)))?,
            )
            .map_err(|_| adapter_invariant(phase, Some(raw_value_span)))?;

            let (message_text, offset_map) =
                decode_json_message(source, raw_value_span, measured_len, available_segments)
                    .map_err(|error| {
                        decode_error(
                            error,
                            phase,
                            raw_value_span,
                            segment_base,
                            &structural_path,
                            occurrence,
                        )
                    })?;
            builder.push_entry(AdapterMessageEntry::new(
                structural_path.clone(),
                CatalogKeyDomain::JsonPointer,
                structural_path,
                None,
                raw_value_span,
                message_text,
                offset_map,
                false,
            ))?;
        }

        Ok(Arc::new(JsonArtifactState))
    }

    fn plan_reescape(
        &self,
        artifact_state: &(dyn Any + Send + Sync),
        _entry: &MessageEntry,
        formatted_message: &str,
        phase: ResourcePhase,
    ) -> Result<AdapterReescapePlan, ResourceError> {
        if artifact_state.downcast_ref::<JsonArtifactState>().is_none() {
            return Err(adapter_invariant(phase, None));
        }
        let measured_len = measure_serialized_json_string(formatted_message)
            .ok_or_else(|| adapter_invariant(phase, None))?;
        Ok(AdapterReescapePlan::new(
            measured_len,
            Box::new(JsonReescapePlan),
        ))
    }

    fn materialize(
        &self,
        artifact_state: &(dyn Any + Send + Sync),
        _entry: &MessageEntry,
        formatted_message: &str,
        plan: &AdapterReescapePlan,
        phase: ResourcePhase,
    ) -> Result<String, ResourceError> {
        if artifact_state.downcast_ref::<JsonArtifactState>().is_none()
            || plan.state().downcast_ref::<JsonReescapePlan>().is_none()
        {
            return Err(adapter_invariant(phase, None));
        }
        serialize_json_string(formatted_message, plan.measured_len())
            .ok_or_else(|| adapter_invariant(phase, None))
    }
}

fn frontend_error(error: JsonFrontendError, phase: ResourcePhase) -> ResourceError {
    match error {
        JsonFrontendError::Syntax(error) => ResourceError::parse_failed(
            "json",
            None,
            phase,
            ResourceErrorSite::new(error.span, None),
        ),
        JsonFrontendError::NestingDepth { span, actual } => ResourceError::limit_exceeded(
            ResourceLimit::NestingDepth,
            actual,
            phase,
            Some(ResourceErrorSite::new(span, None)),
        ),
    }
}

fn unsupported_string(
    error: JsonStringError,
    reason: EntryUnsupportedReason,
    phase: ResourcePhase,
) -> ResourceError {
    ResourceError::entry_unsupported(
        "json",
        None,
        reason,
        phase,
        ResourceErrorSite::new(error.span, None),
    )
}

fn decode_error(
    error: JsonMessageDecodeError,
    phase: ResourcePhase,
    raw_value_span: Utf8ByteSpan,
    segment_base: u128,
    structural_path: &str,
    occurrence: u32,
) -> ResourceError {
    match error {
        JsonMessageDecodeError::Unrepresentable(error) => unsupported_string(
            error,
            EntryUnsupportedReason::MessageTextUnrepresentable,
            phase,
        ),
        JsonMessageDecodeError::OffsetMap(_) => ResourceError::internal(
            InternalResourceErrorReason::OffsetMapInvariantFailed,
            phase,
            Some(ResourceErrorSite::new(
                raw_value_span,
                Some(entry_key(structural_path, occurrence)),
            )),
        ),
        JsonMessageDecodeError::OffsetMapSegments(error) => ResourceError::limit_exceeded(
            ResourceLimit::OffsetMapSegments,
            segment_base + error.actual as u128,
            phase,
            Some(ResourceErrorSite::new(
                error.raw_span,
                Some(entry_key(structural_path, occurrence)),
            )),
        ),
    }
}

fn entry_key(structural_path: &str, occurrence: u32) -> EntryKey {
    EntryKey::new(
        crate::StructuralPathKey::from_shared(Arc::from(structural_path)),
        occurrence,
    )
}

fn adapter_invariant(phase: ResourcePhase, span: Option<Utf8ByteSpan>) -> ResourceError {
    ResourceError::internal(
        InternalResourceErrorReason::AdapterInvariantFailed,
        phase,
        span.map(|span| ResourceErrorSite::new(span, None)),
    )
}

fn measure_serialized_json_string(message: &str) -> Option<u64> {
    let mut bytes = 2_u64;
    for character in message.chars() {
        let emitted = match character {
            '"' | '\\' | '\u{0008}' | '\t' | '\n' | '\u{000c}' | '\r' => 2,
            '\u{0000}'..='\u{001f}' => 6,
            _ => u64::try_from(character.len_utf8()).ok()?,
        };
        bytes = bytes.checked_add(emitted)?;
    }
    Some(bytes)
}

fn serialize_json_string(message: &str, measured_len: u64) -> Option<String> {
    let capacity = usize::try_from(measured_len).ok()?;
    let mut output = String::with_capacity(capacity);
    output.push('"');
    for character in message.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\u{0008}' => output.push_str("\\b"),
            '\t' => output.push_str("\\t"),
            '\n' => output.push_str("\\n"),
            '\u{000c}' => output.push_str("\\f"),
            '\r' => output.push_str("\\r"),
            '\u{0000}'..='\u{001f}' => {
                output.push_str("\\u00");
                let value = character as u8;
                output.push(hex_digit(value >> 4));
                output.push(hex_digit(value & 0x0f));
            }
            _ => output.push(character),
        }
    }
    output.push('"');
    (output.len() == capacity).then_some(output)
}

const fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => (b'0' + value) as char,
        10..=15 => (b'a' + value - 10) as char,
        _ => unreachable!(),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::thread;

    use allocation_counter::measure;

    use super::{
        decode_error, measure_serialized_json_string, serialize_json_string, JsonArtifactState,
        JsonMessageDecodeError,
    };
    use crate::offset_map::OffsetMapSegmentLimit;
    use crate::{
        CatalogKeyDomain, EntryUnsupportedReason, FormattedEntry, HostFormatRegistry,
        InternalResourceErrorReason, ResourceErrorCode, ResourceErrorDetails, ResourceLimit,
        ResourcePhase, Utf8ByteSpan, WriteBackOutcome, MAX_MESSAGE_BYTES, MAX_NESTING_DEPTH,
        MAX_OFFSET_MAP_SEGMENTS,
    };

    fn extract(source: &str) -> crate::ExtractedCatalog {
        let registry = HostFormatRegistry::new();
        let resolved = registry.resolve_direct_extension(".JSON").unwrap();
        registry.extract(resolved, Arc::from(source)).unwrap()
    }

    fn extraction_error(source: &str) -> crate::ResourceError {
        let registry = HostFormatRegistry::new();
        let resolved = registry.resolve_direct_extension(".json").unwrap();
        registry.extract(resolved, Arc::from(source)).unwrap_err()
    }

    #[test]
    fn extracts_every_json_string_leaf_in_raw_order_with_pointer_identity() {
        let source = r#"{"a.b":"dot","a":{"b":"nested"},"items":{"0":"object"},"items":["array",1,true,null],"dup":"first","dup":"second"}"#;
        let catalog = extract(source);
        let entries = catalog.entries();
        let keys = entries
            .iter()
            .map(|entry| {
                (
                    entry.key().structural_path().as_str(),
                    entry.key().occurrence(),
                    entry.message_text(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            keys,
            [
                ("/a.b", 0, "dot"),
                ("/a/b", 0, "nested"),
                ("/items/0", 0, "object"),
                ("/items/0", 1, "array"),
                ("/dup", 0, "first"),
                ("/dup", 1, "second"),
            ]
        );
        for entry in entries {
            assert_eq!(entry.catalog_key_domain(), CatalogKeyDomain::JsonPointer);
            assert_eq!(
                entry.catalog_key().as_str(),
                entry.key().structural_path().as_str()
            );
            assert!(entry.display_key().is_none());
            assert!(!entry.is_read_only());
            let span = entry.raw_value_span();
            assert_eq!(
                source.as_bytes()[usize::try_from(span.start()).unwrap()],
                b'"'
            );
            assert_eq!(
                source.as_bytes()[usize::try_from(span.end() - 1).unwrap()],
                b'"'
            );
        }
    }

    #[test]
    fn root_string_and_zero_entry_documents_are_supported() {
        let root = extract(r#""message""#);
        assert_eq!(root.entries()[0].key().structural_path().as_str(), "");
        assert_eq!(root.entries()[0].message_text(), "message");

        for source in ["{}", "[]", r#"{"n":1,"b":true,"z":null}"#] {
            assert!(extract(source).entries().is_empty());
        }
    }

    #[test]
    fn offset_maps_cover_quotes_identity_escapes_and_surrogate_pairs() {
        let source = r#"{"x":"a\n\u00e9\uD83D\uDE00z","empty":""}"#;
        let catalog = extract(source);
        let entry = &catalog.entries()[0];
        assert_eq!(entry.message_text(), "a\né😀z");

        let newline_raw = u32::try_from(source.find("\\n").unwrap()).unwrap();
        assert_eq!(
            entry
                .offset_map()
                .map_span(Utf8ByteSpan::new(1, 2))
                .unwrap(),
            Utf8ByteSpan::new(newline_raw, newline_raw + 2)
        );
        let emoji_message = u32::try_from(entry.message_text().find('😀').unwrap()).unwrap();
        let emoji_raw = u32::try_from(source.find("\\uD83D").unwrap()).unwrap();
        assert_eq!(
            entry
                .offset_map()
                .map_span(Utf8ByteSpan::new(emoji_message, emoji_message + 4))
                .unwrap(),
            Utf8ByteSpan::new(emoji_raw, emoji_raw + 12)
        );

        let empty = &catalog.entries()[1];
        assert_eq!(
            empty
                .offset_map()
                .map_span(Utf8ByteSpan::new(0, 0))
                .unwrap(),
            Utf8ByteSpan::new(
                empty.raw_value_span().start() + 1,
                empty.raw_value_span().start() + 1,
            )
        );
    }

    #[test]
    fn candidate_and_path_unpaired_surrogates_fail_complete_with_exact_sites() {
        let value_source = r#"{"ok":"value","bad":"\uD800"}"#;
        let error = extraction_error(value_source);
        assert_eq!(error.code(), ResourceErrorCode::EntryUnsupported);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::EntryUnsupported {
                reason: EntryUnsupportedReason::MessageTextUnrepresentable,
                ..
            }
        ));
        let site = error.site().unwrap().span();
        assert_eq!(
            &value_source
                [usize::try_from(site.start()).unwrap()..usize::try_from(site.end()).unwrap()],
            "\\uD800"
        );

        let path_source = r#"{"\uD800":{"message":"value"}}"#;
        let error = extraction_error(path_source);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::EntryUnsupported {
                reason: EntryUnsupportedReason::StructuralPathUnsupported,
                ..
            }
        ));
        let site = error.site().unwrap().span();
        assert_eq!(
            &path_source
                [usize::try_from(site.start()).unwrap()..usize::try_from(site.end()).unwrap()],
            "\\uD800"
        );
        assert!(error.site().unwrap().entry_key().is_none());
    }

    #[test]
    fn unrepresentable_member_name_without_a_string_candidate_is_not_decoded() {
        let catalog = extract(r#"{"\uD800":{"n":1,"b":false},"ok":"value"}"#);
        assert_eq!(catalog.entries().len(), 1);
        assert_eq!(catalog.entries()[0].key().structural_path().as_str(), "/ok");
    }

    #[test]
    fn syntax_and_depth_errors_use_typed_resource_contracts() {
        let error = extraction_error("{]");
        assert_eq!(error.code(), ResourceErrorCode::ParseFailed);
        assert_eq!(error.phase(), ResourcePhase::Extract);
        assert_eq!(error.site().unwrap().span(), Utf8ByteSpan::new(1, 2));
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::ParseFailed {
                format: "json",
                outer_format: None,
            }
        ));

        let depth = usize::try_from(MAX_NESTING_DEPTH).unwrap();
        let source = format!("{}0{}", "[".repeat(depth + 1), "]".repeat(depth + 1));
        let error = extraction_error(&source);
        assert_eq!(error.code(), ResourceErrorCode::LimitExceeded);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::LimitExceeded {
                resource: ResourceLimit::NestingDepth,
                actual,
                ..
            } if *actual == u128::from(MAX_NESTING_DEPTH) + 1
        ));
    }

    #[test]
    fn optional_bom_and_all_bytes_outside_values_are_preserved() {
        let source = "\u{feff}{\r\n  \"a\": \"one\",\r\n  \"b\": \"two\"\r\n}\r\n";
        let catalog = extract(source);
        assert_eq!(catalog.entries()[0].raw_value_span().start(), 13);
        let formatted = [
            FormattedEntry {
                entry: catalog.entries()[0].handle(),
                formatted_message: "ONE",
            },
            FormattedEntry {
                entry: catalog.entries()[1].handle(),
                formatted_message: "TWO",
            },
        ];
        let WriteBackOutcome::Changed(write_back) =
            catalog.build_and_validate_write_back(&formatted).unwrap()
        else {
            panic!("changed values must produce a candidate");
        };
        assert_eq!(
            write_back.candidate().source(),
            "\u{feff}{\r\n  \"a\": \"ONE\",\r\n  \"b\": \"TWO\"\r\n}\r\n"
        );
    }

    #[test]
    fn byte_identical_messages_preserve_optional_escape_spelling() {
        let catalog = extract(r#"{"value":"\/\u0061"}"#);
        assert_eq!(catalog.entries()[0].message_text(), "/a");
        let formatted = [FormattedEntry {
            entry: catalog.entries()[0].handle(),
            formatted_message: "/a",
        }];
        assert!(matches!(
            catalog.build_and_validate_write_back(&formatted).unwrap(),
            WriteBackOutcome::Unchanged
        ));
        assert_eq!(catalog.source(), r#"{"value":"\/\u0061"}"#);
    }

    #[test]
    fn canonical_serializer_covers_every_escape_and_direct_unicode_branch() {
        let message = "\"\\\u{0008}\t\n\u{000c}\r\u{0000}\u{001f}/é\u{2028}\u{2029}<";
        let measured = measure_serialized_json_string(message).unwrap();
        let raw = serialize_json_string(message, measured).unwrap();
        assert_eq!(
            raw,
            "\"\\\"\\\\\\b\\t\\n\\f\\r\\u0000\\u001f/é\u{2028}\u{2029}<\""
        );
        assert_eq!(u64::try_from(raw.len()).unwrap(), measured);

        let catalog = extract(r#"{"value":"old"}"#);
        let formatted = [FormattedEntry {
            entry: catalog.entries()[0].handle(),
            formatted_message: message,
        }];
        let WriteBackOutcome::Changed(write_back) =
            catalog.build_and_validate_write_back(&formatted).unwrap()
        else {
            panic!("changed value must produce a candidate");
        };
        assert_eq!(write_back.replacements()[0].raw_text(), raw);
        assert_eq!(write_back.candidate().entries()[0].message_text(), message);

        let candidate = write_back.into_candidate();
        let repeated = [FormattedEntry {
            entry: candidate.entries()[0].handle(),
            formatted_message: message,
        }];
        assert!(matches!(
            candidate.build_and_validate_write_back(&repeated).unwrap(),
            WriteBackOutcome::Unchanged
        ));
    }

    #[test]
    fn serializer_measurement_is_exact_and_allocation_free() {
        let message = "plain \"quoted\" text\nwith é and 😀";
        let allocations = measure(|| {
            assert_eq!(measure_serialized_json_string(message), Some(41));
        });
        assert_eq!(allocations.count_total, 0);
    }

    #[test]
    fn message_limit_is_checked_before_output_sized_decode_allocation() {
        let boundary = "x".repeat(usize::try_from(MAX_MESSAGE_BYTES).unwrap());
        let catalog = extract(&format!("{{\"value\":\"{boundary}\"}}"));
        assert_eq!(catalog.entries()[0].message_text().len(), boundary.len());

        let over = "x".repeat(usize::try_from(MAX_MESSAGE_BYTES + 1).unwrap());
        let error = extraction_error(&format!("{{\"value\":\"{over}\"}}"));
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::LimitExceeded {
                resource: ResourceLimit::MessageBytes,
                actual,
                ..
            } if *actual == u128::from(MAX_MESSAGE_BYTES) + 1
        ));
        assert_eq!(
            error
                .site()
                .unwrap()
                .entry_key()
                .unwrap()
                .structural_path()
                .as_str(),
            "/value"
        );

        let unrepresentable = format!("{{\"value\":\"{over}\\uD800\"}}");
        let error = extraction_error(&unrepresentable);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::EntryUnsupported {
                reason: EntryUnsupportedReason::MessageTextUnrepresentable,
                ..
            }
        ));
    }

    #[test]
    fn offset_map_limit_reports_the_first_crossing_raw_unit_and_entry() {
        let crossing_span = Utf8ByteSpan::new(8, 10);
        let error = decode_error(
            JsonMessageDecodeError::OffsetMapSegments(OffsetMapSegmentLimit {
                raw_span: crossing_span,
                actual: 2,
            }),
            ResourcePhase::Extract,
            Utf8ByteSpan::new(6, 11),
            u128::from(MAX_OFFSET_MAP_SEGMENTS) - 1,
            "/value",
            3,
        );

        assert!(matches!(
            error.details(),
            ResourceErrorDetails::LimitExceeded {
                resource: ResourceLimit::OffsetMapSegments,
                actual,
                ..
            } if *actual == u128::from(MAX_OFFSET_MAP_SEGMENTS) + 1
        ));
        assert_eq!(error.site().unwrap().span(), crossing_span);
        let key = error.site().unwrap().entry_key().unwrap();
        assert_eq!(key.structural_path().as_str(), "/value");
        assert_eq!(key.occurrence(), 3);
    }

    #[test]
    fn registry_and_artifacts_are_safe_for_concurrent_json_extraction() {
        let registry = Arc::new(HostFormatRegistry::new());
        let handles = (0..8)
            .map(|_| {
                let registry = Arc::clone(&registry);
                thread::spawn(move || {
                    let resolved = registry.resolve_direct_extension(".json").unwrap();
                    let catalog = registry
                        .extract(resolved, Arc::from(r#"{"a":"one","b":"two"}"#))
                        .unwrap();
                    catalog
                        .entries()
                        .iter()
                        .map(|entry| entry.message_text().to_owned())
                        .collect::<Vec<_>>()
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            assert_eq!(handle.join().unwrap(), ["one", "two"]);
        }
    }

    #[test]
    fn published_json_artifact_state_does_not_retain_the_syntax_tape() {
        assert_eq!(std::mem::size_of::<JsonArtifactState>(), 0);
        let catalog = extract(r#"{"ignored":{"n":1},"message":"value"}"#);
        assert_eq!(catalog.entries().len(), 1);
    }

    #[test]
    fn generated_json_candidate_failures_are_not_partially_exposed() {
        let catalog = extract(r#"{"value":"old"}"#);
        let formatted = [FormattedEntry {
            entry: catalog.entries()[0].handle(),
            formatted_message: "new",
        }];
        let outcome = catalog.build_and_validate_write_back(&formatted).unwrap();
        assert!(matches!(outcome, WriteBackOutcome::Changed(_)));

        let error = super::adapter_invariant(ResourcePhase::ValidateWriteBack, None);
        assert!(matches!(
            error.details(),
            ResourceErrorDetails::Internal {
                reason: InternalResourceErrorReason::AdapterInvariantFailed
            }
        ));
    }
}
