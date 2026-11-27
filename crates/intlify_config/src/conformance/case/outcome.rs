// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The closed form of a Resolver Case's expected outcome (D15:3271-3303):
//! its outcome, Resolver Construction Identity, profile view, exact Resolved
//! Profile Artifact Binding keys, and Resolution Evidence, whose two
//! projections list each record with its Evidence record owner's Unit view.
//!
//! Each record has its view of 015's Finding, Evidence, and accounting
//! encoding (D15:2451-2519). A Finding's code fixes its severity, whether it
//! blocks, its suggestion, its `details` members, and the reasons it states
//! (D15:2230, 2232, 2309), so the Finding and Finding Key views hold one
//! branch per code, generated from the Check and Finding Registries. The
//! profile view holds its twelve groups (D15:1214-1237), its policy section
//! generated from the section the resolver writes.

use serde_json::{json, Map, Value};

use crate::boundary::locator::LocatorVariant;
use crate::construction::envelope::{closed, digest, token};
use crate::construction::registry::check::{details_members, DetailsMember, CHECK_ID_PATTERN};
use crate::construction::registry::finding::{
    blocking, severity, suggestion_kind, FindingRegistry, SuggestionKind,
};
use crate::construction::registry::RANK_PATTERN;
use crate::model::CoverageMode;
use crate::resolver::dependency::{MaterializedSlot, SemanticGroup};
use crate::resolver::evaluation::{Code, Phase};
use crate::resolver::limits::{fixture_limits, Group};
use crate::resolver::origin::{EvidenceKind, SafeOriginKind};
use crate::resolver::specification::Slot;
use crate::resolver::subject::SubjectKind;

use super::{closed_with, constant, definition, of, pattern, strings, HEX_PATTERN};

/// The JSON type tags a redacted value names (D15:2469).
const JSON_TYPES: [&str; 6] = ["null", "boolean", "number", "string", "array", "object"];

/// A bound a Finding's `details` names: a section bound, or a capability
/// bound by its namespace and path (D15:2232).
const BOUND_PATTERN: &str = "^[a-z][A-Za-z0-9]*(?:\\.[a-z][A-Za-z0-9]*)*$";

/// The definitions of the expected-outcome fixture: its record views, the
/// profile view, and the fixture itself.
pub(super) fn outcome(definitions: &mut Map<String, Value>) {
    views(definitions);
    evidence(definitions);
    profile(definitions);
    fixture(definitions);
}

fn text() -> Value {
    json!({ "type": "string", "minLength": 1 })
}

fn count() -> Value {
    pattern(RANK_PATTERN)
}

fn array(items: Value) -> Value {
    let mut array = json!({ "type": "array", "items": null });
    array["items"] = items;
    array
}

fn nullable(schema: Value) -> Value {
    let mut nullable = json!({ "oneOf": [null, { "type": "null" }] });
    nullable["oneOf"][0] = schema;
    nullable
}

/// An object whose every member, of any name, has the form `members`.
fn map(members: Value) -> Value {
    let mut map = json!({ "type": "object", "additionalProperties": null });
    map["additionalProperties"] = members;
    map
}

/// The views every record kind shares: the token, the Unit, the Finding Key
/// and Finding of each code, and the records that name them (D15:2471-2519).
fn views(definitions: &mut Map<String, Value>) {
    definitions.insert("phaseId".into(), strings(Phase::ALL.map(Phase::id)));
    definitions.insert(
        "versionedIdentity".into(),
        closed(&[("identity", token()), ("revision", token())]),
    );
    definitions.insert(
        "tokenView".into(),
        closed_with(&[("token", digest())], &[("collision", count())]),
    );
    definitions.insert(
        "unitView".into(),
        closed(&[
            ("phase", definition("phaseId")),
            ("subjectKind", of::<SubjectKind>()),
            ("subject", definition("tokenView")),
            ("check", pattern(CHECK_ID_PATTERN)),
        ]),
    );
    let registry = FindingRegistry::revision_zero();
    let checks = |code: Code| -> Vec<String> {
        registry
            .rows
            .iter()
            .find(|row| row.code == code)
            .map(|row| row.allowed_check_ids.clone())
            .unwrap_or_default()
    };
    for code in Code::ALL {
        definitions.insert(key_definition(code), finding_key(code, &checks(code)));
    }
    definitions.insert(
        "findingKeyView".into(),
        json!({ "oneOf": Code::ALL.map(|code| definition(&key_definition(code))).to_vec() }),
    );
    definitions.insert(
        "findingView".into(),
        json!({ "oneOf": Code::ALL.map(finding).to_vec() }),
    );
    let unit = || definition("unitView");
    let key = || definition("findingKeyView");
    definitions.insert(
        "evaluationStatusView".into(),
        json!({
            "oneOf": [
                closed(&[("unit", unit()), ("state", constant("evaluated"))]),
                closed(&[
                    ("unit", unit()),
                    ("state", constant("not-evaluated")),
                    ("causes", array(key())),
                ]),
            ]
        }),
    );
    definitions.insert(
        "sharedUnavailabilityCauseView".into(),
        closed(&[("cause", key())]),
    );
    definitions.insert(
        "terminalStateView".into(),
        closed(&[
            ("domain", strings(["entry", "shared"])),
            ("nextUnitCursor", unit()),
            ("cause", key()),
        ]),
    );
    let cell = closed(&[("locale", text()), ("surfaceClass", text())]);
    definitions.insert(
        "ruleDomainView".into(),
        closed(&[
            ("locales", array(text())),
            ("surfaceClasses", array(text())),
        ]),
    );
    definitions.insert(
        "coverageDecisionBasisView".into(),
        closed(&[
            ("cell", cell.clone()),
            ("mode", of::<CoverageMode>()),
            ("decision", strings(["default", "rule"])),
            ("domains", array(definition("ruleDomainView"))),
        ]),
    );
    definitions.insert(
        "coverageDecisionEvidenceView".into(),
        closed(&[
            ("cell", cell),
            (
                "source",
                json!({ "oneOf": [constant("default"), definition("ruleDomainView")] }),
            ),
            ("evidence", definition("evidenceView")),
        ]),
    );
    definitions.insert(
        "findingOccurrenceView".into(),
        closed(&[
            ("finding", key()),
            ("primary", definition("evidenceView")),
            ("ordinal", count()),
            ("related", array(definition("evidenceView"))),
        ]),
    );
    dependencies(definitions);
}

/// The definition of `code`'s Finding Key, which both the Finding Key and
/// the Finding views name.
fn key_definition(code: Code) -> String {
    format!("findingKey:{}", code.as_str())
}

/// The Finding Key of `code`, which the checks `checks` may report: its
/// stable reason when the code states one (D15:2232, 2380).
fn finding_key(code: Code, checks: &[String]) -> Value {
    let mut members = vec![
        ("specification", definition("versionedIdentity")),
        ("phase", definition("phaseId")),
        ("check", strings(checks.iter().map(String::as_str))),
        ("code", constant(code.as_str())),
        ("subject", definition("tokenView")),
    ];
    if !code.reasons().is_empty() {
        members.push(("reason", reasons(code)));
    }
    closed(&members)
}

fn reasons(code: Code) -> Value {
    strings(code.reasons().iter().map(|reason| reason.as_str()))
}

/// The Finding of `code`: its key, the severity and blocking flag the code
/// fixes, its `details` when the code has members, and its suggestion when
/// it suggests one (D15:2230, 2309).
fn finding(code: Code) -> Value {
    let mut members = vec![
        ("key", definition(&key_definition(code))),
        ("severity", json!({ "const": severity(code) })),
        ("blocking", json!({ "const": blocking(code) })),
    ];
    let named = details_members(code);
    if !named.is_empty() {
        let names: Vec<String> = named.iter().map(|member| member_name(*member)).collect();
        let details: Vec<(&str, Value)> = names
            .iter()
            .zip(named)
            .map(|(name, member)| (name.as_str(), detail(code, *member)))
            .collect();
        members.push(("details", closed(&details)));
    }
    if suggestion_kind(code) == SuggestionKind::CanonicalLocaleReplacement {
        members.push((
            "suggestion",
            closed(&[("canonical-locale-replacement", text())]),
        ));
    }
    closed(&members)
}

fn member_name(member: DetailsMember) -> String {
    match serde_json::to_value(member) {
        Ok(Value::String(name)) => name,
        _ => unreachable!("a details member serializes as its name"),
    }
}

/// The value of one `details` member of `code` (D15:2232).
fn detail(code: Code, member: DetailsMember) -> Value {
    use DetailsMember as M;
    match member {
        M::Reason => reasons(code),
        // A schema member's path: its names, `*` for a map entry, and none
        // for the root.
        M::Field => array(text()),
        M::Component => strings(["delivery-graph", "delivery-placement"]),
        M::Slot => strings(Slot::ALL.map(Slot::name)),
        M::Subject => of::<SubjectKind>(),
        M::ResourceGroup => strings(Group::ALL.map(Group::path)),
        M::Bound => pattern(BOUND_PATTERN),
        M::Limit | M::Actual | M::Required | M::Available => count(),
        M::ActualRelation => strings(["exact", "at-least"]),
    }
}

/// The three dependency records: a semantic group with the slots it reads,
/// a materialized slot, and an entry slot with what it depends on
/// (D15:2511-2514).
fn dependencies(definitions: &mut Map<String, Value>) {
    let groups = SemanticGroup::ALL.map(|group| {
        closed(&[
            ("group", constant(group.id())),
            (
                "inputs",
                json!({ "const": group.inputs().iter().map(|slot| slot.id()).collect::<Vec<_>>() }),
            ),
        ])
    });
    definitions.insert(
        "semanticDependencyView".into(),
        json!({ "oneOf": groups.to_vec() }),
    );
    definitions.insert(
        "materializedDependencyView".into(),
        closed(&[(
            "slot",
            strings(MaterializedSlot::ALL.map(MaterializedSlot::id)),
        )]),
    );
    // Each common slot's origin, which every entry fixes alike, then the
    // entry's own: its source, a programmatic entry's call-site source when
    // it submits a sidecar, and its selector (D15:2514).
    let common: Vec<(&str, Value)> = MaterializedSlot::ALL
        .into_iter()
        .filter_map(|slot| Some((slot.id(), constant(slot.common_origin()?.id()))))
        .collect();
    let fixed = |entry: SafeOriginKind, selector: SafeOriginKind| {
        let mut required = common.clone();
        required.extend([
            ("entry-source", constant(entry.id())),
            ("profile-selector", constant(selector.id())),
        ]);
        required
    };
    let file = fixed(
        SafeOriginKind::ConfigurationFile,
        SafeOriginKind::ProfileSelectorCli,
    );
    let programmatic = fixed(
        SafeOriginKind::ConfigurationProgrammatic,
        SafeOriginKind::ProfileSelectorProgrammatic,
    );
    let call_site = constant(SafeOriginKind::ConfigurationProgrammatic.id());
    let origins = json!({
        "oneOf": [
            closed(&file),
            closed_with(&programmatic, &[("call-site-source", call_site)]),
        ]
    });
    definitions.insert(
        "entryDependencyView".into(),
        json!({
            "oneOf": [
                closed_with(
                    &[
                        ("slot", strings(["entry-source", "call-site-source"])),
                        ("source", definition("sourceView")),
                        ("revision", text()),
                    ],
                    &[("locator", definition("locatorView"))],
                ),
                closed(&[
                    ("slot", constant("profile-selector")),
                    ("selector", definition("selectorView")),
                ]),
                closed(&[("slot", constant("origins")), ("origins", origins)]),
            ]
        }),
    );
}

/// The Evidence view of each evidence kind with the origins that permit it,
/// and the views its members hold (D15:2482-2506).
fn evidence(definitions: &mut Map<String, Value>) {
    definitions.insert(
        "sourceView".into(),
        json!({
            "oneOf": [
                closed(&[("safe-opaque", pattern(HEX_PATTERN))]),
                closed(&[("redacted", definition("tokenView"))]),
            ]
        }),
    );
    let variants = [
        LocatorVariant::RelativePath,
        LocatorVariant::SafeLabel,
        LocatorVariant::SafeUri,
    ];
    definitions.insert(
        "locatorView".into(),
        json!({ "oneOf": variants.map(|variant| closed(&[(variant.id(), text())])).to_vec() }),
    );
    let segment = json!({
        "oneOf": [
            closed(&[("safe-member", json!({ "type": "string" }))]),
            closed(&[("array-index", count())]),
            closed(&[("redacted-member", definition("tokenView"))]),
        ]
    });
    definitions.insert("pathSegmentView".into(), segment);
    definitions.insert(
        "valueView".into(),
        json!({
            "oneOf": [
                closed(&[("exact-json-pointer", json!({ "type": "string" }))]),
                closed(&[("redacted-logical-path", array(definition("pathSegmentView")))]),
            ]
        }),
    );
    let length = json!({ "oneOf": [closed(&[("exact", count())]), constant("over-limit")] });
    definitions.insert(
        "selectorView".into(),
        json!({
            "oneOf": [
                closed(&[("absent", json!({ "type": "null" }))]),
                closed(&[("safe-profile-id", text())]),
                closed(&[(
                    "redacted-value",
                    closed_with(
                        &[("type", strings(JSON_TYPES)), ("token", definition("tokenView"))],
                        &[("length", length)],
                    ),
                )]),
            ]
        }),
    );
    definitions.insert(
        "componentView".into(),
        array(json!({
            "oneOf": [
                closed(&[("fixed-role", text())]),
                closed(&[("safe-key", pattern(HEX_PATTERN))]),
                closed(&[("redacted-key", definition("tokenView"))]),
            ]
        })),
    );
    definitions.insert(
        "evidenceView".into(),
        json!({ "oneOf": EvidenceKind::ALL.map(evidence_kind).to_vec() }),
    );
}

/// The Evidence view of `kind`: its kind and an origin that permits it, then
/// the members the kind has (D15:2493).
fn evidence_kind(kind: EvidenceKind) -> Value {
    let origins = SafeOriginKind::ALL
        .iter()
        .filter(|origin| origin.permitted_evidence_kinds().contains(&kind))
        .map(|origin| origin.id());
    let mut required = vec![("kind", constant(kind.id())), ("origin", strings(origins))];
    let optional = match kind {
        EvidenceKind::EntrySource => {
            required.push(("source", definition("sourceView")));
            vec![
                ("locator", definition("locatorView")),
                ("path", array(definition("pathSegmentView"))),
                ("span", span()),
            ]
        }
        EvidenceKind::ConfigurationValue => {
            required.extend([
                ("source", definition("sourceView")),
                ("value", definition("valueView")),
            ]);
            vec![("locator", definition("locatorView")), ("span", span())]
        }
        EvidenceKind::ProfileSelector => {
            required.push(("selector", definition("selectorView")));
            Vec::new()
        }
        _ => {
            required.push(("component", definition("componentView")));
            Vec::new()
        }
    };
    closed_with(&required, &optional)
}

fn span() -> Value {
    closed(&[("start", count()), ("end", count())])
}

/// The profile view's twelve groups in their order (D15:1214-1237), each an
/// exact reference where it names an artifact.
fn profile(definitions: &mut Map<String, Value>) {
    let specification = || definition("versionedIdentity");
    let pinned = || {
        closed(&[
            ("identity", token()),
            ("revision", token()),
            ("semanticDigest", digest()),
        ])
    };
    let reference = || definition("exactReference");
    let locale = text;
    let candidate = json!({
        "oneOf": [text(), closed(&[("kind", constant("intent-source-locale"))])]
    });
    let section = decimals(&fixture_limits(1).section());
    definitions.insert(
        "profileView".into(),
        closed(&[
            ("profileSpecification", pinned()),
            (
                "projectIdentity",
                closed(&[("projectId", text()), ("selectionScope", text())]),
            ),
            (
                "localeCanonicalization",
                closed(&[
                    ("specification", specification()),
                    (
                        "dataset",
                        closed(&[("identity", text()), ("semanticDigest", text())]),
                    ),
                ]),
            ),
            (
                "projectLocales",
                closed(&[
                    ("requestedLocales", array(locale())),
                    ("defaultRequestedLocale", locale()),
                    ("defaultSourceLocale", nullable(locale())),
                ]),
            ),
            (
                "localeNegotiation",
                closed(&[
                    ("specification", specification()),
                    ("aliases", map(locale())),
                ]),
            ),
            (
                "messageFallback",
                closed(&[
                    ("specification", specification()),
                    ("mappings", map(array(candidate))),
                ]),
            ),
            (
                "coverage",
                closed(&[
                    ("specification", specification()),
                    ("vocabulary", pinned()),
                    ("defaultMode", of::<CoverageMode>()),
                    ("decisions", map(map(of::<CoverageMode>()))),
                ]),
            ),
            (
                "requiredPolicies",
                closed(&[
                    ("resourceLimits", reference()),
                    ("trust", reference()),
                    ("sourceAdmission", reference()),
                    ("approval", reference()),
                    ("selection", reference()),
                    ("projectProfileResolution", section),
                ]),
            ),
            (
                "optionalPolicies",
                closed(&[
                    ("providerRouting", nullable(reference())),
                    ("glossarySet", nullable(reference())),
                ]),
            ),
            (
                "targets",
                map(closed(&[
                    ("profile", reference()),
                    ("requestedLocales", array(locale())),
                    ("defaultRequestedLocale", nullable(locale())),
                    ("effectiveDefaultRequestedLocale", locale()),
                ])),
            ),
            (
                "deploymentGroups",
                map(closed(&[
                    ("members", array(text())),
                    (
                        "hydrationRelations",
                        array(closed(&[("server", text()), ("client", text())])),
                    ),
                ])),
            ),
            (
                "delivery",
                closed(&[
                    ("graph", specification()),
                    ("placement", specification()),
                    ("mode", constant("duplicate")),
                ]),
            ),
        ]),
    );
}

/// The closed form of `section`: each object with exactly its members, and
/// each bound a decimal count.
fn decimals(section: &Value) -> Value {
    match section {
        Value::Object(members) => {
            let members: Vec<(&str, Value)> = members
                .iter()
                .map(|(name, member)| (name.as_str(), decimals(member)))
                .collect();
            closed(&members)
        }
        _ => count(),
    }
}

/// The expected-outcome fixture (D15:3271-3303): a checked outcome with its
/// profile and bindings, or a blocked one with neither, and the Resolution
/// Evidence of both projections, whose Shared projection is absent after an
/// entry failure before materialization. Only a reserved record, the
/// terminal Finding or the Entry reserve's cause, has no owner: a row
/// without one holds the terminal reporting-limit Finding, or a cause that
/// names its key (D15:3303).
fn fixture(definitions: &mut Map<String, Value>) {
    let row = |record: &str| {
        closed(&[
            ("owner", definition("unitView")),
            ("record", definition(record)),
        ])
    };
    let owned = |record: &str| array(row(record));
    let terminal = Code::ReportingLimitExceeded;
    definitions.insert("terminalFindingView".into(), finding(terminal));
    definitions.insert(
        "reserveCauseView".into(),
        closed(&[("cause", definition(&key_definition(terminal)))]),
    );
    let reserved = |record: &str, reserve: &str| {
        array(json!({
            "oneOf": [
                row(record),
                closed(&[
                    ("owner", json!({ "type": "null" })),
                    ("record", definition(reserve)),
                ]),
            ]
        }))
    };
    definitions.insert(
        "sharedProjection".into(),
        closed(&[
            ("findings", reserved("findingView", "terminalFindingView")),
            ("evaluationStatus", owned("evaluationStatusView")),
            ("coverageDecisionBases", owned("coverageDecisionBasisView")),
            ("semanticDependencies", owned("semanticDependencyView")),
            (
                "materializedDependencies",
                owned("materializedDependencyView"),
            ),
        ]),
    );
    definitions.insert(
        "entryProjection".into(),
        closed(&[
            ("findings", reserved("findingView", "terminalFindingView")),
            ("evaluationStatus", owned("evaluationStatusView")),
            ("occurrences", owned("findingOccurrenceView")),
            (
                "coverageDecisionEvidence",
                owned("coverageDecisionEvidenceView"),
            ),
            ("evidenceIndex", owned("evidenceView")),
            ("entryDependencies", owned("entryDependencyView")),
            (
                "sharedUnavailabilityCauses",
                reserved("sharedUnavailabilityCauseView", "reserveCauseView"),
            ),
        ]),
    );
    definitions.insert(
        "resolutionEvidence".into(),
        closed(&[
            ("complete", json!({ "type": "boolean" })),
            ("terminalStates", array(definition("terminalStateView"))),
            ("shared", nullable(definition("sharedProjection"))),
            (
                "sharedUnavailableCauseKeys",
                array(definition("findingKeyView")),
            ),
            ("entry", definition("entryProjection")),
        ]),
    );
    let outcome = |name: &str, profile: Value, bindings: Value| {
        closed(&[
            ("outcome", constant(name)),
            (
                "resolverConstructionIdentity",
                definition("constructionIdentity"),
            ),
            ("profile", profile),
            ("resolvedProfileArtifactBindings", bindings),
            ("resolutionEvidence", definition("resolutionEvidence")),
        ])
    };
    let bindings = array(closed(&[("key", definition("exactReference"))]));
    definitions.insert(
        "expectedOutcomeFixture".into(),
        json!({
            "oneOf": [
                outcome("checked", definition("profileView"), bindings),
                outcome("blocked", json!({ "type": "null" }), json!({ "type": "null" })),
            ]
        }),
    );
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::conformance::case::schema;
    use crate::construction::admission::DRAFT_7;
    use crate::construction::validator::schema::Schema;

    const RECORDS: &[u8] = include_bytes!("../../../fixtures/evidence-records/vectors.json");
    const PROFILE_VIEWS: &[u8] = include_bytes!("../../../fixtures/profile-view/vectors.json");

    /// This crate's validator and the independent oracle over one definition
    /// of the Case Schema, as the runner reads a fixture's form.
    struct Validators {
        read: Schema,
        oracle: jsonschema::Validator,
    }

    impl Validators {
        fn of(definition: &str) -> Self {
            let schema = schema().unwrap();
            let root = json!({
                "$schema": DRAFT_7,
                "$ref": format!("#/definitions/{definition}"),
                "definitions": schema["definitions"],
            });
            Self {
                read: Schema::compile(&root).unwrap(),
                oracle: jsonschema::draft7::new(&root).unwrap(),
            }
        }

        fn admits(&self, value: &Value) -> bool {
            let read = self.read.validate(value).valid;
            let oracle = self.oracle.is_valid(value);
            assert_eq!(read, oracle, "the validators disagree on {value}");
            read
        }
    }

    /// The view definition of each record kind (D15:2519).
    fn definition_of(kind: &str) -> &'static str {
        match kind {
            "entry-finding" | "shared-finding" => "findingView",
            "entry-evaluation-status" | "shared-evaluation-status" => "evaluationStatusView",
            "finding-occurrence" => "findingOccurrenceView",
            "coverage-decision-basis" => "coverageDecisionBasisView",
            "coverage-decision-evidence" => "coverageDecisionEvidenceView",
            "evidence-index" => "evidenceView",
            "shared-unavailability-cause" => "sharedUnavailabilityCauseView",
            "semantic-dependency" => "semanticDependencyView",
            "materialized-dependency" => "materializedDependencyView",
            "entry-dependency" => "entryDependencyView",
            other => panic!("no record kind {other}"),
        }
    }

    /// The committed accounting view of each record kind holds its owner in
    /// the Unit form and its record in the kind's form, and neither form
    /// admits a member the view does not have.
    #[test]
    fn each_committed_record_view_is_in_its_form() {
        let vectors: Value = serde_json::from_slice(RECORDS).unwrap();
        let views = vectors["accountingViews"].as_array().unwrap();
        assert_eq!(views.len(), 12);
        let unit = Validators::of("unitView");
        for view in views {
            let kind = view["recordKind"].as_str().unwrap();
            let accounting = &view["accounting"];
            assert!(unit.admits(&accounting["owner"]), "{kind}");
            let form = Validators::of(definition_of(kind));
            let record = &accounting["record"][kind];
            assert!(form.admits(record), "{kind}: {record}");
            let mut extended = record.clone();
            extended["extra"] = json!(true);
            assert!(!form.admits(&extended), "{kind}");
        }
    }

    /// An entry's `origins` dependency maps each common slot and the entry's
    /// own slots to the Safe Origin Kind each fixes, a programmatic entry's
    /// with its call-site source when it has a sidecar; one keyed by Safe
    /// Origin Kinds, a file entry with a call-site source, or another
    /// entry's selector is not in the form (D15:2508, 2514, 2517).
    #[test]
    fn an_origins_dependency_maps_each_slot_to_the_kind_it_fixes() {
        let form = Validators::of("entryDependencyView");
        let common = [
            (
                "specification-input",
                "profile-resolution-specification-input",
            ),
            (
                "intent-surface-class-vocabulary",
                "intent-surface-class-vocabulary",
            ),
            (
                "locale-canonicalization-data",
                "locale-canonicalization-data",
            ),
            (
                "artifact-collection",
                "profile-resolution-artifact-collection",
            ),
            ("verification-input", "resource-limit-policy-verification"),
            ("implementation-capability", "implementation-capability"),
        ];
        let origins = |entry: &str, selector: &str, call_site: bool| {
            let mut origins = common
                .iter()
                .map(|(slot, kind)| ((*slot).to_owned(), json!(kind)))
                .collect::<Map<_, _>>();
            origins.insert("entry-source".into(), json!(entry));
            origins.insert("profile-selector".into(), json!(selector));
            if call_site {
                origins.insert(
                    "call-site-source".into(),
                    json!("configuration-programmatic"),
                );
            }
            json!({ "slot": "origins", "origins": origins })
        };
        let file = ("configuration-file", "profile-selector-cli");
        let programmatic = (
            "configuration-programmatic",
            "profile-selector-programmatic",
        );
        assert!(form.admits(&origins(file.0, file.1, false)));
        assert!(form.admits(&origins(programmatic.0, programmatic.1, false)));
        assert!(form.admits(&origins(programmatic.0, programmatic.1, true)));
        assert!(!form.admits(&origins(file.0, file.1, true)));
        assert!(!form.admits(&origins(file.0, programmatic.1, false)));
        let mut by_kind = origins(file.0, file.1, false);
        let slots = by_kind["origins"].as_object().unwrap().clone();
        by_kind["origins"] = Value::Object(
            slots
                .into_iter()
                .map(|(_, kind)| (kind.as_str().unwrap().to_owned(), kind))
                .collect(),
        );
        assert!(!form.admits(&by_kind));
    }

    /// Every committed profile view is in the profile view's form, and a view
    /// without one group, or with a policy bound that is no count, is not.
    #[test]
    fn each_committed_profile_view_is_in_its_form() {
        let vectors: Value = serde_json::from_slice(PROFILE_VIEWS).unwrap();
        let views = vectors["views"].as_array().unwrap();
        assert_eq!(views.len(), 16);
        let form = Validators::of("profileView");
        for view in views {
            let view = &view["view"];
            assert!(form.admits(view), "{view}");
        }
        let view = &views[0]["view"];
        let mut missing = view.clone();
        missing.as_object_mut().unwrap().remove("delivery");
        assert!(!form.admits(&missing));
        let mut bound = view.clone();
        bound["requiredPolicies"]["projectProfileResolution"]["configurationInput"]["maxNodes"] =
            json!("01");
        assert!(!form.admits(&bound));
    }

    /// The Finding view holds one branch per code, each with exactly the
    /// `details` members the code reports, and a Finding of one code is not
    /// in another code's branch.
    #[test]
    fn a_finding_is_in_its_own_code_s_form() {
        let schema = schema().unwrap();
        let branches = schema["definitions"]["findingView"]["oneOf"]
            .as_array()
            .unwrap();
        assert_eq!(branches.len(), Code::ALL.len());
        for (code, branch) in Code::ALL.iter().zip(branches) {
            let name = key_definition(*code);
            assert_eq!(branch["properties"]["key"], definition(&name));
            assert_eq!(
                schema["definitions"][&name]["properties"]["code"]["const"],
                code.as_str()
            );
            let members = details_members(*code);
            let details = &branch["properties"]["details"];
            if members.is_empty() {
                assert!(details.is_null(), "{}", code.as_str());
            } else {
                let mut named: Vec<String> = details["required"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|name| name.as_str().unwrap().to_owned())
                    .collect();
                named.sort();
                let mut expected: Vec<String> = members.iter().map(|m| member_name(*m)).collect();
                expected.sort();
                assert_eq!(named, expected, "{}", code.as_str());
            }
        }
        let vectors: Value = serde_json::from_slice(RECORDS).unwrap();
        let finding = vectors["accountingViews"]
            .as_array()
            .unwrap()
            .iter()
            .find(|view| view["recordKind"] == "entry-finding")
            .unwrap()["accounting"]["record"]["entry-finding"]
            .clone();
        let form = Validators::of("findingView");
        let mut other = finding.clone();
        other["severity"] = json!("warning");
        assert!(!form.admits(&other));
        let mut unknown = finding;
        unknown["key"]["code"] = json!("project-profile-unknown");
        assert!(!form.admits(&unknown));
    }

    /// An expected outcome is checked with its profile and bindings, or
    /// blocked with neither, and its bundle is complete or not; only a
    /// reserved row has no owner.
    #[test]
    fn an_expected_outcome_is_checked_or_blocked() {
        let vectors: Value = serde_json::from_slice(PROFILE_VIEWS).unwrap();
        let profile = vectors["views"][0]["view"].clone();
        let empty = json!({
            "complete": true,
            "terminalStates": [],
            "shared": {
                "findings": [],
                "evaluationStatus": [],
                "coverageDecisionBases": [],
                "semanticDependencies": [],
                "materializedDependencies": []
            },
            "sharedUnavailableCauseKeys": [],
            "entry": {
                "findings": [],
                "evaluationStatus": [],
                "occurrences": [],
                "coverageDecisionEvidence": [],
                "evidenceIndex": [],
                "entryDependencies": [],
                "sharedUnavailabilityCauses": []
            }
        });
        let identity = "rci0_AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";
        let checked = json!({
            "outcome": "checked",
            "resolverConstructionIdentity": identity,
            "profile": profile,
            "resolvedProfileArtifactBindings": [
                { "key": profile["requiredPolicies"]["trust"] }
            ],
            "resolutionEvidence": empty,
        });
        let form = Validators::of("expectedOutcomeFixture");
        assert!(form.admits(&checked));
        let mut blocked = checked.clone();
        blocked["outcome"] = json!("blocked");
        assert!(!form.admits(&blocked));
        blocked["profile"] = Value::Null;
        blocked["resolvedProfileArtifactBindings"] = Value::Null;
        blocked["resolutionEvidence"]["shared"] = Value::Null;
        assert!(form.admits(&blocked));
        let mut complete = blocked.clone();
        complete["resolutionEvidence"]["complete"] = json!("yes");
        assert!(!form.admits(&complete));
        let vectors: Value = serde_json::from_slice(RECORDS).unwrap();
        let status = vectors["accountingViews"]
            .as_array()
            .unwrap()
            .iter()
            .find(|view| view["recordKind"] == "entry-evaluation-status")
            .unwrap()["accounting"]["record"]["entry-evaluation-status"]
            .clone();
        let mut ownerless = checked;
        ownerless["resolutionEvidence"]["entry"]["evaluationStatus"] =
            json!([{ "owner": null, "record": status }]);
        assert!(!form.admits(&ownerless));
        // Only the terminal Finding, and a cause naming its key, may have no
        // owner.
        let key = |check: &str, code: &str, reason: Option<&str>| {
            let mut key = json!({
                "specification": { "identity": "intlify-project-profile", "revision": "0" },
                "phase": "reporting-finalization",
                "check": check,
                "code": code,
                "subject": { "token": format!("sha256:{}", "5".repeat(64)) },
            });
            if let Some(reason) = reason {
                key["reason"] = json!(reason);
            }
            key
        };
        let terminal = key(
            "reporting.diagnostic-domain.complete",
            "project-profile-reporting-limit-exceeded",
            Some("entry"),
        );
        let finding = json!({
            "key": terminal,
            "severity": "error",
            "blocking": true,
            "details": { "reason": "entry", "bound": "maxRecords" },
        });
        let mut reserved = blocked.clone();
        reserved["resolutionEvidence"]["complete"] = json!(false);
        reserved["resolutionEvidence"]["entry"]["findings"] =
            json!([{ "owner": null, "record": finding }]);
        reserved["resolutionEvidence"]["entry"]["sharedUnavailabilityCauses"] =
            json!([{ "owner": null, "record": { "cause": terminal } }]);
        assert!(form.admits(&reserved));
        // A committed ordinary Finding is in its row's form with its owner,
        // and is not without it; nor is a cause naming its key.
        let ordinary = vectors["accountingViews"]
            .as_array()
            .unwrap()
            .iter()
            .find(|view| view["recordKind"] == "entry-finding")
            .unwrap()["accounting"]
            .clone();
        let record = &ordinary["record"]["entry-finding"];
        let with = |findings: Value, causes: Value| {
            let mut outcome = reserved.clone();
            outcome["resolutionEvidence"]["entry"]["findings"] = findings;
            outcome["resolutionEvidence"]["entry"]["sharedUnavailabilityCauses"] = causes;
            form.admits(&outcome)
        };
        let reserve = json!([{ "owner": null, "record": { "cause": terminal } }]);
        let owned = json!([{ "owner": ordinary["owner"], "record": record }]);
        let unowned = json!([{ "owner": null, "record": record }]);
        assert!(with(owned.clone(), reserve.clone()));
        assert!(!with(unowned, reserve));
        let cause = json!([{ "owner": null, "record": { "cause": record["key"] } }]);
        assert!(!with(owned, cause));
    }
}
