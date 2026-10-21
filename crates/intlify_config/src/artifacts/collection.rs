// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's `submitted-profile-resolution-artifact-collection` pre-invocation
//! boundary: every member must be formally representable (017), strictly
//! decoded within the caller's limits into the values the shared framing
//! carries, which has no numbers. A failure here is a typed integration result,
//! and the resolver is not invoked. The boundary keeps every occurrence,
//! invalid, duplicate, or unreferenced, for the resolver to admit: it never
//! deduplicates, drops a member, or looks at what a member declares.
//!
//! The container's own form and any inspection of host values belong to the
//! adapter that builds the list of members, so `malformed-container` and
//! `unsafe-host-inspection` are reported there.

use intlify_shared_json::decode::{self, DecodeFailure};

use super::envelope::FormalMember;
use super::{count, depth};

/// The caller's finite limits on a collection, each checked before the work it
/// bounds. There are no defaults, and they are not 015's `artifactAdmission`
/// bounds, which the resolver checks under the Resource Limit Policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CollectionLimits {
    /// The number of members.
    pub(crate) members: u64,
    /// The submitted bytes of each member.
    pub(crate) member_bytes: u64,
    /// The submitted bytes of all members together.
    pub(crate) total_bytes: u64,
    /// The nesting of objects and arrays in each member.
    pub(crate) depth: u64,
}

/// What a boundary limit bounds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CollectionLimit {
    Members,
    MemberBytes,
    TotalBytes,
    Depth,
}

/// Why the boundary refuses a collection. Each has one of 015's closed
/// reasons, which [`CollectionFailure::reason`] names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CollectionFailure {
    /// No collection was supplied.
    Missing,
    /// The collection or a member is over one of the caller's limits.
    OverBound(CollectionLimit),
    /// A member is beyond the shared reader's or framing's own capacity.
    BeyondCapacity,
    /// A member is not one strictly decoded JSON value without numbers.
    Unrepresentable,
}

impl CollectionFailure {
    /// 015's closed reason at the boundary. Every limit is applied before
    /// anything is decoded, as 017 requires, so an over-bound member is
    /// reported before an unrepresentable one wherever each is.
    pub(crate) const fn reason(self) -> &'static str {
        match self {
            Self::Missing => "missing-input",
            Self::OverBound(_) | Self::BeyondCapacity => "formal-envelope-over-bound",
            Self::Unrepresentable => "unrepresentable-member",
        }
    }
}

/// A collection past the boundary: every member, in the order submitted,
/// which is not part of the input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FormalCollection(Vec<FormalMember>);

impl FormalCollection {
    pub(crate) fn members(&self) -> &[FormalMember] {
        &self.0
    }
}

/// Read a collection at the pre-invocation boundary. The member count, each
/// member's bytes, the total, and each member's depth are checked before any
/// member is decoded; then every member is decoded and framed once, and a
/// capacity failure anywhere comes before an unrepresentable member anywhere.
pub(crate) fn read_collection(
    members: Option<&[&[u8]]>,
    limits: &CollectionLimits,
) -> Result<FormalCollection, CollectionFailure> {
    let members = members.ok_or(CollectionFailure::Missing)?;
    if count(members.len()) > limits.members {
        return Err(CollectionFailure::OverBound(CollectionLimit::Members));
    }
    let mut total = 0_u64;
    for bytes in members {
        let length = count(bytes.len());
        if length > limits.member_bytes {
            return Err(CollectionFailure::OverBound(CollectionLimit::MemberBytes));
        }
        total = total.saturating_add(length);
    }
    if total > limits.total_bytes {
        return Err(CollectionFailure::OverBound(CollectionLimit::TotalBytes));
    }
    if members.iter().any(|bytes| depth(bytes) > limits.depth) {
        return Err(CollectionFailure::OverBound(CollectionLimit::Depth));
    }
    let mut formal = Vec::with_capacity(members.len());
    let mut unrepresentable = false;
    for bytes in members {
        match decode::value(bytes, true) {
            Ok(value) => {
                formal
                    .push(FormalMember::new(value).map_err(|_| CollectionFailure::BeyondCapacity)?);
            }
            Err(DecodeFailure::Capacity) => return Err(CollectionFailure::BeyondCapacity),
            Err(_) => unrepresentable = true,
        }
    }
    if unrepresentable {
        return Err(CollectionFailure::Unrepresentable);
    }
    Ok(FormalCollection(formal))
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    use super::*;
    use crate::artifacts::envelope::{envelope_of, ArtifactKind};

    const ROOM: CollectionLimits = CollectionLimits {
        members: u64::MAX,
        member_bytes: u64::MAX,
        total_bytes: u64::MAX,
        depth: u64::MAX,
    };

    fn bytes(value: &Value) -> Vec<u8> {
        serde_json::to_vec(value).unwrap()
    }

    fn read(
        members: &[&[u8]],
        limits: &CollectionLimits,
    ) -> Result<FormalCollection, CollectionFailure> {
        read_collection(Some(members), limits)
    }

    #[test]
    fn an_absent_collection_is_missing_and_an_empty_one_is_admitted() {
        assert_eq!(
            read_collection(None, &ROOM),
            Err(CollectionFailure::Missing)
        );
        assert_eq!(read(&[], &ROOM).unwrap().members(), []);
    }

    #[test]
    fn each_failure_has_its_015_reason() {
        let reasons = [
            (CollectionFailure::Missing, "missing-input"),
            (
                CollectionFailure::OverBound(CollectionLimit::Members),
                "formal-envelope-over-bound",
            ),
            (
                CollectionFailure::OverBound(CollectionLimit::MemberBytes),
                "formal-envelope-over-bound",
            ),
            (
                CollectionFailure::OverBound(CollectionLimit::TotalBytes),
                "formal-envelope-over-bound",
            ),
            (
                CollectionFailure::OverBound(CollectionLimit::Depth),
                "formal-envelope-over-bound",
            ),
            (
                CollectionFailure::BeyondCapacity,
                "formal-envelope-over-bound",
            ),
            (CollectionFailure::Unrepresentable, "unrepresentable-member"),
        ];
        for (failure, reason) in reasons {
            assert_eq!(failure.reason(), reason, "{failure:?}");
        }
    }

    #[test]
    fn each_limit_admits_its_exact_value_and_refuses_the_next() {
        // Counted by hand: 12 and 23 bytes, nested 1 and 3 deep.
        let members: [&[u8]; 2] = [br#"{"kind":"x"}"#, br#"{"body":{"content":{}}}"#];
        assert_eq!(members.map(<[u8]>::len), [12, 23]);
        let exact = CollectionLimits {
            members: 2,
            member_bytes: 23,
            total_bytes: 35,
            depth: 3,
        };
        assert!(read(&members, &exact).is_ok());
        let over = [
            (
                CollectionLimits {
                    members: 1,
                    ..exact
                },
                CollectionLimit::Members,
            ),
            (
                CollectionLimits {
                    member_bytes: 22,
                    ..exact
                },
                CollectionLimit::MemberBytes,
            ),
            (
                CollectionLimits {
                    total_bytes: 34,
                    ..exact
                },
                CollectionLimit::TotalBytes,
            ),
            (
                CollectionLimits { depth: 2, ..exact },
                CollectionLimit::Depth,
            ),
        ];
        for (limits, limit) in over {
            assert_eq!(
                read(&members, &limits),
                Err(CollectionFailure::OverBound(limit)),
                "{limit:?}"
            );
        }
    }

    #[test]
    fn limits_come_before_any_member_is_decoded() {
        // Neither member is JSON, yet each limit speaks first.
        let members: [&[u8]; 2] = [b"[[[ not json", b"{"];
        let cases = [
            (
                CollectionLimits { members: 1, ..ROOM },
                CollectionLimit::Members,
            ),
            (
                CollectionLimits {
                    member_bytes: 11,
                    ..ROOM
                },
                CollectionLimit::MemberBytes,
            ),
            (
                CollectionLimits {
                    total_bytes: 12,
                    ..ROOM
                },
                CollectionLimit::TotalBytes,
            ),
            (
                CollectionLimits { depth: 2, ..ROOM },
                CollectionLimit::Depth,
            ),
        ];
        for (limits, limit) in cases {
            assert_eq!(
                read(&members, &limits),
                Err(CollectionFailure::OverBound(limit)),
                "{limit:?}"
            );
        }
        assert_eq!(
            read(&members, &ROOM),
            Err(CollectionFailure::Unrepresentable)
        );
    }

    #[test]
    fn a_member_over_its_own_limit_is_reported_before_the_total() {
        // The total is past its limit before the long member is reached.
        let members: [&[u8]; 3] = [b"\"aaa\"", b"\"bbb\"", b"\"cccccc\""];
        let limits = CollectionLimits {
            member_bytes: 5,
            total_bytes: 9,
            ..ROOM
        };
        assert_eq!(
            read(&members, &limits),
            Err(CollectionFailure::OverBound(CollectionLimit::MemberBytes))
        );
    }

    #[test]
    fn what_the_strict_decoder_refuses_is_unrepresentable() {
        let refused: [&[u8]; 9] = [
            br#"{"limit":1}"#,
            br#"{"kind":"a","kind":"b"}"#,
            br#"{"kind":"a""#,
            b"",
            b"\xef\xbb\xbf{}",
            b"{} {}",
            b"\xff",
            br#"["\ud800"]"#,
            b"-0",
        ];
        for member in refused {
            assert_eq!(
                read(&[member], &ROOM),
                Err(CollectionFailure::Unrepresentable),
                "{member:?}"
            );
        }
        // A value the framing carries is representable, whatever it declares.
        for member in [&b"null"[..], b"\"text\"", b"[true,false]", b"{}"] {
            assert!(read(&[member], &ROOM).is_ok(), "{member:?}");
        }
    }

    #[test]
    fn a_capacity_failure_anywhere_comes_before_an_unrepresentable_member() {
        // The shared reader stops at 64 levels whatever the caller allows.
        let deep = format!("{}{}", "[".repeat(65), "]".repeat(65));
        let number = br#"{"limit":1}"#.as_slice();
        for members in [[number, deep.as_bytes()], [deep.as_bytes(), number]] {
            assert_eq!(
                read(&members, &ROOM),
                Err(CollectionFailure::BeyondCapacity)
            );
        }
        let within = format!("{}{}", "[".repeat(64), "]".repeat(64));
        assert!(read(&[within.as_bytes()], &ROOM).is_ok());
    }

    #[test]
    fn every_occurrence_is_kept_in_the_order_submitted() {
        let approval = envelope_of(
            ArtifactKind::ApprovalPolicy,
            "approval",
            "1",
            json!({"localizedApproval": "required"}),
        );
        let submitted = [
            bytes(&approval),
            bytes(&json!(["not", "an", "envelope"])),
            serde_json::to_string_pretty(&approval)
                .unwrap()
                .into_bytes(),
            bytes(&json!({"kind": "localized-message"})),
        ];
        let members: Vec<&[u8]> = submitted.iter().map(Vec::as_slice).collect();
        let collection = read(&members, &ROOM).unwrap();
        let values: Vec<&Value> = collection
            .members()
            .iter()
            .map(FormalMember::value)
            .collect();
        assert_eq!(
            values,
            [
                &approval,
                &json!(["not", "an", "envelope"]),
                &approval,
                &json!({"kind": "localized-message"}),
            ]
        );
        // The two spellings of one member are one canonical body.
        let members = collection.members();
        assert_eq!(members[0].canonical_body(), members[2].canonical_body());
    }
}
