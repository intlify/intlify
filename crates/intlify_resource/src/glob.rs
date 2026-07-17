// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::sync::Arc;

use serde::ser::{Serialize, Serializer};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceGlob {
    source: Arc<str>,
    segments: Arc<[GlobSegment]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum GlobSegment {
    Recursive,
    Pattern(Arc<[GlobToken]>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GlobToken {
    Literal(char),
    AnyOne,
    AnyMany,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct InvalidResourceGlob;

impl ResourceGlob {
    pub(crate) fn parse(source: &str) -> Result<Self, InvalidResourceGlob> {
        if source.is_empty()
            || source.starts_with('/')
            || source.starts_with("\\\\")
            || has_windows_drive_prefix(source)
        {
            return Err(InvalidResourceGlob);
        }

        let mut segments = Vec::new();
        for raw_segment in source.split('/') {
            if raw_segment.is_empty() {
                return Err(InvalidResourceGlob);
            }
            if raw_segment == "**" {
                segments.push(GlobSegment::Recursive);
            } else {
                segments.push(GlobSegment::Pattern(parse_segment(raw_segment)?));
            }
        }

        Ok(Self {
            source: Arc::from(source),
            segments: Arc::from(segments),
        })
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn is_match(&self, path: &str) -> bool {
        let path_segments = path.split('/').collect::<Vec<_>>();
        let mut previous = vec![false; path_segments.len() + 1];
        previous[0] = true;

        for segment in self.segments.iter() {
            let mut current = vec![false; path_segments.len() + 1];
            match segment {
                GlobSegment::Recursive => {
                    current[0] = previous[0];
                    for index in 1..=path_segments.len() {
                        current[index] = previous[index] || current[index - 1];
                    }
                }
                GlobSegment::Pattern(tokens) => {
                    for index in 1..=path_segments.len() {
                        current[index] = previous[index - 1]
                            && matches_segment(tokens, path_segments[index - 1]);
                    }
                }
            }
            previous = current;
        }

        previous[path_segments.len()]
    }
}

impl Serialize for ResourceGlob {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.source())
    }
}

fn has_windows_drive_prefix(source: &str) -> bool {
    let bytes = source.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

fn parse_segment(segment: &str) -> Result<Arc<[GlobToken]>, InvalidResourceGlob> {
    let mut tokens = Vec::new();
    let mut literal = String::with_capacity(segment.len());
    let mut has_wildcard = false;
    let mut characters = segment.chars().peekable();

    while let Some(character) = characters.next() {
        match character {
            '\\' => {
                let escaped = characters.next().ok_or(InvalidResourceGlob)?;
                if escaped == '/' {
                    return Err(InvalidResourceGlob);
                }
                tokens.push(GlobToken::Literal(escaped));
                literal.push(escaped);
            }
            '*' => {
                if matches!(characters.peek(), Some('*' | '(')) {
                    return Err(InvalidResourceGlob);
                }
                tokens.push(GlobToken::AnyMany);
                has_wildcard = true;
            }
            '?' => {
                if characters.peek() == Some(&'(') {
                    return Err(InvalidResourceGlob);
                }
                tokens.push(GlobToken::AnyOne);
                has_wildcard = true;
            }
            '[' | ']' | '{' | '}' => return Err(InvalidResourceGlob),
            '+' | '@' | '!' if characters.peek() == Some(&'(') => return Err(InvalidResourceGlob),
            literal_character => {
                tokens.push(GlobToken::Literal(literal_character));
                literal.push(literal_character);
            }
        }
    }

    if !has_wildcard && matches!(literal.as_str(), "." | "..") {
        return Err(InvalidResourceGlob);
    }

    Ok(Arc::from(tokens))
}

fn matches_segment(tokens: &[GlobToken], segment: &str) -> bool {
    let characters = segment.chars().collect::<Vec<_>>();
    let mut previous = vec![false; characters.len() + 1];
    previous[0] = true;

    for token in tokens {
        let mut current = vec![false; characters.len() + 1];
        match token {
            GlobToken::Literal(expected) => {
                for index in 1..=characters.len() {
                    current[index] = previous[index - 1] && characters[index - 1] == *expected;
                }
            }
            GlobToken::AnyOne => {
                current[1..].copy_from_slice(&previous[..characters.len()]);
            }
            GlobToken::AnyMany => {
                current[0] = previous[0];
                for index in 1..=characters.len() {
                    current[index] = previous[index] || current[index - 1];
                }
            }
        }
        previous = current;
    }

    previous[characters.len()]
}

#[cfg(test)]
mod tests {
    use super::ResourceGlob;

    #[test]
    fn accepts_the_fixed_resource_glob_grammar() {
        for pattern in [
            "messages.json",
            "locales/*.json",
            "locales/??.json",
            "locales/**/*.json",
            "**/messages.json",
            "literal/\\*.json",
            "literal/\\?.json",
            "literal/\\[name\\].json",
            "literal/!important.json",
            "unicode/日本語.json",
        ] {
            ResourceGlob::parse(pattern).unwrap_or_else(|_| panic!("valid pattern: {pattern}"));
        }
    }

    #[test]
    fn rejects_unsupported_or_non_relative_patterns() {
        for pattern in [
            "",
            "/absolute.json",
            "C:/absolute.json",
            "c:relative.json",
            r"\\server\share\messages.json",
            "locales//en.json",
            "locales/",
            "./locales/*.json",
            "locales/../messages.json",
            "locales/foo**bar.json",
            "locales/***.json",
            "locales/[a].json",
            "locales/{a,b}.json",
            "locales/@(a|b).json",
            "locales/!(generated).json",
            "locales/escaped\\/slash.json",
            "locales/trailing\\",
        ] {
            assert!(
                ResourceGlob::parse(pattern).is_err(),
                "invalid pattern: {pattern}"
            );
        }
    }

    #[test]
    fn matches_segments_case_sensitively_without_normalization() {
        let cases = [
            ("locales/*.json", "locales/en.json", true),
            ("locales/*.json", "locales/nested/en.json", false),
            ("locales/**/*.json", "locales/en.json", true),
            ("locales/**/*.json", "locales/nested/en.json", true),
            ("**/messages.json", "messages.json", true),
            ("**/messages.json", "a/b/messages.json", true),
            ("locales/??.json", "locales/日本.json", true),
            ("locales/??.json", "locales/日.json", false),
            ("locales/*.json", "Locales/en.json", false),
            ("literal/\\*.json", "literal/*.json", true),
            ("literal/!important.json", "literal/!important.json", true),
        ];

        for (pattern, path, expected) in cases {
            assert_eq!(
                ResourceGlob::parse(pattern).unwrap().is_match(path),
                expected,
                "pattern={pattern}, path={path}"
            );
        }
    }
}
