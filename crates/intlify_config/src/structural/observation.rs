// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Complete private structural-result observation, outside the measured interval.
//! Physical node IDs become canonical value-preorder ranks; schema IDs become
//! generated schema locations. Neither allocator addresses nor elapsed time enter it.

use std::collections::BTreeMap;

use crate::benchmark::observation::{self, Frame, Observation};
use crate::materialize::{InputCounts, NodeId, NodeKind};

use super::eval::{IssueKind, LocationRole, SchemaIssue};
use super::program::SchemaId;
use super::{FragmentState, StructuralAnalysis, StructuralFailure};

impl<Policy, Target> StructuralAnalysis<Policy, Target> {
    pub(crate) const fn benchmark_profile_id_bound(&self) -> crate::input_limits::Bound {
        self.limits.max_profile_id_bytes
    }

    pub(crate) fn benchmark_structural_units(&self) -> Option<u64> {
        self.structural_units().or_else(|| {
            self.issues.iter().find_map(|issue| match issue.reason {
                StructuralFailure::StructuralWorkLimit { actual, .. } => Some(actual),
                _ => None,
            })
        })
    }

    pub(crate) fn benchmark_retained_record_counts(&self) -> (u64, u64, u64) {
        let admission = u64::try_from(self.issues.len()).expect("bounded admission issues");
        let Some(evaluation) = &self.evaluation else {
            // Actual retained record counts are zero. This does not assert that
            // an unavailable schema traversal had zero applicable work units.
            return (admission, 0, 0);
        };
        let fragments = u64::try_from(evaluation.fragments.len()).expect("bounded fragments");
        let mut issues = 0_u64;
        let mut pending: Vec<_> = evaluation.issues.iter().collect();
        while let Some(issue) = pending.pop() {
            issues = issues.checked_add(1).expect("addressable retained issues");
            pending.extend(issue.alternatives.iter());
        }
        (admission, fragments, issues)
    }

    pub(crate) fn benchmark_input_counts(&self) -> InputCounts {
        self.doc.counts()
    }

    pub(crate) fn benchmark_profile_counts(&self) -> Option<(u64, u64)> {
        let NodeKind::Object(profiles) = self.doc.node(self.profiles?).kind() else {
            return None;
        };
        Some((
            u64::try_from(profiles.len()).expect("bounded profiles"),
            profiles
                .keys()
                .map(|id| u64::try_from(id.len()).expect("bounded IDs"))
                .max()
                .unwrap_or(0),
        ))
    }

    pub(crate) fn benchmark_observation(&self) -> Observation {
        let order = observation::document_order(&self.doc);
        let locations: BTreeMap<_, _> = self.binding.program.locations().collect();
        let document = observation::document(&self.doc);
        let mut shared = Frame::new("structural-analysis");
        let mut entry = Frame::new("structural-analysis-entry");
        shared.digest(document.shared);
        entry.digest(document.entry.expect("materialized document source map"));
        shared.json(&self.binding.body);
        shared.uint(self.limits.max_profiles.get());
        shared.uint(self.limits.max_profile_id_bytes.get());
        shared.uint(self.limits.max_structural_analysis_units.get());
        shared.flag(self.selected_version);
        shared.flag(self.is_complete());
        shared.flag(self.profiles.is_some());
        if let Some(profiles) = self.profiles {
            shared.uint(order[&profiles]);
        }
        let mut excluded: Vec<_> = self.excluded.iter().map(|id| order[id]).collect();
        excluded.sort_unstable();
        shared.uint(u64::try_from(excluded.len()).expect("bounded exclusions"));
        for rank in excluded {
            shared.uint(rank);
        }
        shared.uint(u64::try_from(self.issues.len()).expect("bounded admission issues"));
        for issue in &self.issues {
            match issue.reason {
                StructuralFailure::RootTypeInvalid => shared.uint(0),
                StructuralFailure::SchemaVersionMissing => shared.uint(1),
                StructuralFailure::SchemaVersionTypeInvalid => shared.uint(2),
                StructuralFailure::SchemaVersionUnsupported => shared.uint(3),
                StructuralFailure::ProfilesLimit { limit, actual } => {
                    shared.uint(4);
                    shared.uint(limit.get());
                    shared.uint(actual);
                }
                StructuralFailure::ProfileIdLimit { limit, actual } => {
                    shared.uint(5);
                    shared.uint(limit.get());
                    shared.uint(actual);
                }
                StructuralFailure::StructuralWorkLimit { limit, actual } => {
                    shared.uint(6);
                    shared.uint(limit.get());
                    shared.uint(actual);
                }
            }
            entry.span(issue.span);
        }
        shared.flag(self.evaluation.is_some());
        if let Some(evaluation) = &self.evaluation {
            shared.flag(evaluation.admitted);
            shared.uint(evaluation.units);
            shared.uint(u64::try_from(evaluation.fragments.len()).expect("bounded fragments"));
            for fragment in &evaluation.fragments {
                shared.text(locations[&fragment.schema]);
                shared.uint(order[&fragment.subject]);
                shared.uint(match fragment.state {
                    FragmentState::Admitted => 0,
                    FragmentState::Invalid => 1,
                    FragmentState::TypeUnavailable => 2,
                    FragmentState::ResourceUnavailable => 3,
                });
            }
            observe_issues(
                &evaluation.issues,
                &locations,
                &order,
                &mut shared,
                &mut entry,
            );
        }
        Observation {
            shared: shared.finish(),
            entry: Some(entry.finish()),
        }
    }
}

fn observe_issues(
    issues: &[SchemaIssue],
    locations: &BTreeMap<SchemaId, &str>,
    order: &BTreeMap<NodeId, u64>,
    shared: &mut Frame,
    entry: &mut Frame,
) {
    shared.uint(u64::try_from(issues.len()).expect("bounded schema issues"));
    for issue in issues {
        shared.uint(match issue.kind {
            IssueKind::TypeInvalid => 0,
            IssueKind::RequiredFieldMissing => 1,
            IssueKind::ValueInvalid => 2,
            IssueKind::UnknownField => 3,
        });
        shared.text(locations[&issue.schema]);
        shared.uint(order[&issue.subject]);
        shared.uint(match issue.role {
            LocationRole::Value => 0,
            LocationRole::MemberKey => 1,
            LocationRole::MissingField => 2,
        });
        shared.flag(issue.missing_field.is_some());
        if let Some(field) = &issue.missing_field {
            shared.text(field);
        }
        entry.span(issue.span);
        observe_issues(&issue.alternatives, locations, order, shared, entry);
    }
}
