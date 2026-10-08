// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The `intlify-continuity-edit-replay` verifier profile, revision `0`.
//!
//! A source edit is evidence of continuity only through a verifier. This
//! profile checks two things, and both are required: replaying the edit over
//! the exact before bytes gives exactly the after bytes, and a declaration's
//! range is carried across the edit by rules that leave no room for a guess.
//!
//! A replacement entirely before a range shifts it by the change in length.
//! One entirely after leaves it alone. One strictly inside moves only its
//! end. One that touches or crosses either end makes the range's fate
//! unreadable: replacing a literal's quotes and inserting a new message right
//! beside one look the same from the edit alone, so neither is taken as a
//! continuation. Those cases need an explicit decision.

use intlify_authoring::{ByteRange, VersionedIdentity};

use super::sources::RetainedSources;
use crate::registry::SourceEdit;

/// The identity of the one verifier profile this crate implements.
pub const EDIT_REPLAY_PROFILE: &str = "intlify-continuity-edit-replay";

/// The revision of [`EDIT_REPLAY_PROFILE`] this crate implements.
pub const EDIT_REPLAY_REVISION: &str = "0";

/// Return whether a `verified-edit` names the profile this crate implements.
#[must_use]
pub fn is_edit_replay_profile(profile: &VersionedIdentity) -> bool {
    profile.identity().as_str() == EDIT_REPLAY_PROFILE
        && profile.revision().as_str() == EDIT_REPLAY_REVISION
}

/// What one edit does to one range of its before bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangeFate {
    /// The range survives intact, at this range of the after bytes.
    Moved(ByteRange),
    /// A replacement touches or crosses one of the range's ends.
    Touched,
    /// One replacement replaces the whole range.
    Replaced,
}

/// Carry one range of the before bytes across an edit.
#[must_use]
pub fn fate(edit: &SourceEdit, range: ByteRange) -> RangeFate {
    let (start, end) = (range.start(), range.end());
    let mut shift: i128 = 0;
    let mut inner: i128 = 0;
    for replacement in edit.replacements() {
        let (from, to) = (replacement.range().start(), replacement.range().end());
        let delta = replacement.text().len() as i128 - i128::from(to - from);
        if from == to {
            // An insertion at either end touches it: its text could belong to
            // the range or to its neighbour.
            match from {
                at if at < start => shift += delta,
                at if at > end => {}
                at if start < at && at < end => inner += delta,
                _ => return RangeFate::Touched,
            }
        } else if to < start {
            shift += delta;
        } else if from > end {
        } else if start < from && to < end {
            inner += delta;
        } else if from <= start && end <= to {
            return RangeFate::Replaced;
        } else {
            return RangeFate::Touched;
        }
    }
    // Replacements before the range keep its start at or after zero, and the
    // ones inside keep its end at or after its start, so both conversions hold
    // for any edit that validated.
    let moved_start = u64::try_from(i128::from(start) + shift).unwrap_or(0);
    let moved_end = u64::try_from(i128::from(end) + shift + inner).unwrap_or(0);
    ByteRange::new(moved_start, moved_end).map_or(RangeFate::Touched, RangeFate::Moved)
}

/// Return whether a range of the after bytes lies entirely inside text the
/// edit inserted.
///
/// An absent before is an empty buffer, so all of a new unit's text is
/// inserted.
#[must_use]
pub fn inserted(edit: &SourceEdit, range: ByteRange) -> bool {
    let mut offset: i128 = 0;
    for replacement in edit.replacements() {
        let (from, to) = (replacement.range().start(), replacement.range().end());
        let length = replacement.text().len() as i128;
        let text_start = i128::from(from) + offset;
        let text_end = text_start + length;
        if length > 0
            && text_start <= i128::from(range.start())
            && i128::from(range.end()) <= text_end
        {
            return true;
        }
        offset += length - i128::from(to - from);
    }
    false
}

/// Why replaying an edit did not reproduce its after bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayGap {
    /// The bytes of a snapshot the edit names were not retained.
    SourceUnavailable,
    /// Replaying the edit gives other bytes than the after snapshot names.
    Mismatch,
}

/// Replay an edit over its retained before bytes and compare the result with
/// its retained after bytes.
pub fn replay(edit: &SourceEdit, sources: &RetainedSources<'_>) -> Result<(), ReplayGap> {
    let side = |snapshot: Option<&intlify_authoring::SourceSnapshot>| match snapshot {
        None => Ok(&[][..]),
        Some(snapshot) => sources.bytes(snapshot).ok_or(ReplayGap::SourceUnavailable),
    };
    let before = side(edit.before())?;
    let after = side(edit.after())?;
    let mut result = Vec::with_capacity(after.len());
    let mut position = 0_usize;
    for replacement in edit.replacements() {
        let (from, to) = (
            usize::try_from(replacement.range().start()).map_err(|_| ReplayGap::Mismatch)?,
            usize::try_from(replacement.range().end()).map_err(|_| ReplayGap::Mismatch)?,
        );
        result.extend_from_slice(before.get(position..from).ok_or(ReplayGap::Mismatch)?);
        result.extend_from_slice(replacement.text().as_bytes());
        position = to;
    }
    result.extend_from_slice(before.get(position..).ok_or(ReplayGap::Mismatch)?);
    if result != after {
        return Err(ReplayGap::Mismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use intlify_authoring::{
        IntegrityDigest, OwnerIdentity, OwnerKind, SourceSnapshot, VersionedIdentity,
    };
    use intlify_shared_json::encoding::digest_bytes;

    use super::*;
    use crate::registry::Replacement;

    fn snapshot(revision: &str, text: &str) -> SourceSnapshot {
        SourceSnapshot::new(
            OwnerIdentity::new(OwnerKind::Application, "storefront").unwrap(),
            "checkout",
            revision,
            VersionedIdentity::literal("intlify-grammar-js-module", "0"),
            text.len() as u64,
            IntegrityDigest::from_hash(digest_bytes(text.as_bytes())).as_str(),
        )
        .unwrap()
    }

    fn range(start: u64, end: u64) -> ByteRange {
        ByteRange::new(start, end).unwrap()
    }

    /// An edit of a 40-byte unit made of these replacements.
    fn edit(replacements: &[(u64, u64, &str)]) -> SourceEdit {
        SourceEdit::new(
            Some(snapshot("1", &"x".repeat(40))),
            None,
            replacements
                .iter()
                .map(|(start, end, text)| Replacement::new(range(*start, *end), text))
                .collect(),
        )
    }

    /// A labelled edit of the 40-byte unit, and what it does to [10, 20).
    type FateCase = (&'static str, &'static [(u64, u64, &'static str)], RangeFate);

    #[test]
    fn a_range_is_carried_only_by_replacements_clear_of_its_ends() {
        // The declaration is [10, 20).
        let declaration = range(10, 20);
        let cases: [FateCase; 14] = [
            ("nothing changed", &[], RangeFate::Moved(range(10, 20))),
            (
                "an insertion before it",
                &[(0, 0, "abc")],
                RangeFate::Moved(range(13, 23)),
            ),
            (
                "a deletion before it",
                &[(2, 5, "")],
                RangeFate::Moved(range(7, 17)),
            ),
            (
                "a replacement after it",
                &[(25, 30, "")],
                RangeFate::Moved(range(10, 20)),
            ),
            (
                "an insertion inside it",
                &[(15, 15, "ab")],
                RangeFate::Moved(range(10, 22)),
            ),
            (
                "a replacement inside it",
                &[(12, 18, "z")],
                RangeFate::Moved(range(10, 15)),
            ),
            (
                "one before, one inside, one after",
                &[(0, 2, ""), (12, 13, "yy"), (30, 30, "q")],
                RangeFate::Moved(range(8, 19)),
            ),
            (
                "an insertion at its start",
                &[(10, 10, "a")],
                RangeFate::Touched,
            ),
            (
                "an insertion at its end",
                &[(20, 20, "a")],
                RangeFate::Touched,
            ),
            (
                "a replacement ending at its start",
                &[(5, 10, "a")],
                RangeFate::Touched,
            ),
            (
                "a replacement starting at its end",
                &[(20, 25, "a")],
                RangeFate::Touched,
            ),
            (
                "a replacement across its start",
                &[(8, 12, "a")],
                RangeFate::Touched,
            ),
            (
                "a replacement of exactly it",
                &[(10, 20, "b")],
                RangeFate::Replaced,
            ),
            (
                "a replacement around it",
                &[(5, 25, "")],
                RangeFate::Replaced,
            ),
        ];
        for (label, replacements, expected) in cases {
            assert_eq!(fate(&edit(replacements), declaration), expected, "{label}");
        }
    }

    #[test]
    fn inserted_text_is_located_in_after_coordinates() {
        // "abc" goes in at 0, then 12..18 becomes "wxyz": in the after bytes
        // the first text is [0, 3) and the second [15, 19).
        let edit = edit(&[(0, 0, "abc"), (12, 18, "wxyz")]);
        assert!(inserted(&edit, range(0, 3)));
        assert!(inserted(&edit, range(16, 18)));
        assert!(
            !inserted(&edit, range(2, 5)),
            "it runs past the inserted text"
        );
        assert!(
            !inserted(&edit, range(5, 9)),
            "those bytes were there before"
        );
        // A deletion inserts nothing.
        assert!(!inserted(&self::edit(&[(4, 8, "")]), range(4, 4)));
        // A new unit is inserted text from end to end.
        let new_unit = SourceEdit::new(
            None,
            Some(snapshot("1", "intent('A')\n")),
            vec![Replacement::new(range(0, 0), "intent('A')\n")],
        );
        assert!(inserted(&new_unit, range(7, 10)));
    }

    #[test]
    fn replay_needs_the_exact_after_bytes_of_the_exact_snapshots() {
        let before = "const a = intent('Pay')\n";
        let after = "// header\nconst a = intent('Pay')\n";
        let (first, second) = (snapshot("1", before), snapshot("2", after));
        let edit = SourceEdit::new(
            Some(first.clone()),
            Some(second.clone()),
            vec![Replacement::new(range(0, 0), "// header\n")],
        );
        let both = RetainedSources::new([
            (first.clone(), before.as_bytes()),
            (second.clone(), after.as_bytes()),
        ])
        .unwrap();
        assert_eq!(replay(&edit, &both), Ok(()));

        let off_by_one = SourceEdit::new(
            Some(first.clone()),
            Some(second.clone()),
            vec![Replacement::new(range(0, 0), "// header \n")],
        );
        assert_eq!(replay(&off_by_one, &both), Err(ReplayGap::Mismatch));

        let only_before = RetainedSources::new([(first, before.as_bytes())]).unwrap();
        assert_eq!(
            replay(&edit, &only_before),
            Err(ReplayGap::SourceUnavailable)
        );

        // A removed unit replays to nothing.
        let gone = SourceEdit::new(
            Some(second.clone()),
            None,
            vec![Replacement::new(range(0, after.len() as u64), "")],
        );
        let retained = RetainedSources::new([(second, after.as_bytes())]).unwrap();
        assert_eq!(replay(&gone, &retained), Ok(()));
    }

    #[test]
    fn only_revision_0_of_the_edit_replay_profile_is_implemented() {
        assert!(is_edit_replay_profile(&VersionedIdentity::literal(
            "intlify-continuity-edit-replay",
            "0"
        )));
        assert!(!is_edit_replay_profile(&VersionedIdentity::literal(
            "intlify-continuity-edit-replay",
            "1"
        )));
        assert!(!is_edit_replay_profile(&VersionedIdentity::literal(
            "some-other-diff",
            "0"
        )));
    }
}
