// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The corpus gate of design 015's initial conformance-gap inventory. It
//! reads CLDR 48.2.0's locale canonicalization corpus and projects each case
//! of its four sets into revision "0"'s direct-input syntax. Whether the input
//! is in the admitted domain comes from the pinned validity data, never from
//! the layer's answer. The gate then compares the result CLDR expects with
//! raw ICU4X 2.2.0 and with ICU4X through the conformance layer, and gives
//! each case one disposition. The registry of every case is the adapter's
//! conformance evidence; a single blocking case refuses the adapter.

use std::collections::BTreeSet;

use serde_json::{json, Map, Value};

use crate::conformance::{LocaleConformanceData, ValidityField, ValidityStatus};
use crate::identifier::Identifier;
use crate::layer::{Component, LayerFailure, Rejection};
use crate::primitives::FileDigest;
use crate::reader::AdmittedData;
use crate::{RawFailure, ENGINE};

/// The corpus's place in the CLDR distribution root.
pub const CORPUS_PATH: &str = "common/testData/localeIdentifiers/localeCanonicalization.txt";

/// The CLDR release and tag the corpus is read at: those of revision "0".
pub const CORPUS_RELEASE: (&str, &str) = ("48.2.0", "release-48-2");

const KIND: &str = "intlify-locale-canonicalization-gap-registry";
const SCHEMA_REVISION: &str = "0";
const GENERATOR: (&str, &str) = ("intlify-locale-icu4x-corpus-gate", "0");
const PROJECTION: (&str, &str) = ("cldr-subtags-joined-by-hyphens", "0");

/// The corpus's four sets, in their upstream order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Set {
    Explicit,
    FromAliases,
    Decanonicalized,
    WithIrrelevants,
}

impl Set {
    const ALL: [Self; 4] = [
        Self::Explicit,
        Self::FromAliases,
        Self::Decanonicalized,
        Self::WithIrrelevants,
    ];

    /// The set's name as the corpus heads it.
    pub const fn name(self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::FromAliases => "fromAliases",
            Self::Decanonicalized => "decanonicalized",
            Self::WithIrrelevants => "withIrrelevants",
        }
    }
}

/// One upstream case, in CLDR's underscore-separated syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Case<'a> {
    pub set: Set,
    /// The case's line in the corpus, from 1.
    pub line: usize,
    pub source: &'a str,
    pub expected: &'a str,
}

/// Why the gate does not read a corpus. It never skips a line it does not
/// recognize.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusFailure {
    /// A line that is no comment, set heading, or `source<TAB>;<TAB>expected`
    /// case, or a case before the first set heading.
    Line(usize),
    /// A set heading other than the next set in the upstream order.
    SetOrder(usize),
    /// The corpus ends before its last set.
    MissingSets,
    /// A source an earlier case already gave.
    Duplicate(usize),
}

/// Every case of `corpus`, in its order.
pub fn cases(corpus: &str) -> Result<Vec<Case<'_>>, CorpusFailure> {
    let mut set: Option<Set> = None;
    let mut sources = BTreeSet::new();
    let mut cases = Vec::new();
    for (index, line) in corpus.lines().enumerate() {
        let number = index + 1;
        if let Some(name) = line.strip_prefix("# ") {
            if let Some(&heading) = Set::ALL.iter().find(|set| set.name() == name) {
                // Each set comes once, after the one before it upstream.
                if set.map_or(0, |set| set as usize + 1) != heading as usize {
                    return Err(CorpusFailure::SetOrder(number));
                }
                set = Some(heading);
            }
            continue;
        }
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split('\t').collect();
        let (Some(set), &[source, ";", expected]) = (set, fields.as_slice()) else {
            return Err(CorpusFailure::Line(number));
        };
        if source.is_empty() || expected.is_empty() {
            return Err(CorpusFailure::Line(number));
        }
        if !sources.insert(source) {
            return Err(CorpusFailure::Duplicate(number));
        }
        cases.push(Case {
            set,
            line: number,
            source,
            expected,
        });
    }
    if set != Some(Set::WithIrrelevants) {
        return Err(CorpusFailure::MissingSets);
    }
    Ok(cases)
}

/// 015's projection of a CLDR identifier into the direct-input syntax: its
/// underscore-separated subtags, joined by hyphens.
fn project(cldr: &str) -> String {
    cldr.replace('_', "-")
}

/// Why a case is outside revision "0"'s admitted direct domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outside {
    /// The projected input is no Unicode BCP 47 locale identifier.
    NotLocaleIdentifier,
    /// A component the pinned validity data does not admit. No status means
    /// the data does not list the code at all.
    Component {
        component: Component,
        code: String,
        status: Option<ValidityStatus>,
    },
}

/// Why the gate gives no registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GateFailure {
    Corpus(CorpusFailure),
    /// A projected input with an extension or a private-use sequence, which
    /// CLDR 48.2.0's corpus does not have and this gate does not classify.
    Extension(usize),
}

/// Whether `input` is in the admitted domain, from the pinned validity data:
/// every component must have a status the layer admits on input.
fn domain(
    conformance: &LocaleConformanceData,
    input: &str,
    line: usize,
) -> Result<Option<Outside>, GateFailure> {
    let Ok(identifier) = Identifier::parse(input) else {
        return Ok(Some(Outside::NotLocaleIdentifier));
    };
    if !identifier.unicode.is_empty()
        || !identifier.transformed.is_empty()
        || identifier.others != 0
        || identifier.private_use
    {
        return Err(GateFailure::Extension(line));
    }
    let language = &identifier.language;
    let mut components = vec![(
        Component::Language,
        ValidityField::Language,
        language.language.to_ascii_lowercase(),
    )];
    if let Some(script) = language.script {
        let mut code = script.to_ascii_lowercase();
        code[..1].make_ascii_uppercase();
        components.push((Component::Script, ValidityField::Script, code));
    }
    if let Some(region) = language.region {
        components.push((
            Component::Region,
            ValidityField::Region,
            region.to_ascii_uppercase(),
        ));
    }
    for variant in &language.variants {
        components.push((
            Component::Variant,
            ValidityField::Variant,
            variant.to_ascii_lowercase(),
        ));
    }
    Ok(components.into_iter().find_map(|(component, field, code)| {
        match conformance.status(field, &code) {
            Some(
                ValidityStatus::Regular
                | ValidityStatus::Special
                | ValidityStatus::Macroregion
                | ValidityStatus::Unknown
                | ValidityStatus::Deprecated,
            ) => None,
            status @ (Some(ValidityStatus::Reserved | ValidityStatus::PrivateUse) | None) => {
                Some(Outside::Component {
                    component,
                    code,
                    status,
                })
            }
        }
    }))
}

/// What the gate makes of one case.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Disposition {
    /// In the domain; raw ICU4X and the layer both give the result CLDR expects.
    Delegated,
    /// In the domain; only the layer gives the result CLDR expects.
    Corrected,
    /// Outside the domain, and the layer rejects the input for the reason the
    /// data gives.
    OutsideDomain,
    /// Anything else: an unexplained mismatch, which refuses the adapter.
    Blocking,
}

impl Disposition {
    const fn name(self) -> &'static str {
        match self {
            Self::Delegated => "delegated",
            Self::Corrected => "corrected",
            Self::OutsideDomain => "outside-domain",
            Self::Blocking => "blocking",
        }
    }
}

fn disposition(
    outside: Option<&Outside>,
    normative: &str,
    raw: &Result<String, RawFailure>,
    layer: &Result<String, LayerFailure>,
) -> Disposition {
    let Some(outside) = outside else {
        return match layer {
            Ok(answer) if answer == normative => {
                if raw.as_deref() == Ok(normative) {
                    Disposition::Delegated
                } else {
                    Disposition::Corrected
                }
            }
            _ => Disposition::Blocking,
        };
    };
    let rejection = match outside {
        Outside::NotLocaleIdentifier => Rejection::Syntax,
        Outside::Component { component, .. } => Rejection::Component(*component),
    };
    if *layer == Err(LayerFailure::Rejected(rejection)) {
        Disposition::OutsideDomain
    } else {
        Disposition::Blocking
    }
}

/// How many cases have each disposition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counts {
    pub delegated: usize,
    pub corrected: usize,
    pub outside_domain: usize,
    pub blocking: usize,
}

impl Counts {
    pub const fn cases(&self) -> usize {
        self.delegated + self.corrected + self.outside_domain + self.blocking
    }

    fn add(&mut self, disposition: Disposition) {
        match disposition {
            Disposition::Delegated => self.delegated += 1,
            Disposition::Corrected => self.corrected += 1,
            Disposition::OutsideDomain => self.outside_domain += 1,
            Disposition::Blocking => self.blocking += 1,
        }
    }

    fn sum(sets: &[Self]) -> Self {
        let mut total = Self::default();
        for set in sets {
            total.delegated += set.delegated;
            total.corrected += set.corrected;
            total.outside_domain += set.outside_domain;
            total.blocking += set.blocking;
        }
        total
    }

    fn json(&self) -> Map<String, Value> {
        let mut map = Map::new();
        for (name, count) in [
            ("cases", self.cases()),
            ("delegated", self.delegated),
            ("corrected", self.corrected),
            ("outsideDomain", self.outside_domain),
            ("blocking", self.blocking),
        ] {
            map.insert(name.into(), json!(count.to_string()));
        }
        map
    }
}

/// The corpus gate's result: every case with its disposition, and the
/// identities of the corpus and the admitted artifact it was run against.
#[derive(Debug, Clone, PartialEq)]
pub struct GapRegistry {
    head: Map<String, Value>,
    cases: Vec<Value>,
    sets: [Counts; 4],
}

impl GapRegistry {
    /// Runs the gate over `corpus` through the adapter `data` admits.
    pub fn run(corpus: &str, data: &AdmittedData) -> Result<Self, GateFailure> {
        let mut sets = [Counts::default(); 4];
        let mut records = Vec::new();
        for case in cases(corpus).map_err(GateFailure::Corpus)? {
            let input = project(case.source);
            let normative = project(case.expected);
            let outside = domain(data.conformance(), &input, case.line)?;
            let raw = data.markers().canonicalize_raw(&input);
            let layer = data.canonicalize(&input);
            let disposition = disposition(outside.as_ref(), &normative, &raw, &layer);
            sets[case.set as usize].add(disposition);
            let mut record = json!({
                "case": format!("{}/{}", case.set.name(), case.source),
                "line": case.line.to_string(),
                "expected": case.expected,
                "input": input,
                "normative": normative,
                "raw": raw_answer(&raw),
                "layer": layer_answer(&layer),
                "disposition": disposition.name(),
            });
            if let Some(outside) = &outside {
                record["outside"] = outside_reason(outside);
            }
            records.push(record);
        }
        let manifest = data.manifest();
        let specification = manifest.specification();
        let dataset = manifest.dataset();
        let mut head = Map::new();
        head.insert("kind".into(), json!(KIND));
        head.insert("schemaRevision".into(), json!(SCHEMA_REVISION));
        head.insert(
            "generator".into(),
            json!({"identity": GENERATOR.0, "revision": GENERATOR.1}),
        );
        head.insert(
            "corpus".into(),
            json!({
                "cldr": {"revision": CORPUS_RELEASE.0, "tag": CORPUS_RELEASE.1},
                "path": CORPUS_PATH,
                "digest": FileDigest::of(corpus.as_bytes()).as_str(),
                "projection": {"identity": PROJECTION.0, "revision": PROJECTION.1},
            }),
        );
        head.insert(
            "specification".into(),
            json!({
                "identity": specification.identity().as_str(),
                "revision": specification.revision().as_str(),
            }),
        );
        head.insert(
            "dataset".into(),
            json!({
                "identity": dataset.identity().as_str(),
                "semanticDigest": dataset.semantic_digest().as_str(),
            }),
        );
        head.insert(
            "engine".into(),
            json!({"identity": ENGINE.0, "revision": ENGINE.1}),
        );
        head.insert(
            "artifactIntegrity".into(),
            json!(manifest.integrity_digest().as_str()),
        );
        Ok(Self {
            head,
            cases: records,
            sets,
        })
    }

    /// The dispositions of every case.
    pub fn counts(&self) -> Counts {
        Counts::sum(&self.sets)
    }

    /// The dispositions of one set's cases.
    pub fn set(&self, set: Set) -> Counts {
        self.sets[set as usize]
    }

    /// Whether the gate admits the adapter: no case is blocking.
    pub fn admitted(&self) -> bool {
        self.counts().blocking == 0
    }

    /// The registry as committed: its identities and outcome, then one case
    /// per line.
    pub fn text(&self) -> String {
        let mut head = self.head.clone();
        let mut outcome = self.counts().json();
        outcome.insert("admitted".into(), json!(self.admitted()));
        head.insert("outcome".into(), Value::Object(outcome));
        let sets = Set::ALL
            .iter()
            .map(|&set| {
                let mut entry = Map::new();
                entry.insert("set".into(), json!(set.name()));
                entry.extend(self.set(set).json());
                Value::Object(entry)
            })
            .collect();
        head.insert("sets".into(), Value::Array(sets));
        let head = Value::Object(head).to_string();
        let cases: Vec<String> = self.cases.iter().map(Value::to_string).collect();
        format!(
            "{},\"cases\":[\n{}\n]}}\n",
            &head[..head.len() - 1],
            cases.join(",\n")
        )
    }
}

fn raw_answer(raw: &Result<String, RawFailure>) -> Value {
    match raw {
        Ok(canonical) => json!({ "canonical": canonical }),
        Err(RawFailure::Unparsable) => json!("unparsable"),
    }
}

fn layer_answer(layer: &Result<String, LayerFailure>) -> Value {
    match layer {
        Ok(canonical) => json!({ "canonical": canonical }),
        Err(LayerFailure::Rejected(rejection)) => json!({ "rejected": rejection_name(*rejection) }),
        Err(LayerFailure::Engine) => json!("engine"),
        Err(LayerFailure::NotIdempotent) => json!("not-idempotent"),
    }
}

const fn rejection_name(rejection: Rejection) -> &'static str {
    match rejection {
        Rejection::Syntax => "syntax",
        Rejection::RepeatedExtension => "repeated-extension",
        Rejection::OtherExtension => "other-extension",
        Rejection::PrivateUse => "private-use",
        Rejection::Attribute => "attribute",
        Rejection::RepeatedKey => "repeated-key",
        Rejection::Component(component) => component_name(component),
        Rejection::Key => "key",
        Rejection::Type => "type",
        Rejection::Deprecated => "deprecated",
    }
}

const fn component_name(component: Component) -> &'static str {
    match component {
        Component::Language => "language",
        Component::Script => "script",
        Component::Region => "region",
        Component::Variant => "variant",
    }
}

fn outside_reason(outside: &Outside) -> Value {
    match outside {
        Outside::NotLocaleIdentifier => json!({"reason": "not-a-locale-identifier"}),
        Outside::Component {
            component,
            code,
            status,
        } => json!({
            "reason": "component",
            "component": component_name(*component),
            "code": code,
            "status": match status {
                None => "absent",
                Some(ValidityStatus::Reserved) => "reserved",
                Some(_) => "private-use",
            },
        }),
    }
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::*;
    use crate::reader::tests::committed;

    const CORPUS: &str =
        include_str!("../cldr/common/testData/localeIdentifiers/localeCanonicalization.txt");
    const REGISTRY: &str = include_str!("../evidence/gap-registry.json");
    const MANIFEST: &str = include_str!("../data/manifest.json");
    const HANDWRITTEN: &str = include_str!("../fixtures/layer/cases.json");

    /// A corpus of `body`'s cases in the explicit set, with every heading.
    fn explicit(body: &str) -> String {
        format!("# explicit\n{body}# fromAliases\n# decanonicalized\n# withIrrelevants\n")
    }

    fn counts(delegated: usize, corrected: usize, outside: usize, blocking: usize) -> Counts {
        Counts {
            delegated,
            corrected,
            outside_domain: outside,
            blocking,
        }
    }

    #[test]
    fn the_committed_registry_is_the_gate_over_the_vendored_corpus() {
        let registry = GapRegistry::run(CORPUS, &committed()).unwrap();
        assert_eq!(registry.text(), REGISTRY);
        assert!(registry.admitted());
        assert_eq!(registry.counts(), counts(542, 0, 1231, 0));
        for (set, expected) in [
            (Set::Explicit, counts(6, 0, 10, 0)),
            (Set::FromAliases, counts(263, 0, 546, 0)),
            (Set::Decanonicalized, counts(2, 0, 60, 0)),
            (Set::WithIrrelevants, counts(271, 0, 615, 0)),
        ] {
            assert_eq!(registry.set(set), expected, "{}", set.name());
        }
    }

    #[test]
    fn every_upstream_case_is_read_once_in_its_set() {
        let cases = cases(CORPUS).unwrap();
        assert_eq!(cases.len(), 1773);
        for (set, first, last) in [
            (
                Set::Explicit,
                ("art_lojban", "jbo", 27),
                ("zh_hakka_xiang", "hak", 42),
            ),
            (
                Set::FromAliases,
                ("aa_saaho", "ssy", 46),
                ("zyb", "za", 854),
            ),
            (
                Set::Decanonicalized,
                ("aar_saaho", "ssy", 858),
                ("zho_xiang", "hsn", 919),
            ),
            (
                Set::WithIrrelevants,
                ("aa_Adlm_AC_fonipa_saaho", "ssy_Adlm_AC_fonipa", 923),
                ("zyb_Adlm_AC_fonipa", "za_Adlm_AC_fonipa", 1808),
            ),
        ] {
            let in_set: Vec<_> = cases.iter().filter(|case| case.set == set).collect();
            for (case, (source, expected, line)) in
                [(in_set[0], first), (in_set[in_set.len() - 1], last)]
            {
                assert_eq!(
                    (case.source, case.expected, case.line),
                    (source, expected, line),
                    "{}",
                    set.name()
                );
            }
        }
    }

    #[test]
    fn a_line_the_gate_does_not_recognize_stops_it() {
        let read = |corpus: &str| cases(corpus).map(|cases| cases.len());
        // Comments, rules, and blank lines are not cases.
        assert_eq!(read(&explicit("# a comment\n######\n\nen\t;\ten\n")), Ok(1));
        for (body, line) in [
            ("en ; en\n", 2),
            ("en\t;\ten\textra\n", 2),
            ("en\t:\ten\n", 2),
            ("\t;\ten\n", 2),
            ("en\t;\t\n", 2),
        ] {
            assert_eq!(
                read(&explicit(body)),
                Err(CorpusFailure::Line(line)),
                "{body:?}"
            );
        }
        assert_eq!(
            read("en\t;\ten\n# explicit\n# fromAliases\n# decanonicalized\n# withIrrelevants\n"),
            Err(CorpusFailure::Line(1))
        );
        for (corpus, line) in [
            ("# fromAliases\n", 1),
            ("# explicit\n# explicit\n", 2),
            ("# explicit\n# decanonicalized\n", 2),
        ] {
            assert_eq!(
                read(corpus),
                Err(CorpusFailure::SetOrder(line)),
                "{corpus:?}"
            );
        }
        assert_eq!(
            read("# explicit\n# fromAliases\n# decanonicalized\n"),
            Err(CorpusFailure::MissingSets)
        );
        assert_eq!(
            read("# explicit\nen\t;\ten\n# fromAliases\nen\t;\ten\n# decanonicalized\n# withIrrelevants\n"),
            Err(CorpusFailure::Duplicate(4))
        );
    }

    #[test]
    fn the_domain_comes_from_the_pinned_validity_data() {
        let data = committed();
        let domain = |input: &str| domain(data.conformance(), input, 9);
        let outside = |component, code: &str, status| {
            Ok(Some(Outside::Component {
                component,
                code: code.into(),
                status,
            }))
        };
        // Every admitted status, in any case.
        for input in [
            "en-Latn-US-fonipa",
            "EN-latn-us-FONIPA",
            "zxx-Zmth-XA",
            "und-Zzzz-ZZ",
            "en-001",
            "aam-heploc",
        ] {
            assert_eq!(domain(input), Ok(None), "{input}");
        }
        assert_eq!(domain("aar"), outside(Component::Language, "aar", None));
        assert_eq!(
            domain("qaa"),
            outside(Component::Language, "qaa", Some(ValidityStatus::Reserved))
        );
        assert_eq!(
            domain("qfz"),
            outside(Component::Language, "qfz", Some(ValidityStatus::PrivateUse))
        );
        assert_eq!(
            domain("en-qaaq"),
            outside(Component::Script, "Qaaq", Some(ValidityStatus::PrivateUse))
        );
        assert_eq!(
            domain("en-Latn-aa"),
            outside(Component::Region, "AA", Some(ValidityStatus::Reserved))
        );
        assert_eq!(domain("und-004"), outside(Component::Region, "004", None));
        assert_eq!(
            domain("en-US-fonipa-aaland"),
            outside(Component::Variant, "aaland", None)
        );
        // The first component that fails is the reason.
        assert_eq!(domain("aar-004"), outside(Component::Language, "aar", None));
        assert_eq!(domain("root"), Ok(Some(Outside::NotLocaleIdentifier)));
        for input in ["en-u-ca-gregory", "en-t-ja", "en-a-foo", "en-x-foo"] {
            assert_eq!(domain(input), Err(GateFailure::Extension(9)), "{input}");
        }
    }

    #[test]
    fn a_disposition_follows_the_domain_and_both_answers() {
        use Disposition::{Blocking, Corrected, Delegated, OutsideDomain};
        let ok = |answer: &str| Ok::<String, LayerFailure>(answer.into());
        let raw = |answer: &str| Ok::<String, RawFailure>(answer.into());
        let rejected = |rejection| Err::<String, _>(LayerFailure::Rejected(rejection));
        assert_eq!(disposition(None, "en", &raw("en"), &ok("en")), Delegated);
        assert_eq!(disposition(None, "en", &raw("und"), &ok("en")), Corrected);
        assert_eq!(
            disposition(None, "en", &Err(RawFailure::Unparsable), &ok("en")),
            Corrected
        );
        assert_eq!(disposition(None, "en", &raw("en"), &ok("fr")), Blocking);
        assert_eq!(
            disposition(None, "en", &raw("en"), &rejected(Rejection::Syntax)),
            Blocking
        );
        let variant = Outside::Component {
            component: Component::Variant,
            code: "aaland".into(),
            status: None,
        };
        let variant_rejected = rejected(Rejection::Component(Component::Variant));
        assert_eq!(
            disposition(Some(&variant), "en-AX", &raw("en-AX"), &variant_rejected),
            OutsideDomain
        );
        // Outside the domain, the layer must reject for the reason the data
        // gives: neither another reason nor an answer explains the case.
        for layer in [
            rejected(Rejection::Component(Component::Language)),
            ok("en-AX"),
            Err(LayerFailure::Engine),
        ] {
            assert_eq!(
                disposition(Some(&variant), "en-AX", &raw("en-AX"), &layer),
                Blocking
            );
        }
        let syntax = Outside::NotLocaleIdentifier;
        assert_eq!(
            disposition(
                Some(&syntax),
                "root",
                &Err(RawFailure::Unparsable),
                &rejected(Rejection::Syntax)
            ),
            OutsideDomain
        );
        assert_eq!(
            disposition(
                Some(&syntax),
                "root",
                &Err(RawFailure::Unparsable),
                &variant_rejected
            ),
            Blocking
        );
    }

    #[test]
    fn each_disposition_counts_in_its_own_column() {
        // CLDR 48.2.0's corpus has no extension, so no case reaches the
        // corrected column through the gate.
        let mut set = Counts::default();
        for disposition in [
            Disposition::Delegated,
            Disposition::Corrected,
            Disposition::Corrected,
            Disposition::OutsideDomain,
            Disposition::OutsideDomain,
            Disposition::OutsideDomain,
            Disposition::Blocking,
        ] {
            set.add(disposition);
        }
        assert_eq!(set, counts(1, 2, 3, 1));
        assert_eq!(set.cases(), 7);
        assert_eq!(Counts::sum(&[set, counts(4, 0, 1, 0)]), counts(5, 2, 4, 1));
        assert_eq!(
            Value::Object(set.json()),
            json!({"cases": "7", "delegated": "1", "corrected": "2", "outsideDomain": "3", "blocking": "1"})
        );
    }

    #[test]
    fn a_blocking_case_refuses_the_adapter() {
        let registry = GapRegistry::run(&explicit("en_US\t;\ten_GB\n"), &committed()).unwrap();
        assert!(!registry.admitted());
        assert_eq!(registry.counts(), counts(0, 0, 0, 1));
        let text: Value = serde_json::from_str(&registry.text()).unwrap();
        assert_eq!(text["outcome"]["blocking"], "1");
        assert_eq!(text["outcome"]["admitted"], false);
        assert_eq!(text["cases"][0]["disposition"], "blocking");
        assert_eq!(
            GapRegistry::run(
                &explicit("en_u_ca_gregory\t;\ten_u_ca_gregory\n"),
                &committed()
            ),
            Err(GateFailure::Extension(2))
        );
        assert_eq!(
            GapRegistry::run("# explicit\n", &committed()),
            Err(GateFailure::Corpus(CorpusFailure::MissingSets))
        );
    }

    #[test]
    fn the_registry_records_each_answer_and_reason() {
        let corpus = explicit("adp\t;\tdz\nen_aaland\t;\ten_AX\nroot\t;\troot\n");
        let registry = GapRegistry::run(&corpus, &committed()).unwrap();
        let text = registry.text();
        assert!(text.ends_with("\n]}\n"));
        let value: Value = serde_json::from_str(&text).unwrap();
        let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(
            value["kind"],
            "intlify-locale-canonicalization-gap-registry"
        );
        assert_eq!(value["schemaRevision"], "0");
        assert_eq!(
            value["generator"],
            json!({"identity": "intlify-locale-icu4x-corpus-gate", "revision": "0"})
        );
        assert_eq!(
            value["corpus"],
            json!({
                "cldr": {"revision": "48.2.0", "tag": "release-48-2"},
                "path": "common/testData/localeIdentifiers/localeCanonicalization.txt",
                "digest": FileDigest::of(corpus.as_bytes()).as_str(),
                "projection": {"identity": "cldr-subtags-joined-by-hyphens", "revision": "0"},
            })
        );
        assert_eq!(value["specification"], manifest["specification"]);
        assert_eq!(
            value["dataset"]["identity"],
            manifest["dataset"]["identity"]
        );
        assert_eq!(
            value["dataset"]["semanticDigest"],
            manifest["dataset"]["semanticDigest"]
        );
        assert_eq!(
            value["engine"],
            json!({"identity": "icu4x", "revision": "2.2.0"})
        );
        assert_eq!(value["artifactIntegrity"], manifest["integrityDigest"]);
        assert_eq!(
            value["outcome"],
            json!({"cases": "3", "delegated": "1", "corrected": "0", "outsideDomain": "2", "blocking": "0", "admitted": true})
        );
        assert_eq!(
            value["sets"][0],
            json!({"set": "explicit", "cases": "3", "delegated": "1", "corrected": "0", "outsideDomain": "2", "blocking": "0"})
        );
        assert_eq!(
            value["sets"][3],
            json!({"set": "withIrrelevants", "cases": "0", "delegated": "0", "corrected": "0", "outsideDomain": "0", "blocking": "0"})
        );
        assert_eq!(
            value["cases"],
            json!([
                {"case": "explicit/adp", "line": "2", "expected": "dz", "input": "adp", "normative": "dz",
                 "raw": {"canonical": "dz"}, "layer": {"canonical": "dz"}, "disposition": "delegated"},
                {"case": "explicit/en_aaland", "line": "3", "expected": "en_AX", "input": "en-aaland", "normative": "en-AX",
                 "raw": {"canonical": "en-AX"}, "layer": {"rejected": "variant"}, "disposition": "outside-domain",
                 "outside": {"reason": "component", "component": "variant", "code": "aaland", "status": "absent"}},
                {"case": "explicit/root", "line": "4", "expected": "root", "input": "root", "normative": "root",
                 "raw": "unparsable", "layer": {"rejected": "syntax"}, "disposition": "outside-domain",
                 "outside": {"reason": "not-a-locale-identifier"}},
            ])
        );
    }

    #[test]
    fn every_answer_and_status_has_its_own_name() {
        let rejections = [
            Rejection::Syntax,
            Rejection::RepeatedExtension,
            Rejection::OtherExtension,
            Rejection::PrivateUse,
            Rejection::Attribute,
            Rejection::RepeatedKey,
            Rejection::Component(Component::Language),
            Rejection::Component(Component::Script),
            Rejection::Component(Component::Region),
            Rejection::Component(Component::Variant),
            Rejection::Key,
            Rejection::Type,
            Rejection::Deprecated,
        ];
        let names: BTreeSet<_> = rejections
            .iter()
            .map(|&rejection| rejection_name(rejection))
            .collect();
        assert_eq!(names.len(), rejections.len());
        assert_eq!(
            names,
            BTreeSet::from([
                "syntax",
                "repeated-extension",
                "other-extension",
                "private-use",
                "attribute",
                "repeated-key",
                "language",
                "script",
                "region",
                "variant",
                "key",
                "type",
                "deprecated",
            ])
        );
        assert_eq!(layer_answer(&Err(LayerFailure::Engine)), json!("engine"));
        assert_eq!(
            layer_answer(&Err(LayerFailure::NotIdempotent)),
            json!("not-idempotent")
        );
        for (status, name) in [
            (None, "absent"),
            (Some(ValidityStatus::Reserved), "reserved"),
            (Some(ValidityStatus::PrivateUse), "private-use"),
        ] {
            let reason = outside_reason(&Outside::Component {
                component: Component::Script,
                code: "Qaaa".into(),
                status,
            });
            assert_eq!(reason["status"], name);
            assert_eq!(reason["component"], "script");
        }
    }

    #[test]
    fn the_corpus_is_read_at_the_release_the_artifact_was_generated_from() {
        let manifest: Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(manifest["generation"]["cldr"]["revision"], CORPUS_RELEASE.0);
        assert!(CORPUS.starts_with("# DO NOT EDIT THIS FILE"));
    }

    #[test]
    fn a_handwritten_case_never_overrides_what_the_corpus_expects() {
        // The handwritten cases supplement the corpus. One whose input the
        // corpus also has, ignoring ASCII case, expects what the corpus
        // expects, or rejects an input outside the domain.
        let handwritten: Value = serde_json::from_str(HANDWRITTEN).unwrap();
        let data = committed();
        let corpus = cases(CORPUS).unwrap();
        let mut met = Vec::new();
        for group in handwritten["groups"].as_array().unwrap() {
            for case in group["cases"].as_array().unwrap() {
                let input = case["input"].as_str().unwrap();
                let Some(upstream) = corpus
                    .iter()
                    .find(|upstream| project(upstream.source).eq_ignore_ascii_case(input))
                else {
                    continue;
                };
                match case["canonical"].as_str() {
                    Some(canonical) => {
                        assert_eq!(canonical, project(upstream.expected), "{input}");
                    }
                    None => assert!(
                        matches!(
                            domain(data.conformance(), input, upstream.line),
                            Ok(Some(_))
                        ),
                        "{input}"
                    ),
                }
                met.push(input);
            }
        }
        assert_eq!(met, ["und-Qaai"]);
    }
}
