// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The traceability report: one record for each subject the suite traces,
//! generated from the suite this crate writes, the registries, and the ID
//! inventories of design 015 (D15:3395-3444).
//!
//! A record follows 015's form: the subject's kind and ID, its classes, its
//! verification status, the reason, the verification owner, and the
//! evidence, and a decision's record also names the decision's class
//! (D15:3417-3442). `mapped` records name at least one case, vector,
//! obligation, or check as evidence; `not-applicable` records are inactive
//! conditional obligations and the Coverage inventory items their rows
//! name, with the obligation's reason; `owned-elsewhere` records are the
//! Handoff Vectors, which their owners verify (D15:3309).
//!
//! The report is committed beside the suite and outside the Suite Index
//! closure, and the suite generator's `--check` compares it with what this
//! module writes (D15:3415). Until every subject is mapped, a subject
//! without a record that follows those rules is listed among the report's
//! gaps instead, and the summary counts both for each kind; the suite is
//! sufficient only when no gap is left (D15:3413, 3444). The Workload
//! Profile's benchmark cases and phase/cost pairs are not inventoried yet:
//! the summary says so rather than counting them.
//!
//! This revision maps a subject only where the suite says so mechanically:
//! a manifest's `traceability` names it, a case expects it, or an
//! obligation's row names it. Mapping the rest is later work.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Value};

use crate::construction::failure::PRECEDENCE;
use crate::construction::generate;
use crate::construction::registry::check::CheckRegistry;
use crate::construction::registry::RegistryRole;
use crate::resolver::evaluation::Code;
use crate::resolver::origin::SafeOriginKind;

use super::case::BOUNDARIES;
use super::index::SuiteFile;

/// Where the report is written below the suite root.
pub const PATH: &str = "traceability-v0.json";

/// The kinds of subject, in the order 015 lists them (D15:3431), with the
/// two benchmark kinds, which are not inventoried yet, last.
const KINDS: [&str; 17] = [
    "decision",
    "check",
    "construction-rule",
    "input-admission-rule",
    "construction-reason",
    "boundary-kind",
    "safe-origin-kind",
    "finding-code",
    "finding-reason",
    "registry-member",
    "case",
    "handoff-vector",
    "conditional-obligation",
    "relation",
    "coverage-item",
    "benchmark-pair",
    "benchmark-case",
];

/// The kinds whose subjects are not inventoried yet.
const NOT_INVENTORIED: [&str; 2] = ["benchmark-pair", "benchmark-case"];

/// The verifier this crate is.
const SELF: &str = "intlify_config";

/// The ID inventories of design 015 the report traces: every Accepted
/// Decision with its class, every Coverage inventory subject, an item split
/// into the parts the obligation table names, and each obligation row's
/// Coverage inventory subjects.
#[derive(Debug)]
pub(crate) struct Inventory {
    decisions: Vec<(String, &'static str)>,
    coverage: Vec<String>,
    named: BTreeMap<String, Vec<String>>,
}

/// The ID that starts `text` when it is `015-` and three digits.
fn decision_at(text: &str) -> Option<&str> {
    let id = text.get(..7)?;
    (id.starts_with("015-") && id[4..].bytes().all(|byte| byte.is_ascii_digit())).then_some(id)
}

/// Every Coverage inventory reference in `text`, `015-COV-NNN` with its
/// `/<owner>` part when it has one, in order.
fn coverage_references(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (at, _) in text.match_indices("015-COV-") {
        let rest = &text[at..];
        let digits = rest[8..].bytes().take_while(u8::is_ascii_digit).count();
        if digits != 3 {
            continue;
        }
        let mut end = 11;
        if rest.as_bytes().get(11) == Some(&b'/') {
            let part = rest[12..]
                .bytes()
                .take_while(|byte| byte.is_ascii_alphanumeric() || *byte == b'-')
                .count();
            if part > 0 {
                end = 12 + part;
            }
        }
        found.push(rest[..end].to_owned());
    }
    found
}

impl Inventory {
    /// Read the inventories from the text of design 015: the Decision Log's
    /// rows, the decision-class table, the Coverage inventory's items, and
    /// the conditional obligation table's rows.
    pub(crate) fn read(design: &str) -> Self {
        let mut classes = BTreeMap::new();
        let mut decisions = Vec::new();
        let mut items = Vec::new();
        let mut named = BTreeMap::new();
        for line in design.lines() {
            for class in ["implementation-measurement", "non-resolver"] {
                if let Some(rest) = line.strip_prefix(&format!("| {class} | ")) {
                    for id in rest.trim_end_matches(" |").split(", ") {
                        classes.insert(id.to_owned(), class);
                    }
                }
            }
            if let Some(id) = line.strip_prefix("| ").and_then(decision_at) {
                if line[2 + id.len()..].starts_with(" |") {
                    decisions.push(id.to_owned());
                }
            }
            if let Some(rest) = line.strip_prefix("- `015-COV-") {
                if rest.len() > 4 && rest.as_bytes()[3] == b'`' {
                    items.push(format!("015-COV-{}", &rest[..3]));
                }
            }
            if let Some(rest) = line.strip_prefix("| `015-") {
                let id = rest.split('`').next().unwrap_or_default();
                if ["XV-", "AD-", "HV-"]
                    .iter()
                    .any(|prefix| id.starts_with(prefix))
                {
                    named.insert(format!("015-{id}"), coverage_references(line));
                }
            }
        }
        // An item with parts the obligation table names is traced by those
        // parts instead of as a whole.
        let parts = named
            .values()
            .flatten()
            .filter(|reference| reference.contains('/'))
            .cloned()
            .collect::<BTreeSet<_>>();
        let mut coverage = Vec::new();
        for item in items {
            let split = parts
                .iter()
                .filter(|part| part.starts_with(&format!("{item}/")))
                .cloned()
                .collect::<Vec<_>>();
            if split.is_empty() {
                coverage.push(item);
            } else {
                coverage.extend(split);
            }
        }
        let decisions = decisions
            .into_iter()
            .map(|id| {
                let class = classes.get(&id).copied().unwrap_or("resolver-semantic");
                (id, class)
            })
            .collect();
        Self {
            decisions,
            coverage,
            named,
        }
    }
}

/// A record or a gap for one subject.
struct Traced {
    kind: &'static str,
    id: String,
    record: Option<Value>,
}

/// A `mapped` record, or a gap without evidence.
fn mapped(kind: &'static str, id: String, classes: &[&str], evidence: BTreeSet<String>) -> Traced {
    let record = (!evidence.is_empty()).then(|| {
        json!({
            "subject": { "kind": kind, "id": id },
            "classes": classes,
            "verificationStatus": "mapped",
            "reason": null,
            "verificationOwner": SELF,
            "evidence": evidence.into_iter().collect::<Vec<_>>(),
        })
    });
    Traced { kind, id, record }
}

/// The suite's manifests as this crate writes them, each with its kind's
/// array and its parsed body, and every other file's bytes by path.
struct Suite {
    manifests: Vec<(&'static str, Value)>,
    files: BTreeMap<String, Vec<u8>>,
}

/// The Suite Index's manifest arrays with the class of each.
const ARRAYS: [(&str, &str); 5] = [
    ("resolverConstructionCases", "resolver-construction"),
    ("resolverInputAdmissionCases", "resolver-input-admission"),
    ("resolverCases", "resolver"),
    ("handoffVectors", "handoff-vector"),
    ("conditionalObligations", "conditional-obligation"),
];

impl Suite {
    fn read(files: &[SuiteFile]) -> Result<Self, serde_json::Error> {
        let files = files
            .iter()
            .map(|file| (file.path.clone(), file.bytes.clone()))
            .collect::<BTreeMap<_, _>>();
        let index: Value = serde_json::from_slice(&files[super::index::PATH])?;
        let mut manifests = Vec::new();
        for (array, _) in ARRAYS {
            for entry in index[array].as_array().into_iter().flatten() {
                let path = entry["path"].as_str().unwrap_or_default();
                manifests.push((array, serde_json::from_slice(&files[path])?));
            }
        }
        Ok(Self { manifests, files })
    }

    /// The manifests of one array.
    fn of<'a>(&'a self, array: &'a str) -> impl Iterator<Item = &'a Value> {
        self.manifests
            .iter()
            .filter(move |(of, _)| *of == array)
            .map(|(_, manifest)| manifest)
    }

    /// The IDs of the manifests whose `traceability` lists `id` under
    /// `member`.
    fn tracing(&self, member: &str, id: &str) -> BTreeSet<String> {
        self.manifests
            .iter()
            .filter(|(_, manifest)| {
                manifest["traceability"][member]
                    .as_array()
                    .is_some_and(|ids| ids.iter().any(|named| named == id))
            })
            .map(|(_, manifest)| manifest["id"].as_str().unwrap_or_default().to_owned())
            .collect()
    }

    /// The bytes of every fixture a Resolver Case or admission case
    /// references, with the case's ID.
    fn fixtures(&self) -> Vec<(String, &[u8])> {
        let mut found = Vec::new();
        for (array, manifest) in &self.manifests {
            if *array != "resolverCases" && *array != "resolverInputAdmissionCases" {
                continue;
            }
            let id = manifest["id"].as_str().unwrap_or_default();
            let mut paths = Vec::new();
            paths_in(manifest, &mut paths);
            for path in paths {
                if let Some(bytes) = self.files.get(&path) {
                    found.push((id.to_owned(), bytes.as_slice()));
                }
            }
        }
        found
    }
}

/// Every `path` member's string value at any depth of `value`.
fn paths_in(value: &Value, paths: &mut Vec<String>) {
    match value {
        Value::Object(members) => {
            for (name, member) in members {
                match member.as_str() {
                    Some(path) if name == "path" => paths.push(path.to_owned()),
                    _ => paths_in(member, paths),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| paths_in(item, paths)),
        _ => {}
    }
}

/// Every object at any depth of `value` that has a `code`, as (code,
/// reason) with the reason when it has one.
fn findings_in(value: &Value, found: &mut BTreeSet<(String, Option<String>)>) {
    match value {
        Value::Object(members) => {
            if let Some(code) = members.get("code").and_then(Value::as_str) {
                let reason = members.get("reason").and_then(Value::as_str);
                found.insert((code.to_owned(), reason.map(str::to_owned)));
            }
            members
                .values()
                .for_each(|member| findings_in(member, found));
        }
        Value::Array(items) => items.iter().for_each(|item| findings_in(item, found)),
        _ => {}
    }
}

/// The report for the suite `files` this crate writes and the text of
/// design 015.
///
/// # Errors
///
/// Returns the failure to parse a file of the suite, which this crate
/// writes, or to serialize the report.
pub fn report(files: &[SuiteFile], design: &str) -> Result<SuiteFile, serde_json::Error> {
    let suite = Suite::read(files)?;
    let inventory = Inventory::read(design);
    let mut traced = Vec::new();
    decisions(&suite, &inventory, &mut traced);
    checks(&suite, &mut traced);
    construction(&suite, &mut traced)?;
    admission(&suite, &mut traced);
    origins(&suite, &mut traced);
    findings(&suite, &mut traced)?;
    registry(&suite, &mut traced);
    manifests(&suite, &mut traced);
    relations(&suite, &inventory, &mut traced);
    written(traced)
}

/// The report's file: the summary of each kind, the records, and the gaps,
/// each subject in kind order, then by the byte order of its ID.
fn written(mut traced: Vec<Traced>) -> Result<SuiteFile, serde_json::Error> {
    let rank = |kind: &str| {
        KINDS
            .iter()
            .position(|of| *of == kind)
            .unwrap_or(KINDS.len())
    };
    traced.sort_by(|a, b| {
        rank(a.kind)
            .cmp(&rank(b.kind))
            .then_with(|| a.id.as_bytes().cmp(b.id.as_bytes()))
    });
    let summary = KINDS
        .iter()
        .map(|kind| {
            if NOT_INVENTORIED.contains(kind) {
                return json!({ "kind": kind, "inventoried": false });
            }
            let of = traced
                .iter()
                .filter(|subject| subject.kind == *kind)
                .collect::<Vec<_>>();
            let status = |status: &str| {
                of.iter()
                    .filter(|subject| {
                        subject
                            .record
                            .as_ref()
                            .is_some_and(|record| record["verificationStatus"] == status)
                    })
                    .count()
            };
            json!({
                "kind": kind,
                "inventoried": true,
                "subjects": of.len(),
                "mapped": status("mapped"),
                "notApplicable": status("not-applicable"),
                "ownedElsewhere": status("owned-elsewhere"),
                "gaps": of.iter().filter(|subject| subject.record.is_none()).count(),
            })
        })
        .collect::<Vec<_>>();
    let gaps = traced
        .iter()
        .filter(|subject| subject.record.is_none())
        .map(|subject| json!({ "kind": subject.kind, "id": subject.id }))
        .collect::<Vec<_>>();
    let records = traced
        .into_iter()
        .filter_map(|subject| subject.record)
        .collect::<Vec<_>>();
    Ok(SuiteFile {
        path: PATH.to_owned(),
        bytes: generate::write(&json!({
            "reportRevision": "0",
            "suiteRevision": super::case::REVISION,
            "summary": summary,
            "records": records,
            "gaps": gaps,
        }))?,
    })
}

/// Accepted Decisions, traced by every manifest that names them.
fn decisions(suite: &Suite, inventory: &Inventory, traced: &mut Vec<Traced>) {
    for (id, class) in &inventory.decisions {
        let mut subject = mapped(
            "decision",
            id.clone(),
            &["resolver"],
            suite.tracing("decisionIds", id),
        );
        if let Some(record) = &mut subject.record {
            let classes = suite
                .manifests
                .iter()
                .filter(|(_, manifest)| subject_named(manifest, "decisionIds", id))
                .map(|(array, _)| class_of(array))
                .collect::<BTreeSet<_>>();
            record["classes"] = json!(classes);
            record["decisionClass"] = json!(class);
        }
        traced.push(subject);
    }
}

/// Checks, through the Check Registry.
fn checks(suite: &Suite, traced: &mut Vec<Traced>) {
    for row in CheckRegistry::revision_zero().rows {
        let evidence = suite.tracing("checkIds", &row.check_id);
        traced.push(mapped("check", row.check_id, &["resolver"], evidence));
    }
}

/// Construction rules and reasons, from the construction precedence.
fn construction(suite: &Suite, traced: &mut Vec<Traced>) -> Result<(), serde_json::Error> {
    let mut reasons = BTreeSet::new();
    for row in PRECEDENCE {
        let rule = super::construction_cases::rule(row)?;
        traced.push(mapped(
            "construction-rule",
            rule.clone(),
            &["resolver-construction"],
            suite.tracing("ruleIds", &rule),
        ));
        reasons.insert(
            serde_json::to_value(row.reason)?
                .as_str()
                .unwrap_or_default()
                .to_owned(),
        );
    }
    for reason in reasons {
        let evidence = suite
            .of("resolverConstructionCases")
            .filter(|manifest| manifest["expected"]["failureReason"] == reason.as_str())
            .map(|manifest| manifest["id"].as_str().unwrap_or_default().to_owned())
            .collect();
        traced.push(mapped(
            "construction-reason",
            reason,
            &["resolver-construction"],
            evidence,
        ));
    }
    Ok(())
}

/// Input admission rules and boundary kinds, from the boundary table; a
/// kind is covered by an accepted case and a rejected one.
fn admission(suite: &Suite, traced: &mut Vec<Traced>) {
    for (kind, kind_reasons) in BOUNDARIES {
        for reason in kind_reasons {
            let rule = super::admission_cases::rule(kind, reason);
            traced.push(mapped(
                "input-admission-rule",
                rule.clone(),
                &["resolver-input-admission"],
                suite.tracing("ruleIds", &rule),
            ));
        }
        let cases = suite
            .of("resolverInputAdmissionCases")
            .filter(|manifest| manifest["boundaryKind"] == kind)
            .collect::<Vec<_>>();
        // A kind is covered by an accepted case and a rejected one.
        let accepted = cases
            .iter()
            .any(|manifest| manifest["expected"]["admitted"] == true);
        let rejected = cases
            .iter()
            .any(|manifest| manifest["expected"]["admitted"] == false);
        let evidence = if accepted && rejected {
            cases
                .iter()
                .map(|manifest| manifest["id"].as_str().unwrap_or_default().to_owned())
                .collect()
        } else {
            BTreeSet::new()
        };
        traced.push(mapped(
            "boundary-kind",
            (*kind).to_owned(),
            &["resolver-input-admission"],
            evidence,
        ));
    }
}

/// Safe Origin Kinds, by the cases whose fixtures carry one.
fn origins(suite: &Suite, traced: &mut Vec<Traced>) {
    let fixtures = suite.fixtures();
    for kind in SafeOriginKind::ALL {
        let needle = format!("\"{}\"", kind.id());
        let evidence = fixtures
            .iter()
            .filter(|(_, bytes)| contains(bytes, needle.as_bytes()))
            .map(|(id, _)| id.clone())
            .collect();
        traced.push(mapped(
            "safe-origin-kind",
            kind.id().to_owned(),
            &["resolver"],
            evidence,
        ));
    }
}

/// Finding codes and reasons, by the Resolver Cases that expect them.
fn findings(suite: &Suite, traced: &mut Vec<Traced>) -> Result<(), serde_json::Error> {
    let mut expected = BTreeMap::<(String, Option<String>), BTreeSet<String>>::new();
    for manifest in suite.of("resolverCases") {
        let id = manifest["id"].as_str().unwrap_or_default();
        let mut paths = Vec::new();
        paths_in(&manifest["entries"], &mut paths);
        for path in paths.iter().filter(|path| path.ends_with("-expected.json")) {
            let mut found = BTreeSet::new();
            findings_in(&serde_json::from_slice(&suite.files[path])?, &mut found);
            for finding in found {
                expected.entry(finding).or_default().insert(id.to_owned());
            }
        }
    }
    for code in Code::ALL {
        let evidence = expected
            .iter()
            .filter(|((found, _), _)| found == code.as_str())
            .flat_map(|(_, cases)| cases.iter().cloned())
            .collect();
        traced.push(mapped(
            "finding-code",
            code.as_str().to_owned(),
            &["resolver"],
            evidence,
        ));
        for reason in code.reasons() {
            let key = (code.as_str().to_owned(), Some(reason.as_str().to_owned()));
            traced.push(mapped(
                "finding-reason",
                format!("{}/{}", code.as_str(), reason.as_str()),
                &["resolver"],
                expected.get(&key).cloned().unwrap_or_default(),
            ));
        }
    }
    Ok(())
}

/// Registry members, by the construction cases that construct with them.
fn registry(suite: &Suite, traced: &mut Vec<Traced>) {
    let constructing = suite
        .of("resolverConstructionCases")
        .filter(|manifest| manifest["expected"]["constructed"] == true)
        .map(|manifest| manifest["id"].as_str().unwrap_or_default().to_owned())
        .collect::<BTreeSet<_>>();
    for role in RegistryRole::ALL {
        traced.push(mapped(
            "registry-member",
            role.name().to_owned(),
            &["resolver-construction"],
            constructing.clone(),
        ));
    }
}

/// Cases, by what their traceability names; vectors, which their owners
/// verify; and obligations, not applicable while inactive.
fn manifests(suite: &Suite, traced: &mut Vec<Traced>) {
    for (array, manifest) in &suite.manifests {
        let id = manifest["id"].as_str().unwrap_or_default().to_owned();
        match *array {
            "handoffVectors" => traced.push(Traced {
                kind: "handoff-vector",
                id: id.clone(),
                record: Some(json!({
                    "subject": { "kind": "handoff-vector", "id": id },
                    "classes": ["handoff-vector"],
                    "verificationStatus": "owned-elsewhere",
                    "reason": "verified-by-owner",
                    "verificationOwner": manifest["verificationOwner"],
                    "evidence": [id],
                })),
            }),
            "conditionalObligations" => {
                let inactive = manifest["activationState"] == "inactive";
                traced.push(Traced {
                    kind: "conditional-obligation",
                    id: id.clone(),
                    record: inactive.then(|| {
                        json!({
                            "subject": { "kind": "conditional-obligation", "id": id },
                            "classes": ["conditional-obligation"],
                            "verificationStatus": "not-applicable",
                            "reason": manifest["notApplicableReason"],
                            "verificationOwner": manifest["verificationOwner"],
                            "evidence": [id],
                        })
                    }),
                });
            }
            _ => {
                let mut evidence = BTreeSet::new();
                for member in ["decisionIds", "ruleIds", "checkIds"] {
                    for named in manifest["traceability"][member]
                        .as_array()
                        .into_iter()
                        .flatten()
                    {
                        evidence.insert(named.as_str().unwrap_or_default().to_owned());
                    }
                }
                traced.push(mapped("case", id, &[class_of(array)], evidence));
            }
        }
    }
}

/// Relations, each traced by the case that declares it, and the Coverage
/// inventory subjects: not applicable where an inactive obligation's row
/// names them, mapped where a relation covers them.
fn relations(suite: &Suite, inventory: &Inventory, traced: &mut Vec<Traced>) {
    let mut covered = BTreeMap::<String, BTreeSet<String>>::new();
    for manifest in suite.of("resolverCases") {
        let id = manifest["id"].as_str().unwrap_or_default();
        for relation in manifest["relations"].as_array().into_iter().flatten() {
            let subject = format!(
                "{id}/{}/{}",
                relation["kind"].as_str().unwrap_or_default(),
                relation["case"].as_str().unwrap_or_default()
            );
            traced.push(mapped(
                "relation",
                subject,
                &["resolver"],
                BTreeSet::from([id.to_owned()]),
            ));
            if let Some(class) = relation["class"].as_str() {
                covered
                    .entry(class.to_owned())
                    .or_default()
                    .insert(id.to_owned());
            }
        }
    }
    // Coverage inventory subjects: not applicable where an inactive
    // obligation's row names them, mapped where a relation covers them.
    let obligations = suite
        .of("conditionalObligations")
        .map(|manifest| (manifest["id"].as_str().unwrap_or_default(), manifest))
        .collect::<BTreeMap<_, _>>();
    for subject in &inventory.coverage {
        let naming = inventory
            .named
            .iter()
            .find(|(_, named)| named.contains(subject))
            .and_then(|(row, _)| obligations.get(row.as_str()));
        let record = match naming {
            Some(obligation) if obligation["activationState"] == "inactive" => Some(json!({
                "subject": { "kind": "coverage-item", "id": subject },
                "classes": ["conditional-obligation"],
                "verificationStatus": "not-applicable",
                "reason": obligation["notApplicableReason"],
                "verificationOwner": obligation["verificationOwner"],
                "evidence": [obligation["id"]],
            })),
            _ => {
                mapped(
                    "coverage-item",
                    subject.clone(),
                    &["resolver"],
                    covered.get(subject).cloned().unwrap_or_default(),
                )
                .record
            }
        };
        traced.push(Traced {
            kind: "coverage-item",
            id: subject.clone(),
            record,
        });
    }
}

/// Whether `manifest`'s `traceability` lists `id` under `member`.
fn subject_named(manifest: &Value, member: &str, id: &str) -> bool {
    manifest["traceability"][member]
        .as_array()
        .is_some_and(|ids| ids.iter().any(|named| named == id))
}

/// The class of the subjects a manifest array holds.
fn class_of(array: &str) -> &'static str {
    ARRAYS
        .iter()
        .find(|(of, _)| *of == array)
        .map_or("resolver", |(_, class)| class)
}

/// Whether `haystack` holds `needle`.
fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The text of design 015 in this repository.
    fn design() -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../design/015-intlify-project-profile-and-locale-policy-design.md");
        std::fs::read_to_string(path).unwrap()
    }

    /// The report this crate writes.
    fn bytes() -> &'static [u8] {
        static REPORT: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        REPORT.get_or_init(|| {
            let files = super::super::index::artifacts().unwrap();
            report(&files, &design()).unwrap().bytes
        })
    }

    /// The report this crate writes, parsed.
    fn written() -> Value {
        serde_json::from_slice(bytes()).unwrap()
    }

    #[test]
    fn the_committed_report_is_the_one_this_crate_writes() {
        assert!(
            include_bytes!("../../conformance/traceability-v0.json") == bytes(),
            "rerun generate_conformance_suite --write"
        );
    }

    /// A suite of these manifests and files alone.
    fn suite_of(manifests: Vec<(&'static str, Value)>, files: &[(&str, &str)]) -> Suite {
        Suite {
            manifests,
            files: files
                .iter()
                .map(|(path, text)| ((*path).to_owned(), text.as_bytes().to_vec()))
                .collect(),
        }
    }

    #[test]
    fn a_boundary_kind_needs_an_admitted_and_a_rejected_case() {
        let (kind, _) = BOUNDARIES[0];
        let case = |id: &str, admitted: bool| {
            (
                "resolverInputAdmissionCases",
                json!({ "id": id, "boundaryKind": kind, "expected": { "admitted": admitted } }),
            )
        };
        let traced_kind = |manifests| {
            let mut traced = Vec::new();
            admission(&suite_of(manifests, &[]), &mut traced);
            traced
                .into_iter()
                .find(|subject| subject.kind == "boundary-kind" && subject.id == kind)
                .unwrap()
                .record
        };
        assert!(traced_kind(vec![case("only-rejected", false)]).is_none());
        assert!(traced_kind(vec![case("only-admitted", true)]).is_none());
        let both = traced_kind(vec![case("admitted", true), case("rejected", false)]).unwrap();
        assert_eq!(both["evidence"], json!(["admitted", "rejected"]));
    }

    #[test]
    fn a_safe_origin_kind_is_found_only_as_a_whole_json_string() {
        let kind = SafeOriginKind::ALL[0].id();
        let traced_kind = |text: String| {
            let manifests = vec![(
                "resolverCases",
                json!({ "id": "case", "fixture": { "path": "fixture.json" } }),
            )];
            let mut traced = Vec::new();
            origins(
                &suite_of(manifests, &[("fixture.json", &text)]),
                &mut traced,
            );
            traced
                .into_iter()
                .find(|subject| subject.kind == "safe-origin-kind" && subject.id == kind)
                .unwrap()
                .record
        };
        assert!(traced_kind(format!("{{\"origin\":\"not-{kind}\"}}")).is_none());
        assert!(traced_kind(format!("{{\"origin\":\"{kind}\"}}")).is_some());
    }

    #[test]
    fn the_inventories_hold_every_decision_and_coverage_item_of_015() {
        let inventory = Inventory::read(&design());
        assert_eq!(inventory.decisions.len(), 217);
        let classes = inventory.decisions.iter().fold(
            BTreeMap::<&str, usize>::new(),
            |mut counts, (_, class)| {
                *counts.entry(class).or_default() += 1;
                counts
            },
        );
        assert_eq!(
            classes,
            BTreeMap::from([
                ("implementation-measurement", 6),
                ("non-resolver", 17),
                ("resolver-semantic", 194),
            ])
        );
        // 347 items, 18 of them split into the 43 parts the obligation
        // table names.
        let parts = inventory
            .coverage
            .iter()
            .filter(|id| id.contains('/'))
            .count();
        assert_eq!(parts, 43);
        assert_eq!(inventory.coverage.len(), 347 - 18 + 43);
        assert_eq!(inventory.named.len(), 19);
        assert_eq!(
            coverage_references("`015-COV-219/020`, `015-COV-231`, 015-COV-12"),
            ["015-COV-219/020", "015-COV-231"]
        );
    }

    #[test]
    fn every_subject_has_one_record_or_one_gap_and_each_record_follows_015() {
        let report = &written();
        let records = report["records"].as_array().unwrap();
        let gaps = report["gaps"].as_array().unwrap();
        let mut seen = BTreeSet::new();
        for record in records {
            let subject = &record["subject"];
            assert!(seen.insert((
                subject["kind"].clone().to_string(),
                subject["id"].to_string()
            )));
            let evidence = record["evidence"].as_array().unwrap();
            assert!(!evidence.is_empty(), "{subject}");
            assert!(
                !record["classes"].as_array().unwrap().is_empty(),
                "{subject}"
            );
            match record["verificationStatus"].as_str().unwrap() {
                "mapped" => {
                    assert!(record["reason"].is_null());
                    assert_eq!(record["verificationOwner"], SELF);
                }
                "not-applicable" => assert!(
                    ["conditional-obligation", "coverage-item"]
                        .contains(&subject["kind"].as_str().unwrap()),
                    "{subject}"
                ),
                "owned-elsewhere" => {
                    assert_eq!(record["reason"], "verified-by-owner");
                    assert_ne!(record["verificationOwner"], SELF);
                }
                other => panic!("{other}"),
            }
            assert_eq!(
                record.get("decisionClass").is_some(),
                subject["kind"] == "decision"
            );
        }
        for gap in gaps {
            assert!(seen.insert((gap["kind"].clone().to_string(), gap["id"].to_string())));
        }
        // The summary counts each kind's records and gaps.
        for row in report["summary"].as_array().unwrap() {
            let kind = &row["kind"];
            if row["inventoried"] == false {
                assert!(records
                    .iter()
                    .all(|record| record["subject"]["kind"] != *kind));
                continue;
            }
            let records = records
                .iter()
                .filter(|record| record["subject"]["kind"] == *kind)
                .count();
            let gaps = gaps.iter().filter(|gap| gap["kind"] == *kind).count();
            assert_eq!(row["subjects"], json!(records + gaps), "{kind}");
            assert_eq!(row["gaps"], json!(gaps), "{kind}");
        }
    }

    #[test]
    fn the_suite_s_own_subjects_are_traced_by_its_manifests() {
        let report = &written();
        let summary = |kind: &str| {
            report["summary"]
                .as_array()
                .unwrap()
                .iter()
                .find(|row| row["kind"] == kind)
                .unwrap()
                .clone()
        };
        // Every vector is its owner's and every obligation is inactive.
        assert_eq!(summary("handoff-vector")["ownedElsewhere"], 87);
        assert_eq!(summary("conditional-obligation")["notApplicable"], 19);
        // Every construction rule and reason, boundary kind, and admission
        // rule has its case.
        for kind in [
            "construction-rule",
            "construction-reason",
            "boundary-kind",
            "input-admission-rule",
            "registry-member",
            "relation",
        ] {
            assert_eq!(summary(kind)["gaps"], 0, "{kind}");
        }
        assert_eq!(summary("construction-rule")["subjects"], 43);
        assert_eq!(summary("construction-reason")["subjects"], 38);
        assert_eq!(summary("boundary-kind")["subjects"], 13);
        assert_eq!(summary("input-admission-rule")["subjects"], 78);
        assert_eq!(summary("check")["subjects"], 191);
        assert_eq!(summary("finding-code")["subjects"], 48);
        assert_eq!(summary("finding-reason")["subjects"], 66);
    }
}
