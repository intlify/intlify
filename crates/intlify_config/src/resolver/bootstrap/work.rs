// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `bootstrapOnly.artifactBootstrap` work (decision 015-164): the exact
//! total each of the four bounds counts, each counted before the work it
//! protects, and the Finding a total past its bound reports. The metadata scan
//! protects candidate selection; envelope depth, decoded allocation, and
//! validation work measure the chosen candidate and protect its body
//! admission.

use serde_json::{Map, Value};

use crate::artifacts::envelope::FormalMember;
use crate::input_limits::Bound;

use super::super::capability::{ArtifactBootstrapLimits, BootstrapBoundId, BootstrapGroup};
use super::super::evaluation::{Actual, Blocking, BootstrapLimitExceeded};
use super::super::limits::{Group, BOUND_COUNT, SECTION};

/// The four `artifactBootstrap` bounds, in the Capability Specification's
/// canonical order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkBound {
    EnvelopeDepth,
    DecodedAllocation,
    MetadataScan,
    ValidationWork,
}

impl WorkBound {
    pub(crate) const ALL: [Self; 4] = [
        Self::EnvelopeDepth,
        Self::DecodedAllocation,
        Self::MetadataScan,
        Self::ValidationWork,
    ];

    /// The bound's place in `bootstrapOnly`.
    pub(crate) const fn id(self) -> BootstrapBoundId {
        BootstrapBoundId {
            group: BootstrapGroup::ArtifactBootstrap,
            name: match self {
                Self::EnvelopeDepth => "maxEnvelopeDepth",
                Self::DecodedAllocation => "maxDecodedAllocationBytes",
                Self::MetadataScan => "maxMetadataScanUnits",
                Self::ValidationWork => "maxValidationWorkUnits",
            },
        }
    }

    /// The bound's value in the admitted capability.
    pub(crate) const fn limit(self, limits: &ArtifactBootstrapLimits) -> Bound {
        match self {
            Self::EnvelopeDepth => limits.max_envelope_depth,
            Self::DecodedAllocation => limits.max_decoded_allocation_bytes,
            Self::MetadataScan => limits.max_metadata_scan_units,
            Self::ValidationWork => limits.max_validation_work_units,
        }
    }
}

/// Whether `total` stays within `bound`, or the Finding 015 reports past it.
/// Every total here is exact: each is counted before the work it protects.
pub(crate) fn within(
    bound: WorkBound,
    limits: &ArtifactBootstrapLimits,
    total: u64,
) -> Result<(), Blocking> {
    let limit = bound.limit(limits);
    if total <= limit.get() {
        Ok(())
    } else {
        Err(Blocking::bootstrap_limit(BootstrapLimitExceeded {
            bound: bound.id(),
            limit,
            actual: Actual::Exact(total),
        }))
    }
}

/// The tuple fields the scan reads from one member, in its order.
const TUPLE_FIELDS: [&[&str]; 5] = [
    &["kind"],
    &["specification", "identity"],
    &["specification", "revision"],
    &["body", "identity"],
    &["body", "revision"],
];

/// The metadata scan of the complete submitted collection: each occurrence's
/// tuple fields in order, through the first that is absent or not a string.
/// Whether a value read names an admitted kind, satisfies the token grammar,
/// or belongs to a valid closed envelope is for classification to decide and
/// does not change the count.
pub(crate) fn metadata_scan_units(members: &[FormalMember]) -> u64 {
    members
        .iter()
        .try_fold(0_u64, |total, member| {
            total.checked_add(fields_read(member.value()))
        })
        .expect("an in-memory count fits u64")
}

/// The fields one occurrence's scan reads: at least `kind`, even from a member
/// that is not an object.
fn fields_read(member: &Value) -> u64 {
    let mut read = 0;
    for path in TUPLE_FIELDS {
        read += 1;
        let field = path.iter().try_fold(member, |value, name| value.get(*name));
        if !field.is_some_and(Value::is_string) {
            break;
        }
    }
    read
}

/// The deepest object or array nesting of the chosen candidate's member, its
/// root at one; a string or other scalar adds no level. Counted without
/// recursion, so a deep member cannot exhaust the stack.
pub(crate) fn envelope_depth(member: &Value) -> u64 {
    let mut deepest = 0;
    let mut pending = vec![(member, 1_u64)];
    while let Some((value, level)) = pending.pop() {
        match value {
            Value::Object(object) => {
                deepest = deepest.max(level);
                pending.extend(object.values().map(|child| (child, level + 1)));
            }
            Value::Array(items) => {
                deepest = deepest.max(level);
                pending.extend(items.iter().map(|child| (child, level + 1)));
            }
            _ => {}
        }
    }
    deepest
}

/// The UTF-8 bytes of every string value and object key decoded from the
/// chosen candidate's `content`, its own keys included, at every occurrence.
pub(crate) fn decoded_allocation_bytes(content: &Map<String, Value>) -> u64 {
    let mut bytes = 0_usize;
    let mut pending = Vec::new();
    open(content, &mut bytes, &mut pending);
    while let Some(value) = pending.pop() {
        match value {
            Value::String(text) => bytes += text.len(),
            Value::Object(object) => open(object, &mut bytes, &mut pending),
            Value::Array(items) => pending.extend(items),
            _ => {}
        }
    }
    u64::try_from(bytes).expect("an in-memory count fits u64")
}

/// Counts `object`'s keys and queues its values.
fn open<'a>(object: &'a Map<String, Value>, bytes: &mut usize, pending: &mut Vec<&'a Value>) {
    for (key, value) in object {
        *bytes += key.len();
        pending.push(value);
    }
}

/// The evaluations admitting the chosen candidate's body counts, from 015's
/// rule inventory and whether or not each runs: its integrity, its pin
/// lookup, its common body, and the section's presence, and, when the section
/// is present, the closed shapes of the section, its five groups, and its two
/// `diagnostics` domains, and its bound values.
pub(crate) fn validation_work_units(content: &Map<String, Value>) -> u64 {
    // The integrity, the pin lookup, the common body, and the presence.
    const BODY: usize = 4;
    // The section, `diagnostics`, and the six groups that hold the bounds.
    const CLOSED_SHAPES: usize = 2 + Group::ALL.len();
    let rules = if content.contains_key(SECTION) {
        BODY + CLOSED_SHAPES + BOUND_COUNT
    } else {
        BODY
    };
    u64::try_from(rules).expect("the rule inventory fits u64")
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind};
    use crate::resolver::capability::{BootstrapLimits, CapabilityBound};
    use crate::resolver::evaluation::{Code, Details};
    use crate::resolver::limits::fixture_limits;

    fn member(value: Value) -> FormalMember {
        FormalMember::new(value).unwrap()
    }

    /// A sealed Resource Limit Policy whose section sets the fixture bounds.
    fn policy() -> Value {
        envelope_of(
            ArtifactKind::ResourceLimitPolicy,
            "intlify-test-limits",
            "1",
            json!({ SECTION: fixture_limits(1).section() }),
        )
    }

    fn content(value: &Value) -> &Map<String, Value> {
        value["body"]["content"].as_object().unwrap()
    }

    #[test]
    fn the_scan_reads_each_field_in_order_through_the_first_absent_or_not_a_string() {
        let full = json!({
            "kind": "resource-limit-policy",
            "specification": { "identity": "s", "revision": "0" },
            "body": { "identity": "i", "revision": "1" },
        });
        let mut unsupported = full.clone();
        unsupported["kind"] = json!("no-such-kind");
        let mut no_body_revision = full.clone();
        no_body_revision["body"] = json!({ "identity": "i" });
        let mut not_a_string = full.clone();
        not_a_string["specification"]["identity"] = json!(true);
        for (value, read) in [
            (full, 5),
            (unsupported, 5),
            (no_body_revision, 5),
            (not_a_string, 2),
            // The order matters: the scan stops at the absent revision before it
            // reaches the body.
            (
                json!({
                    "kind": "resource-limit-policy",
                    "specification": { "identity": "s" },
                    "body": { "identity": "i", "revision": "1" },
                }),
                3,
            ),
            (json!({ "kind": "resource-limit-policy" }), 2),
            (json!({ "specification": { "identity": "s" } }), 1),
            (json!({ "kind": null }), 1),
            (json!("x"), 1),
            (json!([]), 1),
        ] {
            assert_eq!(fields_read(&value), read, "{value}");
        }
    }

    #[test]
    fn the_scan_counts_every_occurrence_of_the_collection() {
        assert_eq!(metadata_scan_units(&[]), 0);
        let sealed = member(policy());
        let members = [
            sealed.clone(),
            sealed,
            member(json!("x")),
            member(json!({})),
        ];
        assert_eq!(metadata_scan_units(&members), 5 + 5 + 1 + 1);
    }

    #[test]
    fn the_depth_counts_objects_and_arrays_and_not_scalars() {
        for (value, depth) in [
            (json!("x"), 0),
            (json!({}), 1),
            (json!({ "a": "x" }), 1),
            (json!({ "a": [] }), 2),
            (json!({ "a": { "b": ["c"] }, "d": "e" }), 3),
        ] {
            assert_eq!(envelope_depth(&value), depth, "{value}");
        }
        // The member, its body, the content, the section, `diagnostics`, and a
        // reporting domain.
        assert_eq!(envelope_depth(&policy()), 6);
    }

    #[test]
    fn the_depth_does_not_recurse_on_the_stack() {
        let mut value = json!("leaf");
        for _ in 0..1_000 {
            value = Value::Array(vec![value]);
        }
        // A stack this small cannot hold a thousand recursive frames. The value
        // comes back so that this thread drops it.
        let (depth, value) = std::thread::Builder::new()
            .stack_size(32 * 1024)
            .spawn(move || (envelope_depth(&value), value))
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(depth, 1_000);
        drop(value);
    }

    #[test]
    fn the_allocation_counts_each_decoded_string_and_key() {
        let content: Map<String, Value> =
            serde_json::from_str(r#"{"\u0061b":"\u00e9","c":["d","d",{"e":null}]}"#).unwrap();
        // "ab" and "c" and "e" as keys, then "é" in two bytes and "d" twice.
        assert_eq!(decoded_allocation_bytes(&content), 2 + 1 + 1 + 2 + 1 + 1);
        assert_eq!(decoded_allocation_bytes(&Map::new()), 0);
    }

    #[test]
    fn the_validation_work_is_four_without_the_section_and_sixty_four_with_it() {
        assert_eq!(validation_work_units(&Map::new()), 4);
        let policy = policy();
        assert_eq!(validation_work_units(content(&policy)), 4 + 8 + 52);
        // Counted whether or not the rules run: a section that is not even an
        // object counts every rule too.
        for section in [json!({}), json!("x"), json!(null)] {
            let mut content = Map::new();
            content.insert(SECTION.into(), section);
            assert_eq!(validation_work_units(&content), 64);
        }
    }

    #[test]
    fn each_bound_admits_its_exact_total_and_refuses_the_next() {
        let bound = |value| Bound::new(value).unwrap();
        for work in WorkBound::ALL {
            let mut limits = BootstrapLimits::MINIMUM.artifact_bootstrap;
            match work {
                WorkBound::EnvelopeDepth => limits.max_envelope_depth = bound(6),
                WorkBound::DecodedAllocation => limits.max_decoded_allocation_bytes = bound(6),
                WorkBound::MetadataScan => limits.max_metadata_scan_units = bound(6),
                WorkBound::ValidationWork => limits.max_validation_work_units = bound(6),
            }
            assert_eq!(within(work, &limits, 6), Ok(()), "{work:?}");
            let refused = within(work, &limits, 7).unwrap_err();
            assert_eq!(refused.code, Code::BootstrapLimitExceeded, "{work:?}");
            assert_eq!(refused.reason, None, "{work:?}");
            assert_eq!(
                refused.details,
                Some(Details::BootstrapLimit(BootstrapLimitExceeded {
                    bound: work.id(),
                    limit: bound(6),
                    actual: Actual::Exact(7),
                })),
                "{work:?}"
            );
        }
    }

    #[test]
    fn the_bounds_are_the_capability_rows_in_their_order() {
        let rows: Vec<BootstrapBoundId> = BootstrapLimits::MINIMUM
            .bounds()
            .into_iter()
            .map(|(id, _)| id)
            .filter(|id| id.group == BootstrapGroup::ArtifactBootstrap)
            .collect();
        assert_eq!(rows, WorkBound::ALL.map(WorkBound::id));
        assert_eq!(
            CapabilityBound::BootstrapOnly(WorkBound::MetadataScan.id()).to_string(),
            "bootstrapOnly.artifactBootstrap.maxMetadataScanUnits"
        );
    }
}
