// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The grammar of a Unicode BCP 47 Locale Identifier, as UTS #35 for CLDR
//! 48.2.0 gives it with its well-formedness constraints, read before ICU4X
//! reads anything. It finds what design 015's conformance layer checks first,
//! which ICU4X 2.2.0 refuses, drops, or passes through: a second extension
//! with one singleton, a repeated key, a `u` attribute, and private use. It
//! accepts a primary language subtag of five to eight letters, which ICU4X
//! cannot parse, so that the validity data rather than ICU4X rejects one.
//! Every subtag keeps its spelling and case.

use std::ops::Range;

/// The input is not a hyphen-separated Unicode BCP 47 Locale Identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Malformed;

/// One identifier: its subtags, and where its parts are among them.
#[derive(Debug)]
pub(crate) struct Identifier<'a> {
    pub(crate) subtags: Vec<&'a str>,
    pub(crate) language: LanguageId<'a>,
    /// Every `u` extension, in input order. More than one is not
    /// well-formed, and the conformance layer rejects it as such.
    pub(crate) unicode: Vec<Unicode<'a>>,
    /// Every `t` extension, in input order, likewise.
    pub(crate) transformed: Vec<Transformed<'a>>,
    /// How many extensions have a singleton other than `u`, `t`, and `x`.
    pub(crate) others: usize,
    pub(crate) private_use: bool,
}

/// A `unicode_language_id`, or a `t` extension's `tlang`.
#[derive(Debug)]
pub(crate) struct LanguageId<'a> {
    pub(crate) language: &'a str,
    pub(crate) script: Option<&'a str>,
    pub(crate) region: Option<&'a str>,
    pub(crate) variants: Vec<&'a str>,
}

/// A `u` extension.
#[derive(Debug)]
pub(crate) struct Unicode<'a> {
    pub(crate) attributes: Vec<&'a str>,
    pub(crate) keywords: Vec<Field<'a>>,
}

/// A `t` extension.
#[derive(Debug)]
pub(crate) struct Transformed<'a> {
    pub(crate) language: Option<LanguageId<'a>>,
    pub(crate) fields: Vec<Field<'a>>,
}

/// A `u` keyword or a `t` field: its key, and its value as a range of the
/// identifier's subtags, which is empty for a keyword without a type.
#[derive(Debug)]
pub(crate) struct Field<'a> {
    pub(crate) key: &'a str,
    pub(crate) value: Range<usize>,
}

impl<'a> Identifier<'a> {
    pub(crate) fn parse(input: &'a str) -> Result<Self, Malformed> {
        let subtags: Vec<&str> = input.split('-').collect();
        // UTS #35 restricts the separator to `-` for BCP 47, and no subtag
        // has more than eight characters.
        if !subtags.iter().all(|subtag| {
            (1..=8).contains(&subtag.len())
                && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
        }) {
            return Err(Malformed);
        }
        let mut reader = Reader {
            subtags: &subtags,
            next: 0,
        };
        // The first subtag must be a language subtag, so `root` and a
        // script-leading identifier are not well-formed.
        let language = reader.language_id()?;
        let mut identifier = Self {
            subtags: Vec::new(),
            language,
            unicode: Vec::new(),
            transformed: Vec::new(),
            others: 0,
            private_use: false,
        };
        while let Some(singleton) = reader.take(is_singleton) {
            match singleton.as_bytes()[0].to_ascii_lowercase() {
                b'u' => identifier.unicode.push(reader.unicode()?),
                b't' => identifier.transformed.push(reader.transformed()?),
                b'x' => {
                    // A private-use sequence runs to the end: every
                    // remaining subtag is one to eight alphanumerics.
                    if reader.next == subtags.len() {
                        return Err(Malformed);
                    }
                    reader.next = subtags.len();
                    identifier.private_use = true;
                }
                _ => {
                    reader.other()?;
                    identifier.others += 1;
                }
            }
        }
        if reader.next != subtags.len() {
            return Err(Malformed);
        }
        identifier.subtags = subtags;
        Ok(identifier)
    }

    /// The value of `field` in lowercase, its subtags joined by `-`.
    pub(crate) fn value(&self, field: &Field<'_>) -> String {
        self.subtags[field.value.clone()]
            .join("-")
            .to_ascii_lowercase()
    }
}

struct Reader<'s, 'a> {
    subtags: &'s [&'a str],
    next: usize,
}

impl<'a> Reader<'_, 'a> {
    /// The next subtag, if `form` accepts it.
    fn take(&mut self, form: fn(&str) -> bool) -> Option<&'a str> {
        let subtag = self
            .subtags
            .get(self.next)
            .copied()
            .filter(|subtag| form(subtag))?;
        self.next += 1;
        Some(subtag)
    }

    fn language_id(&mut self) -> Result<LanguageId<'a>, Malformed> {
        let language = self.take(is_language).ok_or(Malformed)?;
        let script = self.take(is_script);
        let region = self.take(is_region);
        let mut variants: Vec<&str> = Vec::new();
        while let Some(variant) = self.take(is_variant) {
            // No variant may repeat, in either case.
            if variants
                .iter()
                .any(|seen| seen.eq_ignore_ascii_case(variant))
            {
                return Err(Malformed);
            }
            variants.push(variant);
        }
        Ok(LanguageId {
            language,
            script,
            region,
            variants,
        })
    }

    /// `(sep keyword)+ | (sep uattribute)+ (sep ufield)*`
    fn unicode(&mut self) -> Result<Unicode<'a>, Malformed> {
        let mut attributes = Vec::new();
        while let Some(attribute) = self.take(is_value) {
            attributes.push(attribute);
        }
        let keywords = self.fields(is_unicode_key);
        if attributes.is_empty() && keywords.is_empty() {
            return Err(Malformed);
        }
        Ok(Unicode {
            attributes,
            keywords,
        })
    }

    /// `(sep tlang (sep tfield)*) | (sep tfield)+`, where a field's value
    /// has at least one subtag.
    fn transformed(&mut self) -> Result<Transformed<'a>, Malformed> {
        let language = match self.subtags.get(self.next) {
            Some(subtag) if is_language(subtag) => Some(self.language_id()?),
            _ => None,
        };
        let fields = self.fields(is_transformed_key);
        if fields.iter().any(|field| field.value.is_empty())
            || (language.is_none() && fields.is_empty())
        {
            return Err(Malformed);
        }
        Ok(Transformed { language, fields })
    }

    /// `(sep alphanum{2,8})+`
    fn other(&mut self) -> Result<(), Malformed> {
        let start = self.next;
        while self.take(|subtag| subtag.len() >= 2).is_some() {}
        if self.next == start {
            return Err(Malformed);
        }
        Ok(())
    }

    fn fields(&mut self, key: fn(&str) -> bool) -> Vec<Field<'a>> {
        let mut fields = Vec::new();
        while let Some(name) = self.take(key) {
            let start = self.next;
            while self.take(is_value).is_some() {}
            fields.push(Field {
                key: name,
                value: start..self.next,
            });
        }
        fields
    }
}

// Every subtag is already one to eight ASCII alphanumerics.

/// `alpha{2,3} | alpha{5,8}`
fn is_language(subtag: &str) -> bool {
    matches!(subtag.len(), 2 | 3 | 5..=8) && subtag.bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// `alpha{4}`
fn is_script(subtag: &str) -> bool {
    subtag.len() == 4 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic())
}

/// `alpha{2} | digit{3}`
fn is_region(subtag: &str) -> bool {
    (subtag.len() == 2 && subtag.bytes().all(|byte| byte.is_ascii_alphabetic()))
        || (subtag.len() == 3 && subtag.bytes().all(|byte| byte.is_ascii_digit()))
}

/// `alphanum{5,8} | digit alphanum{3}`
fn is_variant(subtag: &str) -> bool {
    subtag.len() >= 5 || (subtag.len() == 4 && subtag.as_bytes()[0].is_ascii_digit())
}

fn is_singleton(subtag: &str) -> bool {
    subtag.len() == 1
}

/// A `u` key, `alphanum alpha`.
fn is_unicode_key(subtag: &str) -> bool {
    subtag.len() == 2 && subtag.as_bytes()[1].is_ascii_alphabetic()
}

/// A `t` key, `alpha digit`.
fn is_transformed_key(subtag: &str) -> bool {
    subtag.len() == 2
        && subtag.as_bytes()[0].is_ascii_alphabetic()
        && subtag.as_bytes()[1].is_ascii_digit()
}

/// A type subtag or attribute, `alphanum{3,8}`.
fn is_value(subtag: &str) -> bool {
    subtag.len() >= 3
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(input: &str) -> Identifier<'_> {
        Identifier::parse(input).unwrap_or_else(|_| panic!("{input} is well-formed"))
    }

    #[test]
    fn a_language_id_reads_its_four_parts_in_order() {
        let identifier = parse("sl-Latn-IT-rozaj-1994");
        let language = &identifier.language;
        assert_eq!(
            (language.language, language.script, language.region),
            ("sl", Some("Latn"), Some("IT"))
        );
        assert_eq!(language.variants, ["rozaj", "1994"]);
        let identifier = parse("en-419");
        assert_eq!(
            (identifier.language.script, identifier.language.region),
            (None, Some("419"))
        );
        let identifier = parse("abcdefgh");
        assert_eq!(identifier.language.language, "abcdefgh");
        assert!(identifier.unicode.is_empty() && identifier.transformed.is_empty());
        assert_eq!(identifier.subtags, ["abcdefgh"]);
    }

    #[test]
    fn extensions_are_read_with_their_fields_and_where_each_value_is() {
        let identifier =
            parse("en-a-bb-ccc-T-ja-Kana-JP-m0-ungegn-x0-abc-def-u-attr-ca-islamic-civil-kn-x-0");
        assert_eq!(identifier.others, 1);
        assert!(identifier.private_use);
        let unicode = &identifier.unicode[0];
        assert_eq!(unicode.attributes, ["attr"]);
        let keywords: Vec<_> = unicode
            .keywords
            .iter()
            .map(|field| (field.key, identifier.value(field)))
            .collect();
        assert_eq!(
            keywords,
            [("ca", "islamic-civil".to_owned()), ("kn", String::new())]
        );
        let transformed = &identifier.transformed[0];
        let language = transformed.language.as_ref().unwrap();
        assert_eq!(
            (language.language, language.script, language.region),
            ("ja", Some("Kana"), Some("JP"))
        );
        let fields: Vec<_> = transformed
            .fields
            .iter()
            .map(|field| (field.key, field.value.clone()))
            .collect();
        assert_eq!(fields, [("m0", 9..10), ("x0", 11..13)]);
        assert_eq!(identifier.subtags[11..13], ["abc", "def"]);
        assert_eq!(identifier.value(&transformed.fields[0]), "ungegn");
        let lowercase = parse("en-u-CA-GREGORY");
        assert_eq!(
            lowercase.value(&lowercase.unicode[0].keywords[0]),
            "gregory"
        );
    }

    #[test]
    fn a_second_extension_with_one_singleton_is_read_for_the_layer_to_reject() {
        let identifier = parse("en-u-ca-gregory-U-nu-latn-t-ja-t-it");
        assert_eq!(
            (identifier.unicode.len(), identifier.transformed.len()),
            (2, 2)
        );
        let identifier = parse("en-t-m0-ungegn");
        assert!(identifier.transformed[0].language.is_none());
        let identifier = parse("en-a-bb-a-cc");
        assert_eq!(identifier.others, 2);
    }

    #[test]
    fn what_uts_35_does_not_call_well_formed_is_malformed() {
        for input in [
            "",
            "en_US",
            "en-US.UTF-8",
            "en_US@calendar=gregorian",
            "root",
            "Latn",
            "Latn-US",
            "abcd",
            "abcdefghi",
            "e",
            "123",
            "-en",
            "en-",
            "en--US",
            "en-US-oed",
            "zh-min-nan",
            "i-klingon",
            "de-1996-fonipa-1996",
            "de-1996-fonipa-1996-t-de",
            "en-t-de-1996-fonipa-1996",
            "de-fonipa-FONIPA",
            "en-u",
            "en-t",
            "en-x",
            "en-a",
            "en-a-b",
            "en-u-a0-abc",
            "en-u-ca-gregory-a0",
            "en-t-m0",
            "en-t-m0-ab",
            "en-t-hans",
            "en-ü",
            "en-US-u-ca-gregory-",
            "en-u-abcdefghi",
            "en-x-abcdefghi",
            "en-x-abc-",
            "en-x--abc",
            "en-x-a.b",
        ] {
            assert_eq!(
                Identifier::parse(input).map(|_| ()),
                Err(Malformed),
                "{input}"
            );
        }
    }

    #[test]
    fn each_subtag_form_has_its_exact_lengths() {
        assert!(
            is_language("en")
                && is_language("und")
                && is_language("abcde")
                && is_language("abcdefgh")
        );
        assert!(
            !is_language("e")
                && !is_language("abcd")
                && !is_language("abcdefghi")
                && !is_language("e1")
        );
        assert!(
            is_script("Latn") && !is_script("Lat") && !is_script("Latin") && !is_script("La1n")
        );
        assert!(
            is_region("US")
                && is_region("419")
                && !is_region("U")
                && !is_region("USA")
                && !is_region("U1")
                && !is_region("41")
        );
        assert!(!is_region("4193") && !is_region("41a"));
        assert!(
            is_variant("1994")
                && is_variant("rozaj")
                && is_variant("fonipa")
                && is_variant("abcdefgh")
                && !is_variant("abcd")
                && !is_variant("199")
        );
        assert!(is_singleton("u") && !is_singleton("uu"));
        assert!(
            is_unicode_key("ca")
                && is_unicode_key("0a")
                && !is_unicode_key("a0")
                && !is_unicode_key("abc")
                && !is_unicode_key("c")
        );
        assert!(
            is_transformed_key("m0")
                && !is_transformed_key("0m")
                && !is_transformed_key("00")
                && !is_transformed_key("mm")
                && !is_transformed_key("m00")
        );
        assert!(is_value("abc") && !is_value("ab"));
    }

    #[test]
    fn two_letter_subtags_are_read_as_whatever_their_place_admits() {
        // `gr` is a second key without a type, and `ab` a region.
        let identifier = parse("en-u-ca-gr");
        let keys: Vec<_> = identifier.unicode[0]
            .keywords
            .iter()
            .map(|field| field.key)
            .collect();
        assert_eq!(keys, ["ca", "gr"]);
        let identifier = parse("en-t-ja-ab");
        assert_eq!(
            identifier.transformed[0].language.as_ref().unwrap().region,
            Some("ab")
        );
    }

    #[test]
    fn a_keyword_takes_every_type_subtag_up_to_the_next_key() {
        let identifier = parse("en-u-ca-islamic-umalqura-nu-latn");
        let unicode = &identifier.unicode[0];
        assert_eq!(unicode.keywords[0].value, 3..5);
        assert_eq!(unicode.keywords[1].value, 6..7);
    }
}
