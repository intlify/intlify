// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! URI references, as Draft 7's `$id` and `$ref` name schemas.
//!
//! A schema names itself and the schemas it uses by URI references (core 8).
//! Revision `"0"` reads one by RFC 3986's grammar, resolves it against its
//! base by section 5.2's strict algorithm, and compares the result after
//! section 6.2.2's syntax-based normalization: the escapes of unreserved
//! characters are decoded, the hexadecimal digits of the other escapes are
//! uppercase, the scheme and the host are lowercase, and the dot segments the
//! decoding spells are removed. Two references name one resource when their
//! normalized components are equal, each of the scheme, the authority, the
//! query, and the fragment telling an absent component from an empty one.
//!
//! No scheme is special: the scheme- and protocol-based normalizations of
//! section 6.2.3 and after, such as dropping a default port, do not apply,
//! and nothing is ever retrieved. A reference holds ASCII only, since the
//! grammar admits no other byte.

use std::fmt;

/// A URI reference split into RFC 3986's five components.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Reference {
    scheme: Option<String>,
    authority: Option<String>,
    path: String,
    query: Option<String>,
    fragment: Option<String>,
}

impl Reference {
    /// Read a URI reference: a URI, or a relative reference.
    ///
    /// Returns `None` when the text is outside RFC 3986's `URI-reference`
    /// grammar, such as one with a space, a non-ASCII character, a `%` not
    /// followed by two hexadecimal digits, or an IP literal that is not an
    /// address.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        // Appendix B's split, then each component's grammar.
        let (scheme, rest) = match text.find([':', '/', '?', '#']) {
            Some(end) if end > 0 && text.as_bytes()[end] == b':' => {
                (Some(&text[..end]), &text[end + 1..])
            }
            _ => (None, text),
        };
        let (authority, rest) = match rest.strip_prefix("//") {
            Some(after) => {
                let end = after.find(['/', '?', '#']).unwrap_or(after.len());
                (Some(&after[..end]), &after[end..])
            }
            None => (None, rest),
        };
        let (path, rest) = rest.split_at(rest.find(['?', '#']).unwrap_or(rest.len()));
        let (query, rest) = match rest.strip_prefix('?') {
            Some(after) => {
                let end = after.find('#').unwrap_or(after.len());
                (Some(&after[..end]), &after[end..])
            }
            None => (None, rest),
        };
        let fragment = rest.strip_prefix('#');
        if !scheme.is_none_or(is_scheme)
            || !authority.is_none_or(is_authority)
            || !spelled(path, is_path_byte)
            || !query.is_none_or(|query| spelled(query, is_query_byte))
            || !fragment.is_none_or(|fragment| spelled(fragment, is_query_byte))
        {
            return None;
        }
        // A relative reference's first segment holds no colon, which would
        // read as the end of a scheme.
        if scheme.is_none()
            && authority.is_none()
            && path
                .split('/')
                .next()
                .is_some_and(|segment| segment.contains(':'))
        {
            return None;
        }
        Some(Self {
            scheme: scheme.map(str::to_owned),
            authority: authority.map(str::to_owned),
            path: path.to_owned(),
            query: query.map(str::to_owned),
            fragment: fragment.map(str::to_owned),
        })
    }

    pub(crate) fn scheme(&self) -> Option<&str> {
        self.scheme.as_deref()
    }

    pub(crate) fn authority(&self) -> Option<&str> {
        self.authority.as_deref()
    }

    pub(crate) fn path(&self) -> &str {
        &self.path
    }

    pub(crate) fn query(&self) -> Option<&str> {
        self.query.as_deref()
    }

    pub(crate) fn fragment(&self) -> Option<&str> {
        self.fragment.as_deref()
    }

    /// The reference with no fragment, as a base or a resource is named.
    pub(crate) fn without_fragment(&self) -> Self {
        Self {
            fragment: None,
            ..self.clone()
        }
    }

    /// The target of this reference against `base`, by RFC 3986 section
    /// 5.2.2's strict algorithm: a scheme in the reference is never read as
    /// the base's.
    ///
    /// Returns `None` when `base` has no scheme, so is not a base. The base's
    /// fragment takes no part.
    pub(crate) fn resolve(&self, base: &Self) -> Option<Self> {
        let scheme = base.scheme.as_ref()?;
        if self.scheme.is_some() {
            return Some(Self {
                path: remove_dot_segments(&self.path),
                ..self.clone()
            });
        }
        let (authority, path, query) = if self.authority.is_some() {
            (
                self.authority.clone(),
                remove_dot_segments(&self.path),
                self.query.clone(),
            )
        } else if self.path.is_empty() {
            (
                base.authority.clone(),
                base.path.clone(),
                self.query.clone().or_else(|| base.query.clone()),
            )
        } else if self.path.starts_with('/') {
            (
                base.authority.clone(),
                remove_dot_segments(&self.path),
                self.query.clone(),
            )
        } else {
            (
                base.authority.clone(),
                remove_dot_segments(&merge(base, &self.path)),
                self.query.clone(),
            )
        };
        Some(Self {
            scheme: Some(scheme.clone()),
            authority,
            path,
            query,
            fragment: self.fragment.clone(),
        })
    }

    /// The reference under RFC 3986 section 6.2.2's syntax-based
    /// normalization. A reference with a scheme also loses the dot segments
    /// that decoding spells; a relative one keeps them for its resolution.
    pub(crate) fn normalize(&self) -> Self {
        let path = normalize_escapes(&self.path, false);
        Self {
            scheme: self
                .scheme
                .as_ref()
                .map(|scheme| scheme.to_ascii_lowercase()),
            authority: self.authority.as_deref().map(normalize_authority),
            path: if self.scheme.is_some() {
                remove_dot_segments(&path)
            } else {
                path
            },
            query: self
                .query
                .as_deref()
                .map(|query| normalize_escapes(query, false)),
            fragment: self
                .fragment
                .as_deref()
                .map(|fragment| normalize_escapes(fragment, false)),
        }
    }
}

/// RFC 3986 section 5.3's recomposition. A path that starts with `//` with no
/// authority, which only a resolution can make, reads back otherwise, so
/// references are compared by their components, not by this text.
impl fmt::Display for Reference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(scheme) = &self.scheme {
            write!(formatter, "{scheme}:")?;
        }
        if let Some(authority) = &self.authority {
            write!(formatter, "//{authority}")?;
        }
        formatter.write_str(&self.path)?;
        if let Some(query) = &self.query {
            write!(formatter, "?{query}")?;
        }
        if let Some(fragment) = &self.fragment {
            write!(formatter, "#{fragment}")?;
        }
        Ok(())
    }
}

/// RFC 3986 section 5.2.3: the reference's path after the base's last `/`,
/// or after `/` when the base has an authority and an empty path.
fn merge(base: &Reference, path: &str) -> String {
    if base.authority.is_some() && base.path.is_empty() {
        return format!("/{path}");
    }
    match base.path.rfind('/') {
        Some(slash) => format!("{}{path}", &base.path[..=slash]),
        None => path.to_owned(),
    }
}

/// RFC 3986 section 5.2.4: remove the `.` and `..` segments of a path, each
/// `..` with the segment before it.
pub(crate) fn remove_dot_segments(path: &str) -> String {
    let mut output = String::with_capacity(path.len());
    let mut index = 0;
    while index < path.len() {
        let input = &path[index..];
        if input.starts_with("../") {
            index += 3;
        } else if input.starts_with("./") || input.starts_with("/./") {
            // `/./` leaves its last `/` as the start of the input.
            index += 2;
        } else if input == "/." {
            output.push('/');
            index = path.len();
        } else if input.starts_with("/../") {
            index += 3;
            remove_last_segment(&mut output);
        } else if input == "/.." {
            remove_last_segment(&mut output);
            output.push('/');
            index = path.len();
        } else if input == "." || input == ".." {
            index = path.len();
        } else {
            let start = usize::from(input.starts_with('/'));
            let end = input[start..]
                .find('/')
                .map_or(input.len(), |end| start + end);
            output.push_str(&input[..end]);
            index += end;
        }
    }
    output
}

/// Remove the output's last segment and the `/` before it.
fn remove_last_segment(output: &mut String) {
    let end = output.rfind('/').unwrap_or(0);
    output.truncate(end);
}

/// Decode the escapes of unreserved characters and uppercase the hexadecimal
/// digits of the other escapes; with `lowercase`, as the host asks, also
/// lowercase every letter outside an escape.
fn normalize_escapes(text: &str, lowercase: bool) -> String {
    let fold = |byte: u8| {
        char::from(if lowercase {
            byte.to_ascii_lowercase()
        } else {
            byte
        })
    };
    let bytes = text.as_bytes();
    let mut normalized = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        let escaped = match (bytes[index], bytes.get(index + 1), bytes.get(index + 2)) {
            (b'%', Some(&high), Some(&low)) => hex(high).zip(hex(low)),
            _ => None,
        };
        if let Some((high, low)) = escaped {
            let byte = high << 4 | low;
            if is_unreserved(byte) {
                normalized.push(fold(byte));
            } else {
                normalized.push('%');
                normalized.push(char::from(bytes[index + 1].to_ascii_uppercase()));
                normalized.push(char::from(bytes[index + 2].to_ascii_uppercase()));
            }
            index += 3;
        } else {
            normalized.push(fold(bytes[index]));
            index += 1;
        }
    }
    normalized
}

/// Normalize an authority: the user information keeps its case, and the host
/// and the port, which is digits, are lowercased.
fn normalize_authority(authority: &str) -> String {
    match authority.find('@') {
        Some(at) => format!(
            "{}@{}",
            normalize_escapes(&authority[..at], false),
            normalize_escapes(&authority[at + 1..], true)
        ),
        None => normalize_escapes(authority, true),
    }
}

fn hex(byte: u8) -> Option<u8> {
    char::from(byte).to_digit(16).map(|digit| digit as u8)
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

fn is_sub_delim(byte: u8) -> bool {
    matches!(
        byte,
        b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
    )
}

/// `pchar` or `/`.
fn is_path_byte(byte: u8) -> bool {
    is_unreserved(byte) || is_sub_delim(byte) || matches!(byte, b':' | b'@' | b'/')
}

/// `pchar`, `/`, or `?`: the bytes of a query or a fragment.
fn is_query_byte(byte: u8) -> bool {
    is_path_byte(byte) || byte == b'?'
}

fn is_userinfo_byte(byte: u8) -> bool {
    is_unreserved(byte) || is_sub_delim(byte) || byte == b':'
}

fn is_reg_name_byte(byte: u8) -> bool {
    is_unreserved(byte) || is_sub_delim(byte)
}

/// Whether every byte of `text` is allowed or begins an escape: `%` and two
/// hexadecimal digits.
fn spelled(text: &str, allowed: fn(u8) -> bool) -> bool {
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let digits = bytes.get(index + 1..index + 3);
            if !digits.is_some_and(|digits| digits.iter().all(u8::is_ascii_hexdigit)) {
                return false;
            }
            index += 3;
        } else if allowed(bytes[index]) {
            index += 1;
        } else {
            return false;
        }
    }
    true
}

/// `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`.
fn is_scheme(scheme: &str) -> bool {
    let mut bytes = scheme.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic())
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'-' | b'.'))
}

/// `[ userinfo "@" ] host [ ":" port ]`. An IPv4 address is a registered
/// name as well, so a host is an IP literal or a registered name.
fn is_authority(authority: &str) -> bool {
    let (userinfo, host_port) = match authority.find('@') {
        Some(at) => (Some(&authority[..at]), &authority[at + 1..]),
        None => (None, authority),
    };
    if !userinfo.is_none_or(|userinfo| spelled(userinfo, is_userinfo_byte)) {
        return false;
    }
    let (host, port) = match host_port.strip_prefix('[') {
        Some(literal) => {
            let Some(close) = literal.find(']') else {
                return false;
            };
            let port = match &literal[close + 1..] {
                "" => None,
                after => match after.strip_prefix(':') {
                    Some(port) => Some(port),
                    None => return false,
                },
            };
            (is_ip_literal(&literal[..close]), port)
        }
        None => match host_port.find(':') {
            Some(colon) => (
                spelled(&host_port[..colon], is_reg_name_byte),
                Some(&host_port[colon + 1..]),
            ),
            None => (spelled(host_port, is_reg_name_byte), None),
        },
    };
    host && port.is_none_or(|port| port.bytes().all(|byte| byte.is_ascii_digit()))
}

/// `IPv6address / IPvFuture`, between the brackets.
fn is_ip_literal(literal: &str) -> bool {
    match literal.strip_prefix(['v', 'V']) {
        // `1*HEXDIG "." 1*( unreserved / sub-delims / ":" )`.
        Some(future) => future.split_once('.').is_some_and(|(version, address)| {
            !version.is_empty()
                && version.bytes().all(|byte| byte.is_ascii_hexdigit())
                && !address.is_empty()
                && address.bytes().all(is_userinfo_byte)
        }),
        None => is_ipv6(literal),
    }
}

/// An IPv6 address: eight 16-bit pieces, or fewer around one `::`, the last
/// two of which may be written as an IPv4 address. A second `::` leaves an
/// empty group, which no piece is.
fn is_ipv6(address: &str) -> bool {
    match address.split_once("::") {
        Some((head, tail)) => matches!(
            (pieces(head, false), pieces(tail, true)),
            (Some(head), Some(tail)) if head + tail <= 7
        ),
        None => pieces(address, true) == Some(8),
    }
}

/// How many 16-bit pieces `part` writes: colon-separated groups of one to
/// four hexadecimal digits, the last of which may be an IPv4 address worth
/// two when `ipv4_last`.
fn pieces(part: &str, ipv4_last: bool) -> Option<usize> {
    if part.is_empty() {
        return Some(0);
    }
    let groups: Vec<&str> = part.split(':').collect();
    let mut count = 0;
    for (index, group) in groups.iter().enumerate() {
        if ipv4_last && index + 1 == groups.len() && group.contains('.') {
            if !is_ipv4(group) {
                return None;
            }
            count += 2;
        } else if (1..=4).contains(&group.len())
            && group.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            count += 1;
        } else {
            return None;
        }
    }
    Some(count)
}

/// Four decimal octets from 0 to 255, none with a leading zero or a sign.
fn is_ipv4(address: &str) -> bool {
    let octets: Vec<&str> = address.split('.').collect();
    octets.len() == 4
        && octets.iter().all(|octet| {
            octet.bytes().all(|byte| byte.is_ascii_digit())
                && (octet.len() == 1 || !octet.starts_with('0'))
                && octet.parse::<u8>().is_ok()
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reference(text: &str) -> Reference {
        Reference::parse(text).unwrap_or_else(|| panic!("{text:?} is a URI reference"))
    }

    fn resolved(base: &str, text: &str) -> String {
        reference(text)
            .resolve(&reference(base))
            .unwrap_or_else(|| panic!("{base:?} is a base"))
            .to_string()
    }

    fn normalized(text: &str) -> String {
        reference(text).normalize().to_string()
    }

    /// RFC 3986 section 5.4's examples against `http://a/b/c/d;p?q`, the
    /// normal ones and then the abnormal ones, `http:g` for a strict parser.
    const EXAMPLES: &[(&str, &str)] = &[
        ("g:h", "g:h"),
        ("g", "http://a/b/c/g"),
        ("./g", "http://a/b/c/g"),
        ("g/", "http://a/b/c/g/"),
        ("/g", "http://a/g"),
        ("//g", "http://g"),
        ("?y", "http://a/b/c/d;p?y"),
        ("g?y", "http://a/b/c/g?y"),
        ("#s", "http://a/b/c/d;p?q#s"),
        ("g#s", "http://a/b/c/g#s"),
        ("g?y#s", "http://a/b/c/g?y#s"),
        (";x", "http://a/b/c/;x"),
        ("g;x", "http://a/b/c/g;x"),
        ("g;x?y#s", "http://a/b/c/g;x?y#s"),
        ("", "http://a/b/c/d;p?q"),
        (".", "http://a/b/c/"),
        ("./", "http://a/b/c/"),
        ("..", "http://a/b/"),
        ("../", "http://a/b/"),
        ("../g", "http://a/b/g"),
        ("../..", "http://a/"),
        ("../../", "http://a/"),
        ("../../g", "http://a/g"),
        ("../../../g", "http://a/g"),
        ("../../../../g", "http://a/g"),
        ("/./g", "http://a/g"),
        ("/../g", "http://a/g"),
        ("g.", "http://a/b/c/g."),
        (".g", "http://a/b/c/.g"),
        ("g..", "http://a/b/c/g.."),
        ("..g", "http://a/b/c/..g"),
        ("./../g", "http://a/b/g"),
        ("./g/.", "http://a/b/c/g/"),
        ("g/./h", "http://a/b/c/g/h"),
        ("g/../h", "http://a/b/c/h"),
        ("g;x=1/./y", "http://a/b/c/g;x=1/y"),
        ("g;x=1/../y", "http://a/b/c/y"),
        ("g?y/./x", "http://a/b/c/g?y/./x"),
        ("g?y/../x", "http://a/b/c/g?y/../x"),
        ("g#s/./x", "http://a/b/c/g#s/./x"),
        ("g#s/../x", "http://a/b/c/g#s/../x"),
        ("http:g", "http:g"),
    ];

    #[test]
    fn rfc_3986s_examples_resolve_as_it_lists() {
        assert_eq!(EXAMPLES.len(), 42);
        for (text, target) in EXAMPLES {
            assert_eq!(resolved("http://a/b/c/d;p?q", text), *target, "{text:?}");
        }
    }

    #[test]
    fn a_reference_splits_into_five_components() {
        type Parts<'a> = (
            Option<&'a str>,
            Option<&'a str>,
            &'a str,
            Option<&'a str>,
            Option<&'a str>,
        );
        let cases: &[(&str, Parts<'_>)] = &[
            (
                "http://a/b/c/d;p?q",
                (Some("http"), Some("a"), "/b/c/d;p", Some("q"), None),
            ),
            ("", (None, None, "", None, None)),
            ("#", (None, None, "", None, Some(""))),
            ("?", (None, None, "", Some(""), None)),
            ("?#", (None, None, "", Some(""), Some(""))),
            ("#?", (None, None, "", None, Some("?"))),
            ("a?b?c", (None, None, "a", Some("b?c"), None)),
            ("http://a", (Some("http"), Some("a"), "", None, None)),
            ("http://", (Some("http"), Some(""), "", None, None)),
            ("//g", (None, Some("g"), "", None, None)),
            ("///g", (None, Some(""), "/g", None, None)),
            (
                "urn:uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f",
                (
                    Some("urn"),
                    None,
                    "uuid:ee564b8a-7a87-4125-8c96-e9f123d6766f",
                    None,
                    None,
                ),
            ),
            ("a:b/c:d", (Some("a"), None, "b/c:d", None, None)),
            ("./a:b", (None, None, "./a:b", None, None)),
            ("/a:b", (None, None, "/a:b", None, None)),
            ("a/:b?:", (None, None, "a/:b", Some(":"), None)),
            ("g?y/./x", (None, None, "g", Some("y/./x"), None)),
            ("g#s/../x", (None, None, "g", None, Some("s/../x"))),
            (
                "#/definitions/a%20b",
                (None, None, "", None, Some("/definitions/a%20b")),
            ),
            (
                "foo://u:p@[::1]:80/a?b#c",
                (
                    Some("foo"),
                    Some("u:p@[::1]:80"),
                    "/a",
                    Some("b"),
                    Some("c"),
                ),
            ),
            ("mailto:a@b", (Some("mailto"), None, "a@b", None, None)),
            (
                "http://a!$&'()*+,;=b:8/",
                (Some("http"), Some("a!$&'()*+,;=b:8"), "/", None, None),
            ),
            ("A+1-.b:", (Some("A+1-.b"), None, "", None, None)),
            (
                "http://a:/?!$&'()*+,;=:@/?#!$&'()*+,;=:@/?",
                (
                    Some("http"),
                    Some("a:"),
                    "/",
                    Some("!$&'()*+,;=:@/?"),
                    Some("!$&'()*+,;=:@/?"),
                ),
            ),
            ("%41%7e", (None, None, "%41%7e", None, None)),
        ];
        for (text, (scheme, authority, path, query, fragment)) in cases {
            let parsed = reference(text);
            assert_eq!(parsed.scheme(), *scheme, "{text:?}");
            assert_eq!(parsed.authority(), *authority, "{text:?}");
            assert_eq!(parsed.path(), *path, "{text:?}");
            assert_eq!(parsed.query(), *query, "{text:?}");
            assert_eq!(parsed.fragment(), *fragment, "{text:?}");
            assert_eq!(parsed.to_string(), *text, "{text:?}");
        }
    }

    #[test]
    fn a_text_outside_rfc_3986s_grammar_is_not_a_reference() {
        for text in [
            " ",
            "a b",
            "#a b",
            "?a b",
            "\"",
            "<a>",
            "a{b}",
            "a|b",
            "a\\b",
            "a^b",
            "a`b",
            "[",
            "a]",
            "%",
            "%4",
            "%4g",
            "a%zz",
            "#%",
            "#%2",
            "\u{e9}",
            "#\u{e9}",
            "1a:b",
            "+a:b",
            "a_b:c",
            ":a",
            ":",
            "a#b#c",
            "http://a b/",
            "http://a^b",
            "http://a^b:80/",
            "http://u%zz@a",
            "http://a@b@c",
            "http://a:b:c",
            "http://a:8a",
            "http://[::1",
            "http://[::1]x",
            "http://[::1]:x",
            "http://[::1]::80",
            "http://[]",
            "http://a/b c",
            "http://a/%",
        ] {
            assert_eq!(Reference::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn an_ip_literal_is_an_ipv6_address_or_a_future_form() {
        let host = |literal: &str| Reference::parse(&format!("http://{literal}/")).is_some();
        for literal in [
            "[::]",
            "[::1]",
            "[1::]",
            "[1:2:3:4:5:6:7:8]",
            "[1:2:3:4:5:6:7::]",
            "[::2:3:4:5:6:7:8]",
            "[1::8]",
            "[::1.2.3.4]",
            "[1:2:3:4:5:6:1.2.3.4]",
            "[1:2:3:4:5::1.2.3.4]",
            "[::ffff:255.255.255.255]",
            "[FFFF::abcd]",
            "[0:0:0:0:0:0:0:0]",
            "[v1.a]",
            "[V1F.a:b!]",
            "[vF.~]",
            "[::1]:8080",
            "u@[::1]",
        ] {
            assert!(host(literal), "{literal:?}");
        }
        for literal in [
            "[1:2:3:4:5:6:7:8:9]",
            "[1:2:3:4:5:6:7]",
            "[1::2::3]",
            "[:::]",
            "[:1]",
            "[1:]",
            "[1:::2]",
            "[1.2.3.4::]",
            "[1.2.3.4]",
            "[1:2:3:4:5:6:7:1.2.3.4]",
            "[1:2:3:4:5:6::1.2.3.4]",
            "[1:2:3:4:5:6:7:8::]",
            "[::1.2.3.4:1]",
            "[::ffff:256.1.1.1]",
            "[::ffff:01.1.1.1]",
            "[::1.2.3]",
            "[::1.2.3.4.5]",
            "[::1.1.1.+1]",
            "[::1.1.1.]",
            "[::1..3.4]",
            "[12345::]",
            "[g::]",
            "[fe80::1%eth0]",
            "[fe80::1%25eth0]",
            "[v1]",
            "[v.a]",
            "[vg.a]",
            "[v1.]",
            "[v1.a/b]",
            "[v1.a%20]",
        ] {
            assert!(!host(literal), "{literal:?}");
        }
    }

    #[test]
    fn merging_follows_the_base_path_or_its_authority() {
        for (base, text, target) in [
            ("http://a", "g", "http://a/g"),
            ("http://a", "", "http://a"),
            ("http://a", "..", "http://a/"),
            ("http://a/b?q", "", "http://a/b?q"),
            ("http://a/b?q", "#f", "http://a/b?q#f"),
            ("http://a/b?q", "?", "http://a/b?"),
            ("http://a/b#base", "", "http://a/b"),
            ("http://a/b#base", "#", "http://a/b#"),
            ("http://a/b", "//c", "http://c"),
            ("http://a/b", "//c/./d/../e", "http://c/e"),
            ("foo:bar", "g", "foo:g"),
            ("foo:bar/baz", "g", "foo:bar/g"),
            ("foo:/a/b", "c", "foo:/a/c"),
            ("foo:", "g", "foo:g"),
            ("urn:uuid:x", "#f", "urn:uuid:x#f"),
            ("urn:uuid:x", "", "urn:uuid:x"),
            ("json-schema:///", "other.json", "json-schema:///other.json"),
            (
                "json-schema:///",
                "#/definitions/a",
                "json-schema:///#/definitions/a",
            ),
            (
                "http://example.com/root.json",
                "other.json",
                "http://example.com/other.json",
            ),
            (
                "http://example.com/other.json",
                "t/inner.json",
                "http://example.com/t/inner.json",
            ),
            ("http://example.com/root.json", "urn:x", "urn:x"),
            ("http://a/b/c", "urn:a/../b", "urn:/b"),
        ] {
            assert_eq!(resolved(base, text), target, "{text:?} against {base:?}");
        }
        assert_eq!(reference("g").resolve(&reference("/a/b")), None);
        assert_eq!(reference("g").resolve(&reference("//a/b")), None);
    }

    #[test]
    fn dot_segments_are_removed_with_the_segment_before_each_double_dot() {
        for (path, removed) in [
            ("/a/b/c/./../../g", "/a/g"),
            ("mid/content=5/../6", "mid/6"),
            ("", ""),
            (".", ""),
            ("..", ""),
            ("./", ""),
            ("../", ""),
            ("/", "/"),
            ("/.", "/"),
            ("/..", "/"),
            ("/./", "/"),
            ("/../", "/"),
            ("/a/..", "/"),
            ("/a/../", "/"),
            ("a/..", "/"),
            ("../a", "a"),
            ("./a", "a"),
            ("a/./b", "a/b"),
            ("a//b", "a//b"),
            ("/a/b/", "/a/b/"),
            ("/a/./", "/a/"),
            ("/a/../../b", "/b"),
            ("/a/b/..", "/a/"),
            (".../a", ".../a"),
            ("/a/.b/..c/", "/a/.b/..c/"),
            ("a", "a"),
        ] {
            assert_eq!(remove_dot_segments(path), removed, "{path:?}");
        }
    }

    #[test]
    fn normalizing_decodes_unreserved_escapes_and_lowercases_the_scheme_and_the_host() {
        for (text, target) in [
            (
                "HTTP://Example.COM/%7Euser/%7e%2f?%41#%61%3a",
                "http://example.com/~user/~%2F?A#a%3A",
            ),
            ("http://%41%42.com", "http://ab.com"),
            ("http://U%40%3a@A", "http://U%40%3A@a"),
            ("http://User@Host:80/Path", "http://User@host:80/Path"),
            ("http://[FE80::1]:80/", "http://[fe80::1]:80/"),
            ("foo:/a/%2E%2E/b", "foo:/b"),
            ("foo:/a/.%2e/b", "foo:/b"),
            ("foo:/a/%2e/b", "foo:/a/b"),
            ("foo:a/%2E%2E/b", "foo:/b"),
            ("%2E%2E/a", "../a"),
            ("./%2e/a", "././a"),
            ("http://a/b?%7E%20", "http://a/b?~%20"),
            ("urn:Uuid:ABC", "urn:Uuid:ABC"),
            ("#%2f%41", "#%2FA"),
            ("http://a/%e9", "http://a/%E9"),
            ("", ""),
        ] {
            assert_eq!(normalized(text), target, "{text:?}");
            assert_eq!(
                reference(target).normalize().to_string(),
                target,
                "{target:?} is normalized"
            );
        }
    }

    #[test]
    fn components_tell_an_absent_part_from_an_empty_one() {
        for (left, right) in [
            ("http://a?", "http://a"),
            ("http://a#", "http://a"),
            ("http://a", "http://a/"),
            ("http:", "http://"),
            ("http://a?#", "http://a?"),
        ] {
            assert_ne!(reference(left), reference(right), "{left:?} and {right:?}");
        }
        let base = reference("http://a/b?q#f");
        assert_eq!(base.without_fragment(), reference("http://a/b?q"));
        assert_eq!(base.fragment(), Some("f"));
        assert_eq!(reference("http://a/b?q#").fragment(), Some(""));
        assert_eq!(reference("http://a/b?q").fragment(), None);
    }

    #[test]
    fn a_path_of_two_slashes_with_no_authority_stays_a_path() {
        let target = reference(".//a").resolve(&reference("foo:/")).unwrap();
        assert_eq!(target.authority(), None);
        assert_eq!(target.path(), "//a");
        // The recomposed text reads back with an authority, so references are
        // compared by their components.
        assert_eq!(target.to_string(), "foo://a");
        assert_ne!(target, reference("foo://a"));
    }
}
