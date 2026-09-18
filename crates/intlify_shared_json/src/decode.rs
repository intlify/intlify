// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Bounded strict reading for shared JSON artifacts.
//!
//! Capacity is checked before JSON tree allocation, and duplicate or malformed
//! input information is never lost by a map that silently discards it. These
//! are private reader capacities, not project Resource Limit Policy defaults.
//!
//! A reader-capacity failure cannot be represented as an admitted truncated
//! value. Decoding is not admission: the owning schema and its validator still
//! decide whether a well-formed value is acceptable.

use serde::de::DeserializeOwned;
use serde_json::Value;

const MAX_BYTES: usize = 16 * 1024 * 1024;
const MAX_DEPTH: usize = 64;
const MAX_ATOM_BYTES: usize = 1024 * 1024;
const MAX_TOKENS: usize = 1_000_000;

/// Complete failure of a bounded strict read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecodeFailure {
    /// Input exceeds a byte, depth, atom, or token capacity.
    Capacity,
    /// Input is not one complete, unambiguous JSON value.
    Syntax,
    /// A JSON number was present where the shared encoding forbids one.
    Number,
    /// The value does not match the requested closed schema.
    Shape,
    /// The selector tuple is not implemented by this reader.
    Unsupported,
    /// A stored integrity digest does not match its recomputed value.
    Integrity,
}

/// Tokens count containers, quoted member names/values, and primitive atoms.
/// Per-container items cannot exceed this total; no collection grows unbounded.
fn preflight(bytes: &[u8], common: bool) -> Result<&str, DecodeFailure> {
    if bytes.len() > MAX_BYTES {
        return Err(DecodeFailure::Capacity);
    }
    let text = std::str::from_utf8(bytes).map_err(|_| DecodeFailure::Syntax)?;
    let mut depth = 0_usize;
    let mut tokens = 0_usize;
    let mut quoted = false;
    let mut escaped = false;
    let mut atom_bytes = 0_usize;
    let mut atom = false;
    for byte in bytes {
        if quoted {
            if !escaped && *byte == b'"' {
                quoted = false;
                atom_bytes = 0;
                continue;
            }
            atom_bytes += 1;
            if atom_bytes > MAX_ATOM_BYTES {
                return Err(DecodeFailure::Capacity);
            }
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            }
            continue;
        }
        match byte {
            b'"' => {
                quoted = true;
                atom = false;
                atom_bytes = 0;
                tokens += 1;
            }
            b'{' | b'[' => {
                depth += 1;
                tokens += 1;
                atom = false;
                atom_bytes = 0;
            }
            b'}' | b']' => {
                depth = depth.checked_sub(1).ok_or(DecodeFailure::Syntax)?;
                atom = false;
                atom_bytes = 0;
            }
            b':' | b',' | b' ' | b'\r' | b'\n' | b'\t' => {
                atom = false;
                atom_bytes = 0;
            }
            _ => {
                if common && (*byte == b'-' || byte.is_ascii_digit()) {
                    return Err(DecodeFailure::Number);
                }
                if !atom {
                    tokens += 1;
                    atom = true;
                }
                atom_bytes += 1;
            }
        }
        if depth > MAX_DEPTH || tokens > MAX_TOKENS || atom_bytes > MAX_ATOM_BYTES {
            return Err(DecodeFailure::Capacity);
        }
    }
    if quoted || depth != 0 {
        return Err(DecodeFailure::Syntax);
    }
    Ok(text)
}

/// Read one complete JSON value under the bounded reader capacities.
///
/// With `common`, JSON numbers are rejected before tree construction because
/// the shared record encoding represents every quantity as a decimal string.
pub fn value(bytes: &[u8], common: bool) -> Result<Value, DecodeFailure> {
    crate::json::decode_unique_json(preflight(bytes, common)?).map_err(|_| DecodeFailure::Syntax)
}

/// Project one already bounded value onto a closed owner type.
pub fn typed<T: DeserializeOwned>(value: Value) -> Result<T, DecodeFailure> {
    serde_json::from_value(value).map_err(|_| DecodeFailure::Shape)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bounded_reader_checks_limits_before_tree_allocation() {
        assert!(preflight(&vec![b' '; MAX_BYTES], false).is_ok());
        assert_eq!(
            preflight(&vec![b' '; MAX_BYTES + 1], false),
            Err(DecodeFailure::Capacity)
        );
        for (count, accepted) in [(MAX_DEPTH, true), (MAX_DEPTH + 1, false)] {
            let input = format!("{}null{}", "[".repeat(count), "]".repeat(count));
            assert_eq!(value(input.as_bytes(), true).is_ok(), accepted);
        }
        for (count, accepted) in [(MAX_ATOM_BYTES, true), (MAX_ATOM_BYTES + 1, false)] {
            let input = format!("\"{}\"", "x".repeat(count));
            assert_eq!(value(input.as_bytes(), true).is_ok(), accepted);
        }
        // One array container plus one token per element; delimiters do not count.
        for (elements, accepted) in [(MAX_TOKENS - 1, true), (MAX_TOKENS, false)] {
            let input = format!("[{}null]", "null,".repeat(elements - 1));
            assert_eq!(preflight(input.as_bytes(), true).is_ok(), accepted);
        }
    }

    #[test]
    fn decoder_rejects_ambiguity_and_never_coerces_json_numbers() {
        for input in [
            b"{\"a\":null,\"a\":false}".as_slice(),
            b"{\"a\":{\"x\":0,\"x\":1}}",
            b"\xff",
            b"\xef\xbb\xbf{}",
            b"{} trailing",
            b"[}",
            b"\"unfinished",
            b"\"\\uD800\"",
        ] {
            assert!(value(input, false).is_err(), "{input:?}");
        }
        for input in [b"0".as_slice(), b"-1", b"[1e9]", b"{\"value\":1.0}"] {
            assert_eq!(value(input, true), Err(DecodeFailure::Number));
            assert!(value(input, false).is_ok());
        }
        assert!(value(br#"{"value":"123 [\"{}\\]","other":null}"#, true).is_ok());
    }

    #[test]
    fn typed_projection_rejects_values_outside_the_closed_owner_type() {
        #[derive(serde::Deserialize, Debug, PartialEq, Eq)]
        #[serde(deny_unknown_fields)]
        struct Closed {
            name: String,
        }

        let admitted = value(br#"{"name":"one"}"#, true).unwrap();
        assert_eq!(typed::<Closed>(admitted), Ok(Closed { name: "one".into() }));
        for input in [
            br#"{"name":"one","extra":true}"#.as_slice(),
            br#"{"other":"one"}"#,
            br#"{"name":null}"#,
            br#"{"name":["one"]}"#,
            b"null",
            b"true",
        ] {
            let bounded = value(input, true).unwrap();
            assert_eq!(
                typed::<Closed>(bounded),
                Err(DecodeFailure::Shape),
                "{}",
                std::str::from_utf8(input).unwrap()
            );
        }
        // Serde admits a struct's ordered sequence form. Owners that must reject
        // it declare the constraint in their own schema rather than assuming the
        // shared reader forbids it.
        assert!(typed::<Closed>(value(br#"["one"]"#, true).unwrap()).is_ok());
    }
}
