// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::fmt::Write as _;

use super::frontend::{JsonPathStep, JsonSyntaxTape};
use crate::offset_map::{MessageOffsetMapBuilder, OffsetMapInvariantError, OffsetMapSegmentLimit};
use crate::{MessageOffsetMap, Utf8ByteSpan, MAX_NESTING_DEPTH};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct JsonStringError {
    pub(super) span: Utf8ByteSpan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum JsonMessageDecodeError {
    Unrepresentable(JsonStringError),
    OffsetMap(OffsetMapInvariantError),
    OffsetMapSegments(OffsetMapSegmentLimit),
}

enum DecodedUnit<'source> {
    Identity {
        text: &'source str,
        raw_span: Utf8ByteSpan,
    },
    Escape {
        character: char,
        raw_span: Utf8ByteSpan,
    },
}

pub(super) fn build_json_pointer(
    source: &str,
    tape: &JsonSyntaxTape,
    leaf: u32,
) -> Result<String, JsonStringError> {
    const MAX_PATH_NODES: usize = MAX_NESTING_DEPTH as usize + 1;

    let mut path = [0_u32; MAX_PATH_NODES];
    let mut path_len = 0;
    let mut current = Some(leaf);
    while let Some(index) = current {
        let slot = path
            .get_mut(path_len)
            .expect("the parser depth limit bounds a leaf path");
        *slot = index;
        path_len += 1;
        current = tape.node(index).parent();
    }

    let mut pointer = String::new();
    for index in path[..path_len].iter().rev() {
        match tape.node(*index).step() {
            JsonPathStep::Root => {}
            JsonPathStep::Member(span) => {
                pointer.push('/');
                let member = decode_json_string(source, span)?;
                push_pointer_token(&mut pointer, &member);
            }
            JsonPathStep::Index(index) => {
                pointer.push('/');
                write!(pointer, "{index}").expect("writing into String cannot fail");
            }
        }
    }
    Ok(pointer)
}

pub(super) fn measure_json_string(
    source: &str,
    span: Utf8ByteSpan,
) -> Result<u64, JsonStringError> {
    let mut measured = 0_u64;
    visit_decoded_units(source, span, |unit| {
        let bytes = match unit {
            DecodedUnit::Identity { text, .. } => text.len(),
            DecodedUnit::Escape { character, .. } => character.len_utf8(),
        };
        measured = measured
            .checked_add(u64::try_from(bytes).expect("usize text length fits u64"))
            .expect("decoded JSON string length is bounded by host bytes");
        true
    })?;
    Ok(measured)
}

pub(super) fn decode_json_message(
    source: &str,
    span: Utf8ByteSpan,
    measured_len: u64,
    max_segments: usize,
) -> Result<(String, MessageOffsetMap), JsonMessageDecodeError> {
    let capacity =
        usize::try_from(measured_len).expect("accepted JSON message length fits supported usize");
    let mut map = MessageOffsetMapBuilder::new(span);
    map.try_push_raw_only(
        0,
        Utf8ByteSpan::new(span.start(), span.start() + 1),
        max_segments,
    )
    .map_err(JsonMessageDecodeError::OffsetMapSegments)?;
    let mut message = String::with_capacity(capacity);
    let mut segment_error = None;

    visit_decoded_units(source, span, |unit| {
        let result = match unit {
            DecodedUnit::Identity { text, raw_span } => {
                let message_start =
                    u32::try_from(message.len()).expect("accepted message length fits u32");
                message.push_str(text);
                let message_end =
                    u32::try_from(message.len()).expect("accepted message length fits u32");
                map.try_push_identity(
                    Utf8ByteSpan::new(message_start, message_end),
                    raw_span,
                    max_segments,
                )
            }
            DecodedUnit::Escape {
                character,
                raw_span,
            } => {
                let message_start =
                    u32::try_from(message.len()).expect("accepted message length fits u32");
                message.push(character);
                let message_end =
                    u32::try_from(message.len()).expect("accepted message length fits u32");
                map.try_push_unescape(
                    Utf8ByteSpan::new(message_start, message_end),
                    raw_span,
                    max_segments,
                )
            }
        };
        match result {
            Ok(()) => true,
            Err(error) => {
                segment_error = Some(error);
                false
            }
        }
    })
    .map_err(JsonMessageDecodeError::Unrepresentable)?;
    if let Some(error) = segment_error {
        return Err(JsonMessageDecodeError::OffsetMapSegments(error));
    }

    let message_len = u32::try_from(message.len()).expect("accepted message length fits u32");
    map.try_push_raw_only(
        message_len,
        Utf8ByteSpan::new(span.end() - 1, span.end()),
        max_segments,
    )
    .map_err(JsonMessageDecodeError::OffsetMapSegments)?;
    if message.is_empty() {
        map.set_empty_message_anchor(span.start() + 1)
            .map_err(JsonMessageDecodeError::OffsetMap)?;
    }
    let map = map
        .finish(source, &message)
        .map_err(JsonMessageDecodeError::OffsetMap)?;
    debug_assert_eq!(message.len(), capacity);
    Ok((message, map))
}

fn decode_json_string(source: &str, span: Utf8ByteSpan) -> Result<String, JsonStringError> {
    let measured = measure_json_string(source, span)?;
    let mut output = String::with_capacity(
        usize::try_from(measured).expect("decoded member name fits supported usize"),
    );
    visit_decoded_units(source, span, |unit| {
        match unit {
            DecodedUnit::Identity { text, .. } => output.push_str(text),
            DecodedUnit::Escape { character, .. } => output.push(character),
        }
        true
    })?;
    Ok(output)
}

fn visit_decoded_units<'source>(
    source: &'source str,
    span: Utf8ByteSpan,
    mut visit: impl FnMut(DecodedUnit<'source>) -> bool,
) -> Result<(), JsonStringError> {
    let content_start = usize::try_from(span.start() + 1).expect("u32 fits usize");
    let content_end = usize::try_from(span.end() - 1).expect("u32 fits usize");
    let bytes = source.as_bytes();
    let mut cursor = content_start;

    while cursor < content_end {
        if bytes[cursor] != b'\\' {
            let run_start = cursor;
            while cursor < content_end && bytes[cursor] != b'\\' {
                cursor += source[cursor..]
                    .chars()
                    .next()
                    .expect("string token ends on a character boundary")
                    .len_utf8();
            }
            if !visit(DecodedUnit::Identity {
                text: &source[run_start..cursor],
                raw_span: byte_span(run_start, cursor),
            }) {
                return Ok(());
            }
            continue;
        }

        let escape_start = cursor;
        let escaped = bytes[cursor + 1];
        let (character, escape_end) = match escaped {
            b'"' => ('"', cursor + 2),
            b'\\' => ('\\', cursor + 2),
            b'/' => ('/', cursor + 2),
            b'b' => ('\u{0008}', cursor + 2),
            b'f' => ('\u{000c}', cursor + 2),
            b'n' => ('\n', cursor + 2),
            b'r' => ('\r', cursor + 2),
            b't' => ('\t', cursor + 2),
            b'u' => {
                let first = decode_hex_quad(&bytes[cursor + 2..cursor + 6]);
                if (0xd800..=0xdbff).contains(&first) {
                    let second_start = cursor + 6;
                    if second_start + 6 > content_end
                        || bytes[second_start] != b'\\'
                        || bytes[second_start + 1] != b'u'
                    {
                        return Err(JsonStringError {
                            span: byte_span(escape_start, cursor + 6),
                        });
                    }
                    let second = decode_hex_quad(&bytes[second_start + 2..second_start + 6]);
                    if !(0xdc00..=0xdfff).contains(&second) {
                        return Err(JsonStringError {
                            span: byte_span(escape_start, cursor + 6),
                        });
                    }
                    let scalar = 0x1_0000
                        + ((u32::from(first) - 0xd800) << 10)
                        + (u32::from(second) - 0xdc00);
                    (
                        char::from_u32(scalar).expect("paired JSON surrogates form a scalar"),
                        second_start + 6,
                    )
                } else if (0xdc00..=0xdfff).contains(&first) {
                    return Err(JsonStringError {
                        span: byte_span(escape_start, cursor + 6),
                    });
                } else {
                    (
                        char::from_u32(u32::from(first))
                            .expect("a non-surrogate JSON escape forms a scalar"),
                        cursor + 6,
                    )
                }
            }
            _ => unreachable!("the JSON frontend validates escape spelling"),
        };
        if !visit(DecodedUnit::Escape {
            character,
            raw_span: byte_span(escape_start, escape_end),
        }) {
            return Ok(());
        }
        cursor = escape_end;
    }
    Ok(())
}

fn decode_hex_quad(bytes: &[u8]) -> u16 {
    bytes.iter().fold(0_u16, |value, byte| {
        (value << 4)
            | u16::from(match byte {
                b'0'..=b'9' => byte - b'0',
                b'a'..=b'f' => byte - b'a' + 10,
                b'A'..=b'F' => byte - b'A' + 10,
                _ => unreachable!("the JSON frontend validates hexadecimal escapes"),
            })
    })
}

fn push_pointer_token(pointer: &mut String, token: &str) {
    for character in token.chars() {
        match character {
            '~' => pointer.push_str("~0"),
            '/' => pointer.push_str("~1"),
            _ => pointer.push(character),
        }
    }
}

fn byte_span(start: usize, end: usize) -> Utf8ByteSpan {
    Utf8ByteSpan::new(
        u32::try_from(start).expect("host byte limit fits u32"),
        u32::try_from(end).expect("host byte limit fits u32"),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        build_json_pointer, decode_json_message, measure_json_string, JsonMessageDecodeError,
    };
    use crate::adapter::json::frontend::parse_json;
    use crate::Utf8ByteSpan;

    fn only_string(source: &str) -> (super::JsonSyntaxTape, u32, Utf8ByteSpan) {
        let tape = parse_json(source).unwrap();
        let (index, node) = tape.string_nodes().next().unwrap();
        (tape, index, node.span())
    }

    #[test]
    fn decodes_every_json_escape_and_compound_surrogate_unit() {
        let source = r#""\"\\\/\b\f\n\r\t\u0000\u00e9\uD83D\uDE00""#;
        let (_, _, span) = only_string(source);
        let measured = measure_json_string(source, span).unwrap();
        let (message, map) = decode_json_message(source, span, measured, usize::MAX).unwrap();

        assert_eq!(message, "\"\\/\u{0008}\u{000c}\n\r\t\0é😀");
        let emoji_start = u32::try_from(message.find('😀').unwrap()).unwrap();
        let raw_start = u32::try_from(source.find("\\uD83D").unwrap()).unwrap();
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(emoji_start, emoji_start + 4))
                .unwrap(),
            Utf8ByteSpan::new(raw_start, raw_start + 12)
        );
    }

    #[test]
    fn rejects_the_first_unpaired_surrogate_escape_as_one_complete_token() {
        for source in [r#""\uD800x""#, r#""\uD800\u0041""#, r#""\uDC00""#] {
            let (_, _, span) = only_string(source);
            let error = measure_json_string(source, span).unwrap_err();
            assert_eq!(
                &source[usize::try_from(error.span.start()).unwrap()
                    ..usize::try_from(error.span.end()).unwrap()],
                &source[1..7]
            );
        }
    }

    #[test]
    fn builds_rfc_6901_paths_without_container_type_tags() {
        let source = r#"{"a/b~c":{"0":"object"},"a.b":"dot","items":["array"]}"#;
        let tape = parse_json(source).unwrap();
        let pointers = tape
            .string_nodes()
            .map(|(index, _)| build_json_pointer(source, &tape, index).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(pointers, ["/a~1b~0c/0", "/a.b", "/items/0"]);
    }

    #[test]
    fn empty_message_anchor_stays_after_the_opening_quote() {
        let source = r#"{"empty":""}"#;
        let (_, _, span) = only_string(source);
        let (message, map) = decode_json_message(source, span, 0, usize::MAX).unwrap();
        assert!(message.is_empty());
        assert_eq!(
            map.map_span(Utf8ByteSpan::new(0, 0)).unwrap(),
            Utf8ByteSpan::new(span.start() + 1, span.start() + 1)
        );
    }

    #[test]
    fn segment_budget_stops_before_the_first_noncanonical_push() {
        let source = r#""a\nb""#;
        let (_, _, span) = only_string(source);
        let measured = measure_json_string(source, span).unwrap();
        let error = decode_json_message(source, span, measured, 3).unwrap_err();
        let JsonMessageDecodeError::OffsetMapSegments(error) = error else {
            panic!("expected offset-map segment limit");
        };
        assert_eq!(error.actual, 4);
        assert_eq!(
            &source[usize::try_from(error.raw_span.start()).unwrap()
                ..usize::try_from(error.raw_span.end()).unwrap()],
            "b"
        );

        let empty_source = r#""""#;
        let (_, _, empty_span) = only_string(empty_source);
        let (_, map) = decode_json_message(empty_source, empty_span, 0, 1).unwrap();
        assert_eq!(map.segment_count(), 1);
    }
}
