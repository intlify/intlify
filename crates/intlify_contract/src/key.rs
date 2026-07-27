// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::cmp::Ordering;

use crate::error::{ValueConstructionError, ValueGrammar, ValueLimit, ValueLimitKind};
use crate::fingerprint::FingerprintPayload;
use crate::{ArtifactLimitEvidence, LinkLimitCounter, LinkLimitObservation, LinkLimits};

const CATALOG_KEY_BYTES: usize = 67_108_864;
const CATALOG_KEY_TOKENS: usize = 256;
const SELECTOR_PATTERN_BYTES: usize = 134_217_728;
const SELECTOR_PATTERN_TOKENS: usize = 513;

/// Closed catalog-key comparison domains admitted by M0 artifacts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum CatalogKeyDomain {
    /// RFC 6901 JSON Pointer.
    JsonPointer,
    /// YAML Core-Schema typed path.
    YamlTypedPath,
    /// XLIFF 1.2 file/group/unit hierarchy.
    Xliff12,
    /// XLIFF 2.x file/group/unit/segment hierarchy.
    Xliff2,
}

impl CatalogKeyDomain {
    /// Return the exact v0.1 wire token.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::JsonPointer => "json-pointer",
            Self::YamlTypedPath => "yaml-typed-path",
            Self::Xliff12 => "xliff-1.2",
            Self::Xliff2 => "xliff-2",
        }
    }
}

impl FingerprintPayload for CatalogKeyDomain {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.as_str().as_bytes());
    }
}

/// One exact decoded structural token in a checked catalog key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CatalogKeyToken(Box<str>);

impl CatalogKeyToken {
    /// Return the exact decoded token spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One complete canonical catalog key interpreted in its comparison domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogKey {
    domain: CatalogKeyDomain,
    canonical: Box<str>,
    tokens: Box<[CatalogKeyToken]>,
}

impl CatalogKey {
    /// Validate one complete canonical key under the selected domain.
    pub fn try_new(
        domain: CatalogKeyDomain,
        canonical: impl Into<Box<str>>,
    ) -> Result<Self, ValueConstructionError> {
        let canonical = canonical.into();
        check_structural_bytes(&canonical, CATALOG_KEY_BYTES)?;
        let tokens = parse_catalog_path(domain, &canonical, PathContract::Complete)?;
        Ok(Self {
            domain,
            canonical,
            tokens: tokens.into_boxed_slice(),
        })
    }

    /// Return the comparison domain used to validate this key.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the exact canonical serialized key.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Return the exact decoded structural tokens.
    #[must_use]
    pub fn tokens(&self) -> &[CatalogKeyToken] {
        &self.tokens
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_definition_bytes(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_first_over(
            self.canonical.len() as u64,
            LinkLimitCounter::CatalogKeyBytes,
            limits,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_selector_bytes(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_first_over(
            self.canonical.len() as u64,
            LinkLimitCounter::SelectorPathBytes,
            limits,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_tokens(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_first_over(
            self.tokens.len() as u64,
            LinkLimitCounter::CatalogKeyTokens,
            limits,
        )
    }
}

impl PartialOrd for CatalogKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CatalogKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.domain
            .cmp(&other.domain)
            .then_with(|| self.canonical.as_bytes().cmp(other.canonical.as_bytes()))
    }
}

impl FingerprintPayload for CatalogKey {
    fn write_fingerprint_payload(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(self.canonical.as_bytes());
    }
}

/// Non-empty structural ancestor selector in one catalog-key domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogKeyPrefix {
    domain: CatalogKeyDomain,
    canonical: Box<str>,
    tokens: Box<[CatalogKeyToken]>,
}

impl CatalogKeyPrefix {
    /// Validate one non-empty canonical prefix under the selected domain.
    pub fn try_new(
        domain: CatalogKeyDomain,
        canonical: impl Into<Box<str>>,
    ) -> Result<Self, ValueConstructionError> {
        let canonical = canonical.into();
        if canonical.is_empty() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
        }
        check_structural_bytes(&canonical, CATALOG_KEY_BYTES)?;
        let tokens = parse_catalog_path(domain, &canonical, PathContract::Prefix)?;
        Ok(Self {
            domain,
            canonical,
            tokens: tokens.into_boxed_slice(),
        })
    }

    /// Return the comparison domain used to validate this prefix.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the exact canonical serialized prefix.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Return the exact decoded structural prefix tokens.
    #[must_use]
    pub fn tokens(&self) -> &[CatalogKeyToken] {
        &self.tokens
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_bytes(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_first_over(
            self.canonical.len() as u64,
            LinkLimitCounter::SelectorPathBytes,
            limits,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_tokens(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_first_over(
            self.tokens.len() as u64,
            LinkLimitCounter::CatalogKeyTokens,
            limits,
        )
    }
}

impl PartialOrd for CatalogKeyPrefix {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CatalogKeyPrefix {
    fn cmp(&self, other: &Self) -> Ordering {
        self.domain
            .cmp(&other.domain)
            .then_with(|| self.canonical.as_bytes().cmp(other.canonical.as_bytes()))
    }
}

/// One checked structural pattern token.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PatternToken {
    /// One exact decoded domain token.
    Literal(CatalogKeyToken),
    /// Exactly one complete candidate token.
    AnyOne,
    /// Zero or more complete candidate tokens.
    AnyMany,
}

/// Non-empty canonical structural pattern in one catalog-key domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CatalogKeyPattern {
    domain: CatalogKeyDomain,
    canonical: Box<str>,
    tokens: Box<[PatternToken]>,
}

impl CatalogKeyPattern {
    /// Parse, validate, and retain one bounded canonical pattern.
    pub fn try_new(
        domain: CatalogKeyDomain,
        canonical: impl Into<Box<str>>,
    ) -> Result<Self, ValueConstructionError> {
        let canonical = canonical.into();
        if canonical.is_empty() {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Empty));
        }
        check_structural_bytes(&canonical, SELECTOR_PATTERN_BYTES)?;
        let tokens = parse_pattern(domain, &canonical)?;
        Ok(Self {
            domain,
            canonical,
            tokens: tokens.into_boxed_slice(),
        })
    }

    /// Return the comparison domain used to validate this pattern.
    #[must_use]
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Return the exact canonical pattern spelling.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.canonical
    }

    /// Return the checked bounded matcher input.
    #[must_use]
    pub fn tokens(&self) -> &[PatternToken] {
        &self.tokens
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_bytes(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_first_over(
            self.canonical.len() as u64,
            LinkLimitCounter::SelectorPatternBytes,
            limits,
        )
    }

    #[allow(dead_code)]
    pub(crate) fn revalidate_tokens(
        &self,
        limits: &LinkLimits,
    ) -> Result<(), ArtifactLimitEvidence> {
        revalidate_first_over(
            self.tokens.len() as u64,
            LinkLimitCounter::SelectorPatternTokens,
            limits,
        )
    }
}

impl PartialOrd for CatalogKeyPattern {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CatalogKeyPattern {
    fn cmp(&self, other: &Self) -> Ordering {
        self.domain
            .cmp(&other.domain)
            .then_with(|| self.canonical.as_bytes().cmp(other.canonical.as_bytes()))
    }
}

/// Closed M0 message selector vocabulary in canonical variant order.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MessageSelector {
    /// Select exactly one canonical key.
    Exact(CatalogKey),
    /// Select one structural root and all descendants.
    Prefix(CatalogKeyPrefix),
    /// Select keys matching a checked bounded structural pattern.
    Pattern(CatalogKeyPattern),
    /// Select every key in the enclosing exact scope-domain pair.
    AllInScope,
    /// Record that a recognized lookup could not be statically bounded.
    UnboundedDynamic,
}

impl MessageSelector {
    /// Return the payload's checked domain, if this variant carries one.
    #[must_use]
    pub const fn domain(&self) -> Option<CatalogKeyDomain> {
        match self {
            Self::Exact(key) => Some(key.domain()),
            Self::Prefix(prefix) => Some(prefix.domain()),
            Self::Pattern(pattern) => Some(pattern.domain()),
            Self::AllInScope | Self::UnboundedDynamic => None,
        }
    }

    /// Return whether this selector is valid inside the supplied enclosing domain.
    #[must_use]
    pub fn is_for_domain(&self, domain: CatalogKeyDomain) -> bool {
        match self.domain() {
            Some(payload_domain) => payload_domain == domain,
            None => true,
        }
    }

    /// Return the exact v0.1 selector kind token.
    #[must_use]
    pub const fn kind_str(&self) -> &'static str {
        match self {
            Self::Exact(_) => "exact",
            Self::Prefix(_) => "prefix",
            Self::Pattern(_) => "pattern",
            Self::AllInScope => "all-in-scope",
            Self::UnboundedDynamic => "unbounded-dynamic",
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum PathContract {
    Complete,
    Prefix,
}

fn parse_catalog_path(
    domain: CatalogKeyDomain,
    canonical: &str,
    contract: PathContract,
) -> Result<Vec<CatalogKeyToken>, ValueConstructionError> {
    if canonical.is_empty() {
        return match (domain, contract) {
            (
                CatalogKeyDomain::JsonPointer | CatalogKeyDomain::YamlTypedPath,
                PathContract::Complete,
            ) => Ok(Vec::new()),
            _ => Err(ValueConstructionError::Grammar(ValueGrammar::Empty)),
        };
    }
    let raw_segments = pointer_segments(canonical)?;
    let mut tokens = Vec::new();
    for raw_segment in raw_segments {
        if tokens.len() == CATALOG_KEY_TOKENS {
            return Err(ValueConstructionError::StructuralLimit(ValueLimit::new(
                ValueLimitKind::Tokens,
                CATALOG_KEY_TOKENS as u64,
                CATALOG_KEY_TOKENS as u64 + 1,
            )));
        }
        let decoded = decode_segment(raw_segment, EscapeContract::CatalogKey)?;
        validate_domain_token(domain, &decoded)?;
        tokens.push(CatalogKeyToken(decoded.into_boxed_str()));
    }

    validate_path_shape(domain, &tokens, contract)?;
    Ok(tokens)
}

fn parse_pattern(
    domain: CatalogKeyDomain,
    canonical: &str,
) -> Result<Vec<PatternToken>, ValueConstructionError> {
    let raw_segments = pointer_segments(canonical)?;
    let mut tokens = Vec::new();
    let mut previous_was_many = false;
    for raw_segment in raw_segments {
        if tokens.len() == SELECTOR_PATTERN_TOKENS {
            return Err(ValueConstructionError::StructuralLimit(ValueLimit::new(
                ValueLimitKind::Tokens,
                SELECTOR_PATTERN_TOKENS as u64,
                SELECTOR_PATTERN_TOKENS as u64 + 1,
            )));
        }

        let token = match raw_segment {
            "*" => PatternToken::AnyOne,
            "**" => PatternToken::AnyMany,
            _ => {
                if raw_segment.contains('*') {
                    return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
                }
                let decoded = decode_segment(raw_segment, EscapeContract::Pattern)?;
                validate_domain_token(domain, &decoded)?;
                PatternToken::Literal(CatalogKeyToken(decoded.into_boxed_str()))
            }
        };

        let current_is_many = matches!(token, PatternToken::AnyMany);
        if previous_was_many && current_is_many {
            return Err(ValueConstructionError::Grammar(
                ValueGrammar::NonCanonicalOrder,
            ));
        }
        previous_was_many = current_is_many;
        tokens.push(token);
    }
    Ok(tokens)
}

fn pointer_segments(value: &str) -> Result<impl Iterator<Item = &str>, ValueConstructionError> {
    value
        .strip_prefix('/')
        .map(|tail| tail.split('/'))
        .ok_or(ValueConstructionError::Grammar(ValueGrammar::Invalid))
}

#[derive(Debug, Clone, Copy)]
enum EscapeContract {
    CatalogKey,
    Pattern,
}

fn decode_segment(raw: &str, contract: EscapeContract) -> Result<String, ValueConstructionError> {
    let mut decoded = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(character) = chars.next() {
        if character != '~' {
            decoded.push(character);
            continue;
        }
        let Some(escape) = chars.next() else {
            return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
        };
        match (contract, escape) {
            (_, '0') => decoded.push('~'),
            (_, '1') => decoded.push('/'),
            (EscapeContract::Pattern, '2') => decoded.push('*'),
            (EscapeContract::CatalogKey | EscapeContract::Pattern, _) => {
                return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
            }
        }
    }

    let encoded = encode_segment(&decoded, contract);
    if encoded != raw {
        return Err(ValueConstructionError::Grammar(ValueGrammar::Invalid));
    }
    Ok(decoded)
}

fn encode_segment(value: &str, contract: EscapeContract) -> String {
    let mut encoded = String::with_capacity(value.len());
    for character in value.chars() {
        match (contract, character) {
            (_, '~') => encoded.push_str("~0"),
            (_, '/') => encoded.push_str("~1"),
            (EscapeContract::Pattern, '*') => encoded.push_str("~2"),
            (EscapeContract::CatalogKey | EscapeContract::Pattern, _) => {
                encoded.push(character);
            }
        }
    }
    encoded
}

fn validate_domain_token(
    domain: CatalogKeyDomain,
    token: &str,
) -> Result<(), ValueConstructionError> {
    let valid = match domain {
        CatalogKeyDomain::JsonPointer => true,
        CatalogKeyDomain::YamlTypedPath => valid_yaml_token(token),
        CatalogKeyDomain::Xliff12 => valid_xliff12_token(token),
        CatalogKeyDomain::Xliff2 => valid_xliff2_token(token),
    };
    if valid {
        Ok(())
    } else {
        Err(ValueConstructionError::Grammar(ValueGrammar::Invalid))
    }
}

fn validate_path_shape(
    domain: CatalogKeyDomain,
    tokens: &[CatalogKeyToken],
    contract: PathContract,
) -> Result<(), ValueConstructionError> {
    let valid = match domain {
        CatalogKeyDomain::JsonPointer | CatalogKeyDomain::YamlTypedPath => true,
        CatalogKeyDomain::Xliff12 => valid_xliff12_path(tokens, contract),
        CatalogKeyDomain::Xliff2 => valid_xliff2_path(tokens, contract),
    };
    if valid {
        Ok(())
    } else {
        Err(ValueConstructionError::Grammar(ValueGrammar::Invalid))
    }
}

fn valid_yaml_token(token: &str) -> bool {
    if token.strip_prefix("k:str:").is_some()
        || matches!(token, "k:null" | "k:bool:true" | "k:bool:false")
    {
        return true;
    }
    if let Some(integer) = token.strip_prefix("k:int:") {
        return valid_canonical_integer(integer);
    }
    if let Some(float) = token.strip_prefix("k:float:") {
        return valid_canonical_float(float);
    }
    token
        .strip_prefix("i:")
        .is_some_and(valid_canonical_unsigned)
}

fn valid_canonical_unsigned(value: &str) -> bool {
    value == "0"
        || value
            .strip_prefix(|character: char| matches!(character, '1'..='9'))
            .is_some_and(|rest| rest.bytes().all(|byte| byte.is_ascii_digit()))
}

fn valid_canonical_integer(value: &str) -> bool {
    if value == "0" {
        return true;
    }
    let unsigned = value.strip_prefix('-').unwrap_or(value);
    !unsigned.is_empty()
        && unsigned.as_bytes()[0].is_ascii_digit()
        && unsigned.as_bytes()[0] != b'0'
        && unsigned.bytes().all(|byte| byte.is_ascii_digit())
}

fn valid_canonical_float(value: &str) -> bool {
    if matches!(value, ".inf" | "-.inf" | ".nan" | "0e0") {
        return true;
    }
    let Some((coefficient, exponent)) = value.split_once('e') else {
        return false;
    };
    if exponent.contains('e') {
        return false;
    }
    let coefficient = coefficient.strip_prefix('-').unwrap_or(coefficient);
    let negative_zero_exponent = exponent == "-0";
    let exponent = exponent.strip_prefix('-').unwrap_or(exponent);
    !coefficient.is_empty()
        && coefficient.as_bytes()[0] != b'0'
        && coefficient.as_bytes()[coefficient.len() - 1] != b'0'
        && coefficient.bytes().all(|byte| byte.is_ascii_digit())
        && !negative_zero_exponent
        && valid_canonical_unsigned(exponent)
}

fn valid_xliff12_token(token: &str) -> bool {
    valid_payload_token(token, "file:original:")
        || valid_index_token(token, "file:i:")
        || valid_payload_token(token, "group:id:")
        || valid_index_token(token, "group:i:")
        || valid_payload_token(token, "unit:id:")
        || valid_index_token(token, "unit:i:")
}

fn valid_xliff2_token(token: &str) -> bool {
    valid_payload_token(token, "file:id:")
        || valid_index_token(token, "file:i:")
        || valid_payload_token(token, "group:id:")
        || valid_index_token(token, "group:i:")
        || valid_payload_token(token, "unit:id:")
        || valid_index_token(token, "unit:i:")
        || valid_payload_token(token, "segment:id:")
        || valid_index_token(token, "segment:i:")
}

fn valid_payload_token(token: &str, prefix: &str) -> bool {
    token
        .strip_prefix(prefix)
        .is_some_and(|payload| !payload.is_empty())
}

fn valid_index_token(token: &str, prefix: &str) -> bool {
    token
        .strip_prefix(prefix)
        .is_some_and(valid_canonical_unsigned)
}

fn valid_xliff12_path(tokens: &[CatalogKeyToken], contract: PathContract) -> bool {
    let Some((first, rest)) = tokens.split_first() else {
        return false;
    };
    if !is_xliff12_file(first.as_str()) {
        return false;
    }
    let group_count = rest
        .iter()
        .take_while(|token| is_xliff_group(token.as_str()))
        .count();
    let tail = &rest[group_count..];
    match contract {
        PathContract::Complete => tail.len() == 1 && is_xliff_unit(tail[0].as_str()),
        PathContract::Prefix => {
            tail.is_empty() || (tail.len() == 1 && is_xliff_unit(tail[0].as_str()))
        }
    }
}

fn valid_xliff2_path(tokens: &[CatalogKeyToken], contract: PathContract) -> bool {
    let Some((first, rest)) = tokens.split_first() else {
        return false;
    };
    if !is_xliff2_file(first.as_str()) {
        return false;
    }
    let group_count = rest
        .iter()
        .take_while(|token| is_xliff_group(token.as_str()))
        .count();
    let tail = &rest[group_count..];
    match contract {
        PathContract::Complete => {
            tail.len() == 2 && is_xliff_unit(tail[0].as_str()) && is_xliff_segment(tail[1].as_str())
        }
        PathContract::Prefix => match tail {
            [] => true,
            [unit] => is_xliff_unit(unit.as_str()),
            [unit, segment] => is_xliff_unit(unit.as_str()) && is_xliff_segment(segment.as_str()),
            _ => false,
        },
    }
}

fn is_xliff12_file(token: &str) -> bool {
    valid_payload_token(token, "file:original:") || valid_index_token(token, "file:i:")
}

fn is_xliff2_file(token: &str) -> bool {
    valid_payload_token(token, "file:id:") || valid_index_token(token, "file:i:")
}

fn is_xliff_group(token: &str) -> bool {
    valid_payload_token(token, "group:id:") || valid_index_token(token, "group:i:")
}

fn is_xliff_unit(token: &str) -> bool {
    valid_payload_token(token, "unit:id:") || valid_index_token(token, "unit:i:")
}

fn is_xliff_segment(token: &str) -> bool {
    valid_payload_token(token, "segment:id:") || valid_index_token(token, "segment:i:")
}

fn check_structural_bytes(value: &str, limit: usize) -> Result<(), ValueConstructionError> {
    if value.len() > limit {
        return Err(ValueConstructionError::StructuralLimit(ValueLimit::new(
            ValueLimitKind::Bytes,
            limit as u64,
            limit as u64 + 1,
        )));
    }
    Ok(())
}

fn revalidate_first_over(
    observed: u64,
    counter: LinkLimitCounter,
    limits: &LinkLimits,
) -> Result<(), ArtifactLimitEvidence> {
    let limit = limits.effective_limit(counter);
    if observed > limit {
        return Err(ArtifactLimitEvidence::try_new(
            counter,
            limit,
            LinkLimitObservation::Exact(limit + 1),
        )
        .expect("checked first-over key or selector evidence is valid"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogKey, CatalogKeyDomain, CatalogKeyPattern, CatalogKeyPrefix, MessageSelector,
        PatternToken,
    };
    use crate::fingerprint::FingerprintPayload;
    use crate::{
        LinkLimitCounter, LinkLimitObservation, LinkLimits, ValueConstructionError, ValueGrammar,
        ValueLimitKind,
    };

    #[test]
    fn domains_have_exact_order_tokens_and_fingerprint_payloads() {
        let domains = [
            CatalogKeyDomain::JsonPointer,
            CatalogKeyDomain::YamlTypedPath,
            CatalogKeyDomain::Xliff12,
            CatalogKeyDomain::Xliff2,
        ];
        assert_eq!(
            domains.map(CatalogKeyDomain::as_str),
            ["json-pointer", "yaml-typed-path", "xliff-1.2", "xliff-2"]
        );
        assert!(domains.windows(2).all(|pair| pair[0] < pair[1]));
        assert_eq!(
            CatalogKeyDomain::YamlTypedPath
                .fingerprint_payload()
                .as_ref(),
            b"yaml-typed-path"
        );
    }

    #[test]
    fn json_pointer_retains_exact_canonical_tokens() {
        let root = CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "").unwrap();
        assert!(root.tokens().is_empty());

        let key = CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/a~1b~0c/0").unwrap();
        assert_eq!(key.as_str(), "/a~1b~0c/0");
        assert_eq!(key.tokens()[0].as_str(), "a/b~c");
        assert_eq!(key.tokens()[1].as_str(), "0");
        assert!(CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "a").is_err());
        assert!(CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/a~2b").is_err());
    }

    #[test]
    fn yaml_typed_path_distinguishes_key_and_index_kinds() {
        for valid in [
            "/k:str:1",
            "/k:null",
            "/k:bool:true",
            "/k:bool:false",
            "/k:int:1",
            "/k:int:-1",
            "/k:float:15e-1",
            "/k:float:.inf",
            "/k:float:-.inf",
            "/k:float:.nan",
            "/i:1",
        ] {
            assert!(
                CatalogKey::try_new(CatalogKeyDomain::YamlTypedPath, valid).is_ok(),
                "{valid}"
            );
        }
        for invalid in [
            "/k:bool:True",
            "/k:int:+1",
            "/k:int:01",
            "/k:int:-0",
            "/k:float:1.5",
            "/k:float:10e-1",
            "/k:float:1e+1",
            "/k:float:1e01",
            "/i:01",
            "/unknown:value",
        ] {
            assert!(
                CatalogKey::try_new(CatalogKeyDomain::YamlTypedPath, invalid).is_err(),
                "{invalid}"
            );
        }
        let escaped =
            CatalogKey::try_new(CatalogKeyDomain::YamlTypedPath, "/k:str:a~1b~0c").unwrap();
        assert_eq!(escaped.tokens()[0].as_str(), "k:str:a/b~c");
    }

    #[test]
    fn xliff_domains_enforce_their_complete_hierarchies() {
        assert!(CatalogKey::try_new(
            CatalogKeyDomain::Xliff12,
            "/file:original:app.json/group:id:menu/unit:id:welcome"
        )
        .is_ok());
        assert!(CatalogKey::try_new(
            CatalogKeyDomain::Xliff2,
            "/file:id:f1/group:i:0/unit:id:u1/segment:id:s1"
        )
        .is_ok());
        assert!(CatalogKey::try_new(
            CatalogKeyDomain::Xliff12,
            "/file:original:app.json/group:id:menu"
        )
        .is_err());
        assert!(CatalogKey::try_new(CatalogKeyDomain::Xliff2, "/file:id:f1/unit:id:u1").is_err());
        assert!(CatalogKey::try_new(CatalogKeyDomain::Xliff12, "/file:id:f1/unit:id:u1").is_err());
    }

    #[test]
    fn prefix_is_non_empty_and_stops_at_structural_boundaries() {
        let prefix = CatalogKeyPrefix::try_new(CatalogKeyDomain::JsonPointer, "/checkout").unwrap();
        assert_eq!(prefix.tokens()[0].as_str(), "checkout");
        assert!(matches!(
            CatalogKeyPrefix::try_new(CatalogKeyDomain::JsonPointer, ""),
            Err(ValueConstructionError::Grammar(ValueGrammar::Empty))
        ));
        assert!(CatalogKeyPrefix::try_new(
            CatalogKeyDomain::Xliff12,
            "/file:original:app.json/group:id:menu"
        )
        .is_ok());
        assert!(CatalogKeyPrefix::try_new(CatalogKeyDomain::Xliff12, "/group:id:menu").is_err());
    }

    #[test]
    fn pattern_parses_operators_and_literal_asterisks() {
        let pattern =
            CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, "/checkout/*/**/~2").unwrap();
        assert!(matches!(
            pattern.tokens(),
            [
                PatternToken::Literal(_),
                PatternToken::AnyOne,
                PatternToken::AnyMany,
                PatternToken::Literal(_)
            ]
        ));
        let PatternToken::Literal(literal) = &pattern.tokens()[3] else {
            panic!("expected literal");
        };
        assert_eq!(literal.as_str(), "*");

        assert!(CatalogKeyPattern::try_new(CatalogKeyDomain::YamlTypedPath, "/k:str:~2/*").is_ok());
        assert!(CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, "/a*/b").is_err());
        assert!(CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, "/**/**").is_err());
        assert!(CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, "").is_err());
    }

    #[test]
    fn key_and_pattern_token_boundaries_are_inclusive() {
        let exact = format!("/{}", vec!["a"; 256].join("/"));
        assert_eq!(
            CatalogKey::try_new(CatalogKeyDomain::JsonPointer, exact)
                .unwrap()
                .tokens()
                .len(),
            256
        );
        let over = format!("/{}", vec!["a"; 257].join("/"));
        assert!(matches!(
            CatalogKey::try_new(CatalogKeyDomain::JsonPointer, over),
            Err(ValueConstructionError::StructuralLimit(limit))
                if limit.kind() == ValueLimitKind::Tokens
                    && limit.limit() == 256
                    && limit.attempted() == 257
        ));

        let exact = (0..513)
            .map(|index| if index % 2 == 0 { "**" } else { "*" })
            .collect::<Vec<_>>()
            .join("/");
        let exact = format!("/{exact}");
        assert_eq!(
            CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, exact)
                .unwrap()
                .tokens()
                .len(),
            513
        );
        let over = (0..514)
            .map(|index| if index % 2 == 0 { "**" } else { "*" })
            .collect::<Vec<_>>()
            .join("/");
        assert!(matches!(
            CatalogKeyPattern::try_new(
                CatalogKeyDomain::JsonPointer,
                format!("/{over}")
            ),
            Err(ValueConstructionError::StructuralLimit(limit))
                if limit.kind() == ValueLimitKind::Tokens
                    && limit.limit() == 513
                    && limit.attempted() == 514
        ));
    }

    #[test]
    fn selector_order_is_variant_then_exact_payload_bytes() {
        let exact_a = MessageSelector::Exact(
            CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/a").unwrap(),
        );
        let exact_b = MessageSelector::Exact(
            CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/b").unwrap(),
        );
        let prefix = MessageSelector::Prefix(
            CatalogKeyPrefix::try_new(CatalogKeyDomain::JsonPointer, "/a").unwrap(),
        );
        let pattern = MessageSelector::Pattern(
            CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, "/a/*").unwrap(),
        );
        assert!(exact_a < exact_b);
        assert!(exact_b < prefix);
        assert!(prefix < pattern);
        assert!(pattern < MessageSelector::AllInScope);
        assert!(MessageSelector::AllInScope < MessageSelector::UnboundedDynamic);
    }

    #[test]
    fn selector_payload_domain_is_explicit_and_checked() {
        let selector = MessageSelector::Exact(
            CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/a").unwrap(),
        );
        assert!(selector.is_for_domain(CatalogKeyDomain::JsonPointer));
        assert!(!selector.is_for_domain(CatalogKeyDomain::YamlTypedPath));
        assert!(MessageSelector::AllInScope.is_for_domain(CatalogKeyDomain::Xliff2));
    }

    #[test]
    fn lower_key_and_pattern_limits_accept_exact_and_reject_first_over() {
        let key = CatalogKey::try_new(CatalogKeyDomain::JsonPointer, "/a/b").unwrap();
        let exact_bytes = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogKeyBytes, 4)
            .unwrap();
        assert!(key.revalidate_definition_bytes(&exact_bytes).is_ok());
        let first_over_bytes = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogKeyBytes, 3)
            .unwrap();
        assert_eq!(
            key.revalidate_definition_bytes(&first_over_bytes)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(4)
        );

        let zero_tokens = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogKeyTokens, 0)
            .unwrap();
        assert_eq!(
            key.revalidate_tokens(&zero_tokens)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(1)
        );

        let selector_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPathBytes, 4)
            .unwrap();
        assert!(key.revalidate_selector_bytes(&selector_exact).is_ok());
        let selector_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPathBytes, 3)
            .unwrap();
        assert_eq!(
            key.revalidate_selector_bytes(&selector_first_over)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(4)
        );

        let prefix = CatalogKeyPrefix::try_new(CatalogKeyDomain::JsonPointer, "/a/b").unwrap();
        let prefix_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPathBytes, 4)
            .unwrap();
        assert!(prefix.revalidate_bytes(&prefix_exact).is_ok());
        let prefix_tokens_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::CatalogKeyTokens, 2)
            .unwrap();
        assert!(prefix.revalidate_tokens(&prefix_tokens_exact).is_ok());

        let pattern = CatalogKeyPattern::try_new(CatalogKeyDomain::JsonPointer, "/a/*/**").unwrap();
        let pattern_bytes_exact = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPatternBytes, 7)
            .unwrap();
        assert!(pattern.revalidate_bytes(&pattern_bytes_exact).is_ok());
        let pattern_bytes_first_over = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPatternBytes, 6)
            .unwrap();
        assert_eq!(
            pattern
                .revalidate_bytes(&pattern_bytes_first_over)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(7)
        );
        let exact_pattern = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPatternTokens, 3)
            .unwrap();
        assert!(pattern.revalidate_tokens(&exact_pattern).is_ok());
        let first_over_pattern = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::SelectorPatternTokens, 2)
            .unwrap();
        assert_eq!(
            pattern
                .revalidate_tokens(&first_over_pattern)
                .unwrap_err()
                .observation(),
            LinkLimitObservation::Exact(3)
        );
    }
}
