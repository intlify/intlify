// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Explicit finite test orchestration over ordinary functions. No benchmark
//! module, public facade, ambient configuration, wire type, or product defaults.

use std::sync::Arc;

use crate::input_limits::{Bound, InputLimits};
use crate::locale::fixtures::{fixture_pins, FixtureProvider};
use crate::locale::Canonicalizer;
use crate::materialize::{materialize_file, MaterializationError};
use crate::model::ProfileId;
use crate::references::{PolicyReference, TargetProfileReference};
use crate::resolver::evaluation::Evaluation;
use crate::resolver::limits::ConfirmedPolicy;
use crate::resolver::locales::{
    fixture_policy, resolve_locales, LocaleFields, LocaleInvariant, ResolvedLocales,
};
use crate::structural::selection::{Selection, SelectionFailure, SelectorInput};
use crate::structural::{AuthoringSchema, StructuralAnalysis, StructuralLimits};

type Schema = AuthoringSchema<PolicyReference, TargetProfileReference>;
type Analysis = StructuralAnalysis<PolicyReference, TargetProfileReference>;

#[derive(Clone, Copy)]
pub(super) struct FixtureLimits {
    pub(super) input: InputLimits,
    pub(super) structural: StructuralLimits,
    pub(super) locale_occurrences: Bound,
    pub(super) requested_locales: Bound,
    pub(super) identifier_bytes: Bound,
}

impl FixtureLimits {
    pub(super) fn finite() -> Self {
        Self {
            input: crate::materialize_tests::limits(),
            structural: StructuralLimits {
                max_profiles: Bound::new(8).unwrap(),
                max_profile_id_bytes: Bound::new(64).unwrap(),
                max_structural_analysis_units: Bound::new(1_000_000).unwrap(),
            },
            locale_occurrences: Bound::new(16).unwrap(),
            requested_locales: Bound::new(8).unwrap(),
            identifier_bytes: Bound::new(128).unwrap(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Step {
    Materialize,
    Structural,
    Construct,
    Select,
    Locale,
}

impl Step {
    pub(super) const ALL: [Self; 5] = [
        Self::Materialize,
        Self::Structural,
        Self::Construct,
        Self::Select,
        Self::Locale,
    ];
}

/// Phase 7's record of every unit it ran and what it resolved.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Locales {
    pub(super) evaluation: Evaluation,
    pub(super) result: Result<ResolvedLocales, LocaleInvariant>,
}

pub(super) enum Outcome {
    Materialization(MaterializationError),
    Structural(Analysis),
    Selection(SelectionFailure),
    Locale(Locales),
}

/// Test-only call trace, not 015 Evidence, Findings, or an admitted outcome.
pub(super) struct Run {
    pub(super) steps: Vec<Step>,
    pub(super) selected: Option<ProfileId>,
    pub(super) outcome: Outcome,
}

pub(super) struct FixtureRunner {
    limits: FixtureLimits,
    schema: Schema,
    policy: ConfirmedPolicy,
    provider: Canonicalizer<FixtureProvider>,
}

impl FixtureRunner {
    pub(super) fn new(limits: FixtureLimits) -> Self {
        Self {
            limits,
            schema: Schema::for_model().expect("formal 015/017 configuration schema"),
            policy: fixture_policy(
                limits.identifier_bytes,
                limits.locale_occurrences,
                limits.requested_locales,
            ),
            provider: Canonicalizer::bind(
                &fixture_pins(),
                Some(FixtureProvider::new()),
                limits.identifier_bytes,
            )
            .expect("explicit finite test-owned provider binding"),
        }
    }

    pub(super) fn run(&self, source: Arc<[u8]>, selector: Option<&str>) -> Run {
        let mut steps = vec![Step::Materialize];
        let doc = match materialize_file(source, self.limits.input) {
            Ok(doc) => doc,
            Err(error) => {
                return Run {
                    steps,
                    selected: None,
                    outcome: Outcome::Materialization(error),
                }
            }
        };
        steps.push(Step::Structural);
        let analysis = self
            .schema
            .analyze(Arc::new(doc), self.limits.structural)
            .expect("ordinary admission invariant failure must fail the test");
        if !analysis.is_complete() {
            return Run {
                steps,
                selected: None,
                outcome: Outcome::Structural(analysis),
            };
        }
        steps.push(Step::Construct);
        let config = analysis
            .construct()
            .expect("ordinary construction invariant")
            .expect("complete analysis must construct the whole authoring root");
        steps.push(Step::Select);
        let bound = self.limits.structural.max_profile_id_bytes;
        let selector = selector.map_or_else(
            || SelectorInput::absent(bound),
            |value| SelectorInput::string(value, bound),
        );
        let selected = match analysis
            .select(&selector)
            .expect("ordinary selection invariant")
        {
            Selection::Selected(selected) => selected,
            Selection::Rejected(failure) => {
                return Run {
                    steps,
                    selected: None,
                    outcome: Outcome::Selection(failure),
                }
            }
            Selection::Unavailable(_) => {
                panic!("selection prerequisites must exist after complete construction")
            }
        };
        steps.push(Step::Locale);
        let declaration = config
            .profiles()
            .get(selected.id())
            .expect("exact selected declaration must exist in this complete root");
        let mut evaluation = Evaluation::default();
        let result = resolve_locales(
            &mut evaluation,
            Ok(&self.policy),
            Ok(&self.provider),
            &LocaleFields::declared(declaration),
        );
        Run {
            steps,
            selected: Some(selected.id().clone()),
            outcome: Outcome::Locale(Locales { evaluation, result }),
        }
    }
}
