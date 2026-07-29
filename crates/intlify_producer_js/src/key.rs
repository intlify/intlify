// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Source-facing string to canonical selector conversion.
//!
//! Conversion is driven only by the checked recognizer's call kind, domain, and
//! key syntax. It does not inspect catalog definitions, infer patterns from a
//! lookup string, or retry a rejected value under another syntax.

use intlify_contract::{CatalogKey, CatalogKeyPattern, MessageSelector};

use crate::{JsKeySyntax, JsRecognizerBinding, JsRecognizerCallKind};

pub(crate) fn convert_static_selector(
    binding: &JsRecognizerBinding,
    value: &str,
) -> Result<MessageSelector, SelectorConversionError> {
    match (binding.kind(), binding.key_syntax()) {
        (JsRecognizerCallKind::Lookup, JsKeySyntax::Canonical) => {
            CatalogKey::try_new(binding.domain(), value)
                .map(MessageSelector::Exact)
                .map_err(|_| SelectorConversionError)
        }
        (JsRecognizerCallKind::Set, JsKeySyntax::Canonical) => {
            CatalogKeyPattern::try_new(binding.domain(), value)
                .map(MessageSelector::Pattern)
                .map_err(|_| SelectorConversionError)
        }
        (JsRecognizerCallKind::Lookup, JsKeySyntax::Literal) => {
            let canonical = one_literal_segment(value, false);
            CatalogKey::try_new(binding.domain(), canonical)
                .map(MessageSelector::Exact)
                .map_err(|_| SelectorConversionError)
        }
        (JsRecognizerCallKind::Set, JsKeySyntax::Literal) => {
            let canonical = one_literal_segment(value, true);
            CatalogKeyPattern::try_new(binding.domain(), canonical)
                .map(MessageSelector::Pattern)
                .map_err(|_| SelectorConversionError)
        }
        (JsRecognizerCallKind::Lookup, JsKeySyntax::DotPath) => {
            let segments = parse_dot_path(value)?;
            let canonical = exact_dot_path(&segments);
            CatalogKey::try_new(binding.domain(), canonical)
                .map(MessageSelector::Exact)
                .map_err(|_| SelectorConversionError)
        }
        (JsRecognizerCallKind::Set, JsKeySyntax::DotPath) => {
            let segments = parse_dot_path(value)?;
            let canonical = pattern_dot_path(&segments);
            CatalogKeyPattern::try_new(binding.domain(), canonical)
                .map(MessageSelector::Pattern)
                .map_err(|_| SelectorConversionError)
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SelectorConversionError;

fn one_literal_segment(value: &str, pattern: bool) -> String {
    let mut canonical = String::with_capacity(value.len() + 1);
    canonical.push('/');
    push_escaped_segment(&mut canonical, value, pattern);
    canonical
}

fn exact_dot_path(segments: &[String]) -> String {
    let capacity = segments.iter().map(String::len).sum::<usize>() + segments.len();
    let mut canonical = String::with_capacity(capacity);
    for segment in segments {
        canonical.push('/');
        push_escaped_segment(&mut canonical, segment, false);
    }
    canonical
}

fn pattern_dot_path(segments: &[String]) -> String {
    let capacity = segments.iter().map(String::len).sum::<usize>() + segments.len();
    let mut canonical = String::with_capacity(capacity);
    for segment in segments {
        canonical.push('/');
        if matches!(segment.as_str(), "*" | "**") {
            canonical.push_str(segment);
        } else {
            push_escaped_segment(&mut canonical, segment, true);
        }
    }
    canonical
}

fn parse_dot_path(value: &str) -> Result<Vec<String>, SelectorConversionError> {
    if value.is_empty() {
        return Err(SelectorConversionError);
    }

    let mut segments = Vec::new();
    let mut current = String::new();
    let mut characters = value.chars();
    while let Some(character) = characters.next() {
        match character {
            '.' => {
                if current.is_empty() {
                    return Err(SelectorConversionError);
                }
                segments.push(std::mem::take(&mut current));
            }
            '\\' => match characters.next() {
                Some(escaped @ ('.' | '\\')) => current.push(escaped),
                _ => return Err(SelectorConversionError),
            },
            other => current.push(other),
        }
    }

    if current.is_empty() {
        return Err(SelectorConversionError);
    }
    segments.push(current);
    Ok(segments)
}

fn push_escaped_segment(output: &mut String, segment: &str, pattern: bool) {
    for character in segment.chars() {
        match character {
            '~' => output.push_str("~0"),
            '/' => output.push_str("~1"),
            '*' if pattern => output.push_str("~2"),
            other => output.push(other),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_dot_path;

    #[test]
    fn dot_path_accepts_only_dot_and_backslash_escapes() {
        assert_eq!(
            parse_dot_path(r"checkout.form\.title\\suffix").unwrap(),
            ["checkout", "form.title\\suffix"]
        );
        for invalid in ["", ".a", "a.", "a..b", r"a\q", r"a\"] {
            assert!(parse_dot_path(invalid).is_err(), "{invalid:?}");
        }
    }
}
