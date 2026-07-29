// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked project catalog scope and locale bindings.
//!
//! This module owns exact resource-side scope and locale values plus the
//! path-capture grammar used by linker-participating catalog definitions. The
//! values are intentionally distinct from similarly named `intlify_contract`
//! types; the later host projection reconstructs contract values explicitly.

use std::fmt;
use std::sync::Arc;

use serde::ser::{Serialize, SerializeStruct, Serializer};

use crate::glob::{matches_segment, parse_segment, GlobToken};
use crate::ProjectRelativeResourcePath;

const BINDING_VALUE_BYTES_LIMIT: u64 = 255;

/// Failure to construct one exact resource-side scope or locale value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CatalogBindingValueError {
    /// The submitted value is empty.
    Empty,
    /// The submitted decoded UTF-8 value exceeds the fixed protocol ceiling.
    Bytes {
        /// Inclusive byte ceiling.
        limit: u64,
        /// Exact submitted decoded UTF-8 byte count.
        observed: u64,
    },
}

impl fmt::Display for CatalogBindingValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("catalog binding value is empty"),
            Self::Bytes { limit, observed } => {
                write!(formatter, "catalog binding bytes {observed} exceed {limit}")
            }
        }
    }
}

impl std::error::Error for CatalogBindingValueError {}

/// Exact project-local catalog scope name.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogScopeName(Arc<str>);

impl CatalogScopeName {
    /// Validate and retain one exact non-empty scope name.
    pub fn try_new(value: impl Into<Arc<str>>) -> Result<Self, CatalogBindingValueError> {
        checked_binding_value(value.into()).map(Self)
    }

    /// Return the exact retained scope spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for CatalogScopeName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

/// Exact resource-resolved locale identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ResolvedLocale(Arc<str>);

impl ResolvedLocale {
    /// Validate and retain one exact non-empty locale spelling.
    pub fn try_new(value: impl Into<Arc<str>>) -> Result<Self, CatalogBindingValueError> {
        checked_binding_value(value.into()).map(Self)
    }

    /// Return the exact retained locale spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Serialize for ResolvedLocale {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

fn checked_binding_value(value: Arc<str>) -> Result<Arc<str>, CatalogBindingValueError> {
    if value.is_empty() {
        return Err(CatalogBindingValueError::Empty);
    }
    let observed = value.len() as u64;
    if observed > BINDING_VALUE_BYTES_LIMIT {
        return Err(CatalogBindingValueError::Bytes {
            limit: BINDING_VALUE_BYTES_LIMIT,
            observed,
        });
    }
    Ok(value)
}

/// Invalid locale capture-pattern spelling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InvalidLocaleCapturePattern;

impl fmt::Display for InvalidLocaleCapturePattern {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("invalid locale capture pattern")
    }
}

impl std::error::Error for InvalidLocaleCapturePattern {}

/// Failure to resolve one checked locale capture against a concrete path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocaleCaptureError {
    /// The complete path does not match the capture pattern.
    Mismatch,
    /// The matched capture is not a representable resource locale.
    InvalidLocale(CatalogBindingValueError),
}

impl fmt::Display for LocaleCaptureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mismatch => formatter.write_str("catalog path does not match locale capture"),
            Self::InvalidLocale(error) => error.fmt(formatter),
        }
    }
}

impl std::error::Error for LocaleCaptureError {}

/// Checked resource glob containing exactly one `{locale}` capture token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleCapturePattern {
    source: Arc<str>,
    segments: Arc<[CaptureGlobSegment]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CaptureGlobSegment {
    Recursive,
    Pattern(Arc<[GlobToken]>),
    Capture(Arc<[CaptureSegmentToken]>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CaptureSegmentToken {
    Literal(char),
    AnyOne,
    Capture,
}

impl LocaleCapturePattern {
    /// Parse one resource-membership glob with exactly one locale capture.
    pub fn parse(source: &str) -> Result<Self, InvalidLocaleCapturePattern> {
        if source.is_empty()
            || source.starts_with('/')
            || source.starts_with("\\\\")
            || has_windows_drive_prefix(source)
        {
            return Err(InvalidLocaleCapturePattern);
        }

        let mut capture_count = 0_u8;
        let mut segments = Vec::new();
        for raw_segment in source.split('/') {
            if raw_segment.is_empty() {
                return Err(InvalidLocaleCapturePattern);
            }
            if raw_segment == "**" {
                segments.push(CaptureGlobSegment::Recursive);
                continue;
            }
            if contains_unescaped_capture(raw_segment) {
                let tokens = parse_capture_segment(raw_segment)?;
                capture_count = capture_count.saturating_add(
                    tokens
                        .iter()
                        .filter(|token| matches!(token, CaptureSegmentToken::Capture))
                        .count() as u8,
                );
                segments.push(CaptureGlobSegment::Capture(tokens));
            } else {
                let tokens = parse_segment(raw_segment).map_err(|_| InvalidLocaleCapturePattern)?;
                segments.push(CaptureGlobSegment::Pattern(tokens));
            }
        }

        if capture_count != 1 {
            return Err(InvalidLocaleCapturePattern);
        }

        Ok(Self {
            source: Arc::from(source),
            segments: Arc::from(segments),
        })
    }

    /// Return the exact submitted capture-pattern spelling.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Resolve the exact captured locale for one complete project-relative path.
    pub fn capture(
        &self,
        path: &ProjectRelativeResourcePath,
    ) -> Result<ResolvedLocale, LocaleCaptureError> {
        let path_segments = path.as_str().split('/').collect::<Vec<_>>();
        let mut previous = vec![None; path_segments.len() + 1];
        previous[0] = Some(None::<Arc<str>>);

        for pattern_segment in self.segments.iter() {
            let mut current = vec![None; path_segments.len() + 1];
            match pattern_segment {
                CaptureGlobSegment::Recursive => {
                    current[0].clone_from(&previous[0]);
                    for index in 1..=path_segments.len() {
                        // Prefer consuming another segment. This keeps matching
                        // deterministic when several `**` partitions fit while
                        // preserving the only already captured value.
                        current[index] = current[index - 1]
                            .clone()
                            .or_else(|| previous[index].clone());
                    }
                }
                CaptureGlobSegment::Pattern(tokens) => {
                    for index in 1..=path_segments.len() {
                        if previous[index - 1].is_some()
                            && matches_segment(tokens, path_segments[index - 1])
                        {
                            current[index].clone_from(&previous[index - 1]);
                        }
                    }
                }
                CaptureGlobSegment::Capture(tokens) => {
                    for index in 1..=path_segments.len() {
                        let Some(None) = &previous[index - 1] else {
                            continue;
                        };
                        if let Some(captured) =
                            match_capture_segment(tokens, path_segments[index - 1])
                        {
                            current[index] = Some(Some(Arc::from(captured)));
                        }
                    }
                }
            }
            previous = current;
        }

        let captured = previous
            .pop()
            .flatten()
            .flatten()
            .ok_or(LocaleCaptureError::Mismatch)?;
        ResolvedLocale::try_new(captured).map_err(LocaleCaptureError::InvalidLocale)
    }
}

impl Serialize for LocaleCapturePattern {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.source())
    }
}

/// Validated locale-binding configuration for one project catalog definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocaleBindingConfig {
    /// Capture one locale from the concrete project-relative path.
    Path {
        /// Checked path-capture pattern.
        pattern: LocaleCapturePattern,
    },
    /// Assign one exact locale to every matching path.
    Fixed {
        /// Checked exact locale value.
        value: ResolvedLocale,
    },
}

impl LocaleBindingConfig {
    /// Resolve this binding for one concrete project-relative path.
    pub fn resolve(
        &self,
        path: &ProjectRelativeResourcePath,
    ) -> Result<ResolvedLocale, LocaleCaptureError> {
        match self {
            Self::Path { pattern } => pattern.capture(path),
            Self::Fixed { value } => Ok(value.clone()),
        }
    }

    /// Return the fixed locale when this is the fixed variant.
    #[must_use]
    pub const fn fixed_locale(&self) -> Option<&ResolvedLocale> {
        match self {
            Self::Path { .. } => None,
            Self::Fixed { value } => Some(value),
        }
    }

    /// Return the path pattern when this is the path variant.
    #[must_use]
    pub const fn path_pattern(&self) -> Option<&LocaleCapturePattern> {
        match self {
            Self::Path { pattern } => Some(pattern),
            Self::Fixed { .. } => None,
        }
    }
}

impl Serialize for LocaleBindingConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Path { pattern } => {
                let mut state = serializer.serialize_struct("LocaleBindingConfig", 2)?;
                state.serialize_field("from", "path")?;
                state.serialize_field("pattern", pattern)?;
                state.end()
            }
            Self::Fixed { value } => {
                let mut state = serializer.serialize_struct("LocaleBindingConfig", 2)?;
                state.serialize_field("from", "fixed")?;
                state.serialize_field("value", value)?;
                state.end()
            }
        }
    }
}

fn contains_unescaped_capture(segment: &str) -> bool {
    let mut escaped = false;
    for (index, character) in segment.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        if character == '\\' {
            escaped = true;
            continue;
        }
        if character == '{' && segment[index..].starts_with("{locale}") {
            return true;
        }
    }
    false
}

fn parse_capture_segment(
    segment: &str,
) -> Result<Arc<[CaptureSegmentToken]>, InvalidLocaleCapturePattern> {
    let mut tokens = Vec::new();
    let mut literal = String::with_capacity(segment.len());
    let mut capture_count = 0_u8;
    let mut characters = segment.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                let escaped = characters.next().ok_or(InvalidLocaleCapturePattern)?;
                if escaped == '/' {
                    return Err(InvalidLocaleCapturePattern);
                }
                tokens.push(CaptureSegmentToken::Literal(escaped));
                literal.push(escaped);
            }
            '{' => {
                for expected in ['l', 'o', 'c', 'a', 'l', 'e', '}'] {
                    if characters.next() != Some(expected) {
                        return Err(InvalidLocaleCapturePattern);
                    }
                }
                capture_count = capture_count.saturating_add(1);
                tokens.push(CaptureSegmentToken::Capture);
            }
            '}' | '*' | '[' | ']' => return Err(InvalidLocaleCapturePattern),
            '?' => tokens.push(CaptureSegmentToken::AnyOne),
            '+' | '@' | '!' if characters.peek() == Some(&'(') => {
                return Err(InvalidLocaleCapturePattern);
            }
            literal_character => {
                tokens.push(CaptureSegmentToken::Literal(literal_character));
                literal.push(literal_character);
            }
        }
    }

    if capture_count != 1 || matches!(literal.as_str(), "." | "..") {
        return Err(InvalidLocaleCapturePattern);
    }
    Ok(Arc::from(tokens))
}

fn match_capture_segment<'a>(tokens: &[CaptureSegmentToken], segment: &'a str) -> Option<&'a str> {
    let characters = segment.char_indices().collect::<Vec<_>>();
    let capture_index = tokens
        .iter()
        .position(|token| matches!(token, CaptureSegmentToken::Capture))?;
    let fixed_tokens = tokens.len().checked_sub(1)?;
    if characters.len() <= fixed_tokens {
        return None;
    }
    let captured_characters = characters.len() - fixed_tokens;

    for (token, (_, actual)) in tokens[..capture_index]
        .iter()
        .zip(characters.iter().copied())
    {
        if !capture_token_matches(*token, actual) {
            return None;
        }
    }

    let suffix_character_start = capture_index + captured_characters;
    for (token, (_, actual)) in tokens[capture_index + 1..]
        .iter()
        .zip(characters[suffix_character_start..].iter().copied())
    {
        if !capture_token_matches(*token, actual) {
            return None;
        }
    }

    let start = characters
        .get(capture_index)
        .map_or(segment.len(), |(offset, _)| *offset);
    let end = characters
        .get(suffix_character_start)
        .map_or(segment.len(), |(offset, _)| *offset);
    (start < end).then(|| &segment[start..end])
}

const fn capture_token_matches(token: CaptureSegmentToken, actual: char) -> bool {
    match token {
        CaptureSegmentToken::Literal(expected) => expected == actual,
        CaptureSegmentToken::AnyOne => true,
        CaptureSegmentToken::Capture => false,
    }
}

fn has_windows_drive_prefix(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogBindingValueError, CatalogScopeName, LocaleCaptureError, LocaleCapturePattern,
        ResolvedLocale,
    };
    use crate::ProjectRelativeResourcePath;

    fn path(value: &str) -> ProjectRelativeResourcePath {
        ProjectRelativeResourcePath::try_from(value).unwrap()
    }

    #[test]
    fn scope_and_locale_preserve_exact_values_at_byte_boundaries() {
        for value in ["a", "EN_us", "  ", "\u{0001}", "é", "e\u{301}"] {
            assert_eq!(CatalogScopeName::try_new(value).unwrap().as_str(), value);
            assert_eq!(ResolvedLocale::try_new(value).unwrap().as_str(), value);
        }
        assert!(CatalogScopeName::try_new("a".repeat(255)).is_ok());
        assert!(ResolvedLocale::try_new("a".repeat(255)).is_ok());
        assert_eq!(
            CatalogScopeName::try_new(""),
            Err(CatalogBindingValueError::Empty)
        );
        assert_eq!(
            ResolvedLocale::try_new("a".repeat(256)),
            Err(CatalogBindingValueError::Bytes {
                limit: 255,
                observed: 256,
            })
        );
    }

    #[test]
    fn locale_capture_uses_resource_globs_plus_one_capture() {
        for pattern in [
            "locales/{locale}.json",
            "**/{locale}/messages.json",
            "locales/?-{locale}.json",
            "literal/\\*/{locale}.json",
            "literal/\\{locale\\}/{locale}.json",
        ] {
            LocaleCapturePattern::parse(pattern)
                .unwrap_or_else(|_| panic!("valid capture pattern: {pattern}"));
        }

        for pattern in [
            "",
            "/locales/{locale}.json",
            "locales/*.json",
            "locales/{locale}/{locale}.json",
            "locales/pre*{locale}.json",
            "locales/{other}.json",
            "locales/\\{locale\\}.json",
            "locales/[a]/{locale}.json",
            "locales/{locale",
        ] {
            assert!(
                LocaleCapturePattern::parse(pattern).is_err(),
                "invalid capture pattern: {pattern}"
            );
        }
    }

    #[test]
    fn locale_capture_matches_complete_paths_and_preserves_exact_capture() {
        let pattern = LocaleCapturePattern::parse("locales/?-{locale}.json").unwrap();
        assert_eq!(
            pattern
                .capture(&path("locales/x-JA_jp.json"))
                .unwrap()
                .as_str(),
            "JA_jp"
        );
        assert_eq!(
            pattern.capture(&path("other/x-JA_jp.json")),
            Err(LocaleCaptureError::Mismatch)
        );

        let recursive = LocaleCapturePattern::parse("**/{locale}/messages.json").unwrap();
        assert_eq!(
            recursive
                .capture(&path("packages/app/en/messages.json"))
                .unwrap()
                .as_str(),
            "en"
        );
    }

    #[test]
    fn locale_capture_rejects_empty_or_overlong_results_without_truncation() {
        let pattern = LocaleCapturePattern::parse("locales/{locale}.json").unwrap();
        assert_eq!(
            pattern.capture(&path("locales/.json")),
            Err(LocaleCaptureError::Mismatch)
        );
        assert_eq!(
            pattern.capture(&path(&format!("locales/{}.json", "a".repeat(256)))),
            Err(LocaleCaptureError::InvalidLocale(
                CatalogBindingValueError::Bytes {
                    limit: 255,
                    observed: 256,
                }
            ))
        );
    }
}
