// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 015's revision "0" conformance layer over an admitted artifact. It
//! takes one direct-input identifier through 015's five steps: its
//! structure, the validity of every component, key, and type, ICU4X 2.2.0's
//! canonicalization, the dataset's corrections followed by ICU4X once more,
//! and the result, which keeps no deprecated value and does not change when
//! steps 3 and 4 run on it again. It gives one canonical identifier or
//! rejects the whole identifier: it never strips a part it rejects, and it
//! reads nothing from the host.

use std::collections::BTreeSet;

use crate::conformance::{
    Extension, ExtensionKey, ExtensionType, LocaleConformanceData, TypeClass, ValidityField,
    ValidityStatus, ValueType,
};
use crate::identifier::{Field, Identifier, LanguageId};
use crate::markers::MarkerBlob;
use crate::reader::AdmittedData;

/// Why the layer gives no canonical identifier. It keeps no part of the
/// input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerFailure {
    /// Step 1, 2, or 5 rejects the input, which 015 reports as a blocking
    /// configuration Finding.
    Rejected(Rejection),
    /// ICU4X 2.2.0 does not parse an identifier steps 1 and 2 admit, or
    /// gives a result that is not one. An adapter gap: it blocks adapter
    /// admission and is never a user-input Finding.
    Engine,
    /// Steps 3 and 4 change the layer's own result. An adapter conformance
    /// failure: it blocks adapter admission and is never a user-input
    /// Finding.
    NotIdempotent,
}

/// The rule that rejects an input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rejection {
    /// Step 1: not a hyphen-separated Unicode BCP 47 Locale Identifier, under
    /// UTS #35's grammar and its well-formedness constraints.
    Syntax,
    /// Step 1: a second `u` or `t` extension.
    RepeatedExtension,
    /// Step 1: an extension singleton other than `u` and `t`.
    OtherExtension,
    /// Step 1: a private-use sequence.
    PrivateUse,
    /// Step 1: a `u` attribute, of which the pinned data registers none.
    Attribute,
    /// Step 1: a `u` key or a `t` field key given twice.
    RepeatedKey,
    /// Step 2 or 5: a component no admitted status lists, in the identifier
    /// or in a `t` extension's transformed language.
    Component(Component),
    /// Step 2 or 5: a key its extension does not list, or a deprecated one.
    Key,
    /// Step 2 or 5: a type its key does not admit.
    Type,
    /// Step 5: a deprecated component or type remains, for which the pinned
    /// data has no deterministic replacement.
    Deprecated,
}

/// The component a rejection names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Language,
    Script,
    Region,
    Variant,
}

impl AdmittedData {
    /// The canonical identifier of `input` under design 015's revision "0"
    /// conformance layer, or why it has none.
    pub fn canonicalize(&self, input: &str) -> Result<String, LayerFailure> {
        Layer {
            conformance: self.conformance(),
            markers: self.markers(),
        }
        .canonicalize(input)
    }
}

/// The layer over a dataset's two halves.
struct Layer<'a> {
    conformance: &'a LocaleConformanceData,
    markers: &'a MarkerBlob,
}

/// What a check reads: the input, where a deprecated value waits for step 5,
/// or the result, where none may remain and no correction source is a value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Stage {
    Input,
    Result,
}

impl Stage {
    /// A deprecated component or class value.
    const fn deprecated(self) -> Result<(), Rejection> {
        match self {
            Self::Input => Ok(()),
            Self::Result => Err(Rejection::Deprecated),
        }
    }
}

/// Whether a value is one of a class.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Membership {
    Admitted,
    /// A deprecated code, which step 5 decides.
    Deprecated,
    Absent,
}

impl Layer<'_> {
    fn canonicalize(&self, input: &str) -> Result<String, LayerFailure> {
        let identifier =
            Identifier::parse(input).map_err(|_| LayerFailure::Rejected(Rejection::Syntax))?;
        structure(&identifier).map_err(LayerFailure::Rejected)?;
        self.validate(&identifier, Stage::Input)
            .map_err(LayerFailure::Rejected)?;
        let result = self.canonical(input)?;
        self.validate(&read(&result)?, Stage::Result)
            .map_err(LayerFailure::Rejected)?;
        if self.canonical(&result)? != result {
            return Err(LayerFailure::NotIdempotent);
        }
        Ok(result)
    }

    /// Steps 3 and 4: ICU4X's canonical form, with every correction applied
    /// and canonicalized again, which restores canonical order and removes a
    /// type `true`.
    fn canonical(&self, input: &str) -> Result<String, LayerFailure> {
        let canonical = self.icu4x(input)?;
        let corrected = self.corrected(&read(&canonical)?);
        self.icu4x(&corrected)
    }

    fn icu4x(&self, input: &str) -> Result<String, LayerFailure> {
        self.markers
            .canonicalize_raw(input)
            .map_err(|_| LayerFailure::Engine)
    }

    /// `identifier` with each `u` or `t` type that is a correction's `from`
    /// replaced by its `to`.
    fn corrected(&self, identifier: &Identifier<'_>) -> String {
        let mut replacements: Vec<_> = fields(identifier)
            .filter_map(|(extension, field)| {
                let key = field.key.to_ascii_lowercase();
                self.conformance
                    .correction(extension, &key, &identifier.value(field))
                    .map(|to| (field.value.clone(), to))
            })
            .collect();
        // Splice in subtag order, whichever extension comes first.
        replacements.sort_by_key(|(range, _)| range.start);
        let mut subtags = Vec::with_capacity(identifier.subtags.len());
        let mut next = 0;
        for (range, to) in replacements {
            subtags.extend_from_slice(&identifier.subtags[next..range.start]);
            subtags.extend(to.split('-'));
            next = range.end;
        }
        subtags.extend_from_slice(&identifier.subtags[next..]);
        subtags.join("-")
    }

    /// Step 2 for the input, or the validity part of step 5 for the result:
    /// every component, then every key and type.
    fn validate(&self, identifier: &Identifier<'_>, stage: Stage) -> Result<(), Rejection> {
        let transformed = identifier
            .transformed
            .iter()
            .filter_map(|transformed| transformed.language.as_ref());
        for language in std::iter::once(&identifier.language).chain(transformed) {
            self.components(language, stage)?;
        }
        for (extension, field) in fields(identifier) {
            let key = field.key.to_ascii_lowercase();
            let subtags: Vec<String> = identifier.subtags[field.value.clone()]
                .iter()
                .map(|subtag| subtag.to_ascii_lowercase())
                .collect();
            self.field(extension, &key, &subtags, stage)?;
        }
        Ok(())
    }

    /// Each subtag of a language identifier, looked up in canonical case in
    /// the validity groups of its field.
    fn components(&self, language: &LanguageId<'_>, stage: Stage) -> Result<(), Rejection> {
        self.component(
            Component::Language,
            &language.language.to_ascii_lowercase(),
            stage,
        )?;
        if let Some(script) = language.script {
            self.component(Component::Script, &title(script), stage)?;
        }
        if let Some(region) = language.region {
            self.component(Component::Region, &region.to_ascii_uppercase(), stage)?;
        }
        for variant in &language.variants {
            self.component(Component::Variant, &variant.to_ascii_lowercase(), stage)?;
        }
        Ok(())
    }

    fn component(&self, component: Component, code: &str, stage: Stage) -> Result<(), Rejection> {
        match self.conformance.status(component.field(), code) {
            Some(
                ValidityStatus::Regular
                | ValidityStatus::Special
                | ValidityStatus::Macroregion
                | ValidityStatus::Unknown,
            ) => Ok(()),
            Some(ValidityStatus::Deprecated) => stage.deprecated(),
            Some(ValidityStatus::Reserved | ValidityStatus::PrivateUse) | None => {
                Err(Rejection::Component(component))
            }
        }
    }

    /// A `u` keyword or `t` field: a key its extension lists and that is not
    /// deprecated, then a type made of values as the key's `valueType` says.
    fn field(
        &self,
        extension: Extension,
        name: &str,
        subtags: &[String],
        stage: Stage,
    ) -> Result<(), Rejection> {
        let key = self
            .conformance
            .key(extension, name)
            .filter(|key| !key.deprecated())
            .ok_or(Rejection::Key)?;
        let value = |value: &str| self.value(extension, name, key, value, stage);
        match key.value_type() {
            // No type stands for `true`, where the key lists it.
            ValueType::Single if subtags.is_empty() => match key.literal("true") {
                Some(ExtensionType::Literal {
                    deprecated: false, ..
                }) => Ok(()),
                _ => Err(Rejection::Type),
            },
            // The whole type is one value: a listed sequence, for
            // `incremental`.
            ValueType::Single | ValueType::Incremental if !subtags.is_empty() => {
                value(&subtags.join("-"))
            }
            ValueType::Multiple | ValueType::Any if !subtags.is_empty() => {
                let mut seen = BTreeSet::new();
                for subtag in subtags {
                    if !seen.insert(subtag) && key.value_type() == ValueType::Multiple {
                        return Err(Rejection::Type);
                    }
                    value(subtag)?;
                }
                Ok(())
            }
            _ => Err(Rejection::Type),
        }
    }

    /// One value of `key`: a listed literal type that is not deprecated, a
    /// correction's `from` while the input is read, or a value of a class
    /// the key lists.
    fn value(
        &self,
        extension: Extension,
        name: &str,
        key: &ExtensionKey,
        value: &str,
        stage: Stage,
    ) -> Result<(), Rejection> {
        match key.literal(value) {
            Some(ExtensionType::Literal {
                deprecated: false, ..
            }) => return Ok(()),
            Some(ExtensionType::Literal {
                deprecated: true, ..
            }) if stage == Stage::Result => return Err(Rejection::Deprecated),
            _ => {}
        }
        if stage == Stage::Input
            && self
                .conformance
                .correction(extension, name, value)
                .is_some()
        {
            return Ok(());
        }
        for class in key.classes() {
            match self.membership(class, value) {
                Membership::Admitted => return Ok(()),
                Membership::Deprecated => return stage.deprecated(),
                Membership::Absent => {}
            }
        }
        Err(Rejection::Type)
    }

    /// Whether `value` is a value of `class`, by 015's table of classes.
    fn membership(&self, class: TypeClass, value: &str) -> Membership {
        let code = |field, code: &str, admitted: &[ValidityStatus]| match self
            .conformance
            .status(field, code)
        {
            Some(ValidityStatus::Deprecated) => Membership::Deprecated,
            Some(status) if admitted.contains(&status) => Membership::Admitted,
            _ => Membership::Absent,
        };
        match class {
            TypeClass::Codepoints => member(
                (4..=6).contains(&value.len())
                    && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                    && u32::from_str_radix(value, 16).is_ok_and(|point| point <= 0x10_FFFF),
            ),
            TypeClass::PrivateUse => member(
                (3..=8).contains(&value.len())
                    && value.bytes().all(|byte| byte.is_ascii_alphanumeric()),
            ),
            // The root collation's reordering block names are not in the
            // dataset; revision "0" stands for them with these values.
            TypeClass::ReorderCode => member(
                value == "others"
                    || self
                        .conformance
                        .status(ValidityField::Script, &title(value))
                        == Some(ValidityStatus::Regular),
            ),
            TypeClass::RgKeyValue => code(
                ValidityField::Subdivision,
                value,
                &[ValidityStatus::Regular, ValidityStatus::Unknown],
            ),
            TypeClass::ScriptCode => code(
                ValidityField::Script,
                &title(value),
                &[
                    ValidityStatus::Regular,
                    ValidityStatus::Special,
                    ValidityStatus::Unknown,
                ],
            ),
            TypeClass::SubdivisionCode => code(
                ValidityField::Subdivision,
                value,
                &[ValidityStatus::Regular],
            ),
        }
    }
}

impl Component {
    const fn field(self) -> ValidityField {
        match self {
            Self::Language => ValidityField::Language,
            Self::Script => ValidityField::Script,
            Self::Region => ValidityField::Region,
            Self::Variant => ValidityField::Variant,
        }
    }
}

/// Step 1 after the grammar: at most one `u` and one `t` extension, no
/// other singleton, no private use, no `u` attribute, and no key twice, in
/// 015's order.
fn structure(identifier: &Identifier<'_>) -> Result<(), Rejection> {
    if identifier.unicode.len() > 1 || identifier.transformed.len() > 1 {
        return Err(Rejection::RepeatedExtension);
    }
    if identifier.others > 0 {
        return Err(Rejection::OtherExtension);
    }
    if identifier.private_use {
        return Err(Rejection::PrivateUse);
    }
    if identifier
        .unicode
        .iter()
        .any(|unicode| !unicode.attributes.is_empty())
    {
        return Err(Rejection::Attribute);
    }
    // A `u` key and a `t` key never collide: their forms differ.
    let mut seen = BTreeSet::new();
    for (_, field) in fields(identifier) {
        if !seen.insert(field.key.to_ascii_lowercase()) {
            return Err(Rejection::RepeatedKey);
        }
    }
    Ok(())
}

/// Every `u` keyword and `t` field, with its extension.
fn fields<'i, 'a>(
    identifier: &'i Identifier<'a>,
) -> impl Iterator<Item = (Extension, &'i Field<'a>)> {
    let unicode = identifier
        .unicode
        .iter()
        .flat_map(|unicode| &unicode.keywords)
        .map(|field| (Extension::U, field));
    let transformed = identifier
        .transformed
        .iter()
        .flat_map(|transformed| &transformed.fields)
        .map(|field| (Extension::T, field));
    unicode.chain(transformed)
}

/// An identifier ICU4X gave, read by the same grammar.
fn read(canonical: &str) -> Result<Identifier<'_>, LayerFailure> {
    Identifier::parse(canonical).map_err(|_| LayerFailure::Engine)
}

const fn member(admitted: bool) -> Membership {
    if admitted {
        Membership::Admitted
    } else {
        Membership::Absent
    }
}

/// A script code in its canonical case, so `thai` is `Thai`.
fn title(code: &str) -> String {
    code.char_indices()
        .map(|(index, character)| {
            if index == 0 {
                character.to_ascii_uppercase()
            } else {
                character.to_ascii_lowercase()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use intlify_shared_json::text;
    use serde::Deserialize;
    use serde_json::{json, Value};

    use super::*;
    use crate::conformance::tests::GENEROUS;
    use crate::markers::tests::committed;

    const DATA: &[u8] = include_bytes!("../data/conformance.json");
    const CASES: &str = include_str!("../fixtures/layer/cases.json");

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Cases {
        dataset: String,
        groups: Vec<Group>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Group {
        rule: String,
        cases: Vec<Case>,
    }

    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Case {
        input: String,
        canonical: Option<String>,
        rejected: Option<String>,
    }

    fn data() -> LocaleConformanceData {
        LocaleConformanceData::read(DATA, &GENEROUS).unwrap()
    }

    /// Conformance data given as a value, which must be in 017's order.
    fn crafted(value: &Value) -> LocaleConformanceData {
        LocaleConformanceData::read(text::write(value).unwrap().as_bytes(), &GENEROUS).unwrap()
    }

    fn canonicalize(
        conformance: &LocaleConformanceData,
        markers: &MarkerBlob,
        input: &str,
    ) -> Result<String, LayerFailure> {
        Layer {
            conformance,
            markers,
        }
        .canonicalize(input)
    }

    fn spelled(failure: LayerFailure) -> &'static str {
        match failure {
            LayerFailure::Rejected(rejection) => match rejection {
                Rejection::Syntax => "syntax",
                Rejection::RepeatedExtension => "repeated-extension",
                Rejection::OtherExtension => "other-extension",
                Rejection::PrivateUse => "private-use",
                Rejection::Attribute => "attribute",
                Rejection::RepeatedKey => "repeated-key",
                Rejection::Component(Component::Language) => "language",
                Rejection::Component(Component::Script) => "script",
                Rejection::Component(Component::Region) => "region",
                Rejection::Component(Component::Variant) => "variant",
                Rejection::Key => "key",
                Rejection::Type => "type",
                Rejection::Deprecated => "deprecated",
            },
            LayerFailure::Engine => "engine",
            LayerFailure::NotIdempotent => "not-idempotent",
        }
    }

    const fn rejected(rejection: Rejection) -> Result<String, LayerFailure> {
        Err(LayerFailure::Rejected(rejection))
    }

    #[test]
    fn the_handwritten_015_cases_hold() {
        let cases: Cases = serde_json::from_str(CASES).unwrap();
        assert_eq!(cases.dataset, "intlify-locale-canonicalization-cldr-48.2.0");
        let (data, markers) = (data(), committed());
        let mut count = 0;
        for group in &cases.groups {
            for case in &group.cases {
                let outcome = canonicalize(&data, &markers, &case.input);
                match (&case.canonical, &case.rejected) {
                    (Some(canonical), None) => {
                        assert_eq!(
                            outcome.as_ref(),
                            Ok(canonical),
                            "{}: {}",
                            group.rule,
                            case.input
                        );
                    }
                    (None, Some(reason)) => {
                        let failure = outcome.expect_err(&case.input);
                        assert_eq!(spelled(failure), reason, "{}: {}", group.rule, case.input);
                    }
                    _ => panic!("{} has neither or both expectations", case.input),
                }
                count += 1;
            }
        }
        assert_eq!((cases.groups.len(), count), (17, 95));
    }

    /// How each code of each field and status fares in an identifier built
    /// for it, counted by outcome.
    fn outcomes(
        data: &LocaleConformanceData,
        markers: &MarkerBlob,
    ) -> BTreeMap<(String, String, String), usize> {
        let value: Value = serde_json::from_slice(DATA).unwrap();
        let mut outcomes = BTreeMap::new();
        for group in value["validity"].as_array().unwrap() {
            let field = group["field"].as_str().unwrap();
            let status = group["status"].as_str().unwrap();
            for code in group["codes"].as_array().unwrap() {
                let code = code.as_str().unwrap();
                let inputs = match field {
                    "language" => vec![("language", code.to_owned())],
                    "subdivision" => vec![
                        ("rg", format!("und-u-rg-{code}")),
                        ("sd", format!("und-u-sd-{code}")),
                    ],
                    _ => vec![(field, format!("und-{code}"))],
                };
                for (place, input) in inputs {
                    let outcome = match canonicalize(data, markers, &input) {
                        Ok(canonical) if canonical == input => "kept",
                        Ok(_) => "replaced",
                        Err(failure) => spelled(failure),
                    };
                    *outcomes
                        .entry((place.to_owned(), status.to_owned(), outcome.to_owned()))
                        .or_insert(0) += 1;
                }
            }
        }
        outcomes
    }

    #[test]
    fn every_listed_code_fares_as_its_status_says() {
        let outcomes = outcomes(&data(), &committed());
        let expected = [
            // Every admitted code is kept as it is, and no valid code is
            // rejected.
            ("language", "regular", "kept", 7965),
            ("language", "special", "kept", 3),
            ("language", "unknown", "kept", 1),
            ("region", "macroregion", "kept", 35),
            ("region", "regular", "kept", 257),
            ("region", "special", "kept", 2),
            ("region", "unknown", "kept", 1),
            ("script", "regular", "kept", 187),
            ("script", "special", "kept", 7),
            ("script", "unknown", "kept", 1),
            ("variant", "regular", "kept", 129),
            ("rg", "regular", "kept", 5027),
            ("rg", "unknown", "kept", 257),
            ("sd", "regular", "kept", 5027),
            // `sd` admits no unknown subdivision.
            ("sd", "unknown", "type", 257),
            ("language", "private-use", "language", 365),
            ("language", "reserved", "language", 155),
            ("region", "private-use", "region", 23),
            ("region", "reserved", "region", 13),
            ("script", "private-use", "script", 34),
            ("script", "reserved", "script", 14),
            // A deprecated code is replaced or left, which rejects it. Some
            // replacements are no subdivision `rg` or `sd` admits.
            ("language", "deprecated", "replaced", 227),
            ("language", "deprecated", "deprecated", 71),
            ("region", "deprecated", "replaced", 12),
            ("script", "deprecated", "replaced", 1),
            ("variant", "deprecated", "replaced", 3),
            ("variant", "deprecated", "deprecated", 2),
            ("rg", "deprecated", "replaced", 138),
            ("rg", "deprecated", "deprecated", 486),
            ("rg", "deprecated", "type", 2),
            ("sd", "deprecated", "replaced", 108),
            ("sd", "deprecated", "deprecated", 486),
            ("sd", "deprecated", "type", 32),
        ];
        let expected: BTreeMap<_, _> = expected
            .into_iter()
            .map(|(place, status, outcome, count)| {
                (
                    (place.to_owned(), status.to_owned(), outcome.to_owned()),
                    count,
                )
            })
            .collect();
        assert_eq!(outcomes, expected);
    }

    #[test]
    fn every_key_and_literal_type_fares_as_its_listing_says() {
        let value: Value = serde_json::from_slice(DATA).unwrap();
        let (data, markers) = (data(), committed());
        let mut fared = BTreeMap::new();
        for key in value["extensionKeys"].as_array().unwrap() {
            let (extension, name) = (
                key["extension"].as_str().unwrap(),
                key["key"].as_str().unwrap(),
            );
            for listed in key["types"].as_array().unwrap() {
                let Some(literal) = listed["type"].as_str() else {
                    continue;
                };
                let input = format!("und-{extension}-{name}-{literal}");
                let outcome = canonicalize(&data, &markers, &input);
                let correction = data.correction(
                    if extension == "t" {
                        Extension::T
                    } else {
                        Extension::U
                    },
                    name,
                    literal,
                );
                let expected = if key["deprecated"] == true {
                    rejected(Rejection::Key)
                } else if listed["deprecated"] == false {
                    // A type `true` is written as no type.
                    Ok(input.strip_suffix("-true").unwrap_or(&input).to_owned())
                } else if let Some(to) = correction {
                    Ok(format!("und-{extension}-{name}-{to}"))
                } else {
                    rejected(Rejection::Type)
                };
                assert_eq!(outcome, expected, "{input}");
                *fared.entry(spelled_outcome(&outcome)).or_insert(0) += 1;
            }
        }
        let fared: Vec<_> = fared.into_iter().collect();
        assert_eq!(
            fared,
            [("key", 2), ("ok", 1066), ("type", 6)],
            "the 6 deprecated types without a replacement are hc-c12, hc-c24, and four co types"
        );
    }

    fn spelled_outcome(outcome: &Result<String, LayerFailure>) -> &'static str {
        outcome
            .as_ref()
            .map_or_else(|failure| spelled(*failure), |_| "ok")
    }

    #[test]
    fn every_correction_is_applied_and_its_result_is_stable() {
        let value: Value = serde_json::from_slice(DATA).unwrap();
        let (data, markers) = (data(), committed());
        let corrections = value["corrections"].as_array().unwrap();
        for correction in corrections {
            let [extension, key, from, to] = ["extension", "key", "from", "to"]
                .map(|member| correction[member].as_str().unwrap());
            let input = format!("und-{extension}-{key}-{from}");
            let expected = if to == "true" {
                format!("und-{extension}-{key}")
            } else {
                format!("und-{extension}-{key}-{to}")
            };
            assert_eq!(
                canonicalize(&data, &markers, &input),
                Ok(expected),
                "{input}"
            );
        }
        assert_eq!(corrections.len(), 64);
    }

    #[test]
    fn step_1_names_015s_rules_in_order() {
        let (data, markers) = (data(), committed());
        for (input, rejection) in [
            ("en-u-attr-u-nu-latn", Rejection::RepeatedExtension),
            ("en-t-ja-a-foo-t-it", Rejection::RepeatedExtension),
            ("en-a-foo-x-bar", Rejection::OtherExtension),
            ("en-u-attr-x-bar", Rejection::PrivateUse),
            ("en-u-attr-ca-gregory-ca-buddhist", Rejection::Attribute),
            ("en-u-CA-gregory-ca-buddhist", Rejection::RepeatedKey),
            ("zz-u-zz-abc", Rejection::Component(Component::Language)),
            (
                "en-u-zz-abc-t-qfz",
                Rejection::Component(Component::Language),
            ),
            ("en-u-ca-madeup-zz-abc", Rejection::Type),
        ] {
            assert_eq!(
                canonicalize(&data, &markers, input),
                rejected(rejection),
                "{input}"
            );
        }
        assert!(canonicalize(&data, &markers, "und-t-m0-bgn-u-ca-gregory").is_ok());
    }

    #[test]
    fn a_language_identifier_is_checked_part_by_part() {
        let (data, markers) = (data(), committed());
        for (input, rejection) in [
            ("en-Qaaq", Component::Script),
            ("en-Latn-XC", Component::Region),
            ("en-Latn-US-fonipa-abcdefg", Component::Variant),
            ("en-t-en-latn-us-abcdefg", Component::Variant),
        ] {
            assert_eq!(
                canonicalize(&data, &markers, input),
                rejected(Rejection::Component(rejection)),
                "{input}"
            );
        }
        assert_eq!(
            canonicalize(&data, &markers, "SL-latn-it-ROZAJ-1994"),
            Ok("sl-Latn-IT-1994-rozaj".to_owned())
        );
    }

    #[test]
    fn a_class_admits_its_values_and_marks_deprecated_codes() {
        let (data, markers) = (data(), committed());
        let layer = Layer {
            conformance: &data,
            markers: &markers,
        };
        let member = |class, value| layer.membership(class, value);
        for (class, value, expected) in [
            (TypeClass::Codepoints, "0041", Membership::Admitted),
            (TypeClass::Codepoints, "10ffff", Membership::Admitted),
            (TypeClass::Codepoints, "110000", Membership::Absent),
            (TypeClass::Codepoints, "041", Membership::Absent),
            (TypeClass::Codepoints, "0000041", Membership::Absent),
            (TypeClass::Codepoints, "+041", Membership::Absent),
            (TypeClass::Codepoints, "00g1", Membership::Absent),
            (TypeClass::PrivateUse, "abc", Membership::Admitted),
            (TypeClass::PrivateUse, "abcdefgh", Membership::Admitted),
            (TypeClass::PrivateUse, "ab", Membership::Absent),
            (TypeClass::PrivateUse, "abcdefghi", Membership::Absent),
            (TypeClass::PrivateUse, "abc-def", Membership::Absent),
            (TypeClass::ReorderCode, "others", Membership::Admitted),
            (TypeClass::ReorderCode, "latn", Membership::Admitted),
            (TypeClass::ReorderCode, "zyyy", Membership::Absent),
            (TypeClass::ReorderCode, "qaai", Membership::Absent),
            (TypeClass::RgKeyValue, "uszzzz", Membership::Admitted),
            (TypeClass::RgKeyValue, "gbsct", Membership::Admitted),
            (TypeClass::RgKeyValue, "no23", Membership::Deprecated),
            (TypeClass::RgKeyValue, "us", Membership::Absent),
            (TypeClass::ScriptCode, "thai", Membership::Admitted),
            (TypeClass::ScriptCode, "zyyy", Membership::Admitted),
            (TypeClass::ScriptCode, "zzzz", Membership::Admitted),
            (TypeClass::ScriptCode, "qaai", Membership::Deprecated),
            (TypeClass::ScriptCode, "qaaq", Membership::Absent),
            (TypeClass::SubdivisionCode, "gbsct", Membership::Admitted),
            (TypeClass::SubdivisionCode, "no23", Membership::Deprecated),
            (TypeClass::SubdivisionCode, "uszzzz", Membership::Absent),
            // A code of another field is not one, though spelled alike.
            (TypeClass::SubdivisionCode, "fonipa", Membership::Absent),
            (TypeClass::RgKeyValue, "1994", Membership::Absent),
        ] {
            assert_eq!(member(class, value), expected, "{class:?} {value}");
        }
    }

    #[test]
    fn corrections_are_spliced_in_subtag_order() {
        let (data, markers) = (data(), committed());
        let layer = Layer {
            conformance: &data,
            markers: &markers,
        };
        let corrected = |input| layer.corrected(&Identifier::parse(input).unwrap());
        assert_eq!(
            corrected("und-u-ms-imperial-t-m0-names"),
            "und-u-ms-uksystem-t-m0-prprname"
        );
        assert_eq!(
            corrected("und-t-m0-beta-metsehaf-u-kn-yes-ms-imperial"),
            "und-t-m0-betamets-u-kn-true-ms-uksystem"
        );
        assert_eq!(corrected("und-u-ca-gregory"), "und-u-ca-gregory");
        // Keys and types are looked up in lowercase; other subtags keep
        // their case.
        assert_eq!(corrected("UND-u-MS-Imperial"), "UND-u-MS-uksystem");
    }

    #[test]
    fn an_identifier_icu4x_cannot_parse_blocks_the_adapter() {
        // A later specification that admits a five-letter language.
        let data = crafted(&json!({
            "corrections": [],
            "extensionKeys": [],
            "validity": [{"field": "language", "status": "regular", "codes": ["abcde"]}],
        }));
        assert_eq!(
            canonicalize(&data, &committed(), "abcde"),
            Err(LayerFailure::Engine)
        );
    }

    /// One key, `sd`, with the subdivisions and corrections given.
    fn subdivisions(validity: &Value, corrections: &[(&str, &str)]) -> LocaleConformanceData {
        crafted(&json!({
            "corrections": corrections.iter().map(|(from, to)| json!({"extension": "u", "key": "sd", "from": from, "to": to})).collect::<Vec<_>>(),
            "extensionKeys": [{"extension": "u", "key": "sd", "deprecated": false, "valueType": "single", "types": [{"kind": "class", "class": "subdivision-code"}]}],
            "validity": validity,
        }))
    }

    #[test]
    fn a_result_steps_3_and_4_change_blocks_the_adapter() {
        // `aaa` becomes `no23`, ICU4X replaces that by `no50`, and a second
        // pass would correct `no50` to `ccc`.
        let data = subdivisions(
            &json!([
                {"field": "language", "status": "unknown", "codes": ["und"]},
                {"field": "subdivision", "status": "deprecated", "codes": ["no23"]},
                {"field": "subdivision", "status": "regular", "codes": ["no50"]},
            ]),
            &[("aaa", "no23"), ("no50", "ccc")],
        );
        assert_eq!(
            canonicalize(&data, &committed(), "und-u-sd-aaa"),
            Err(LayerFailure::NotIdempotent)
        );
    }

    #[test]
    fn a_correction_source_is_no_value_of_a_result() {
        // As above, but `no50` is no subdivision the data lists, so only its
        // being a correction's source could admit it.
        let data = subdivisions(
            &json!([
                {"field": "language", "status": "unknown", "codes": ["und"]},
                {"field": "subdivision", "status": "deprecated", "codes": ["no23"]},
            ]),
            &[("aaa", "no23"), ("no50", "ccc")],
        );
        assert_eq!(
            canonicalize(&data, &committed(), "und-u-sd-aaa"),
            rejected(Rejection::Type)
        );
    }

    #[test]
    fn a_deprecated_literal_type_left_in_a_result_is_rejected() {
        let data = crafted(&json!({
            "corrections": [{"extension": "u", "key": "ms", "from": "ccc", "to": "bbb"}],
            "extensionKeys": [{"extension": "u", "key": "ms", "deprecated": false, "valueType": "single", "types": [{"kind": "literal", "type": "bbb", "deprecated": true}]}],
            "validity": [{"field": "language", "status": "unknown", "codes": ["und"]}],
        }));
        assert_eq!(
            canonicalize(&data, &committed(), "und-u-ms-ccc"),
            rejected(Rejection::Deprecated)
        );
    }

    #[test]
    fn no_type_stands_for_true_only_where_the_key_admits_true() {
        let data = crafted(&json!({
            "corrections": [],
            "extensionKeys": [
                {"extension": "u", "key": "kb", "deprecated": false, "valueType": "single", "types": [{"kind": "literal", "type": "true", "deprecated": true}]},
                {"extension": "u", "key": "kn", "deprecated": false, "valueType": "single", "types": [{"kind": "literal", "type": "true", "deprecated": false}]},
            ],
            "validity": [{"field": "language", "status": "unknown", "codes": ["und"]}],
        }));
        let markers = committed();
        assert_eq!(
            canonicalize(&data, &markers, "und-u-kb"),
            rejected(Rejection::Type)
        );
        assert_eq!(
            canonicalize(&data, &markers, "und-u-kn").as_deref(),
            Ok("und-u-kn")
        );
    }

    #[test]
    fn a_deprecated_value_waits_for_step_5_only_while_the_input_is_read() {
        assert_eq!(Stage::Input.deprecated(), Ok(()));
        assert_eq!(Stage::Result.deprecated(), Err(Rejection::Deprecated));
    }

    #[test]
    fn a_script_code_is_looked_up_in_title_case() {
        assert_eq!(title("thai"), "Thai");
        assert_eq!(title("THAI"), "Thai");
        assert_eq!(title(""), "");
    }

    #[test]
    fn admitted_data_canonicalizes_through_the_layer() {
        let admitted = crate::reader::tests::committed();
        assert_eq!(admitted.canonicalize("iw-IL").as_deref(), Ok("he-IL"));
        assert_eq!(
            admitted.canonicalize("und-u-ca-islamicc").as_deref(),
            Ok("und-u-ca-islamic-civil")
        );
        assert_eq!(
            admitted.canonicalize("zz"),
            rejected(Rejection::Component(Component::Language))
        );
        // ICU4X alone keeps what the layer rejects.
        assert_eq!(
            admitted.markers().canonicalize_raw("zz").as_deref(),
            Ok("zz")
        );
    }
}
