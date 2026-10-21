// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Counts kept with checked arithmetic, and what a count proves against a
//! bound when part of what it counts may be unavailable. 015 requires checked
//! unsigned accounting: a count past `u64` is past every bound, and a bound is
//! never proved from a prefix it does not exceed.

use crate::input_limits::Bound;

use super::evaluation::{Actual, Admitted, Blocking, Causes, LimitExceeded};
use super::limits::BoundId;

/// A count kept with checked arithmetic. `None` is a count past `u64`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Tally(Option<u64>);

impl Tally {
    pub(crate) fn of(count: usize) -> Self {
        Self(u64::try_from(count).ok())
    }

    /// A count already in the `u64` the bounds are written in.
    pub(crate) const fn exactly(count: u64) -> Self {
        Self(Some(count))
    }

    pub(crate) fn plus(self, other: Self) -> Self {
        Self(
            self.0
                .zip(other.0)
                .and_then(|(left, right)| left.checked_add(right)),
        )
    }
}

pub(crate) fn total(tallies: impl IntoIterator<Item = Tally>) -> Tally {
    tallies.into_iter().fold(Tally::of(0), Tally::plus)
}

/// A total, or, past `u64`, the smallest witness that it exceeds `limit`.
pub(crate) fn actual(tally: Tally, limit: Bound) -> Actual {
    // `Bound` excludes `u64::MAX`, so the witness is representable.
    tally
        .0
        .map_or(Actual::AtLeast(limit.get() + 1), Actual::Exact)
}

/// What a count proves against `limit` while part of what it counts may be
/// unavailable: an exact total when nothing is, a witness when what is
/// visible already exceeds `limit` (the rest can only add), and otherwise
/// nothing, with the causes of the missing part.
pub(crate) fn measure(tally: Tally, unavailable: Option<Causes>, limit: Bound) -> Admitted<Actual> {
    match (actual(tally, limit), unavailable) {
        (actual, None) => Ok(actual),
        (Actual::Exact(total), Some(causes)) if total <= limit.get() => Err(causes),
        (_, Some(_)) => Ok(Actual::AtLeast(limit.get() + 1)),
    }
}

/// The bound's issue when `actual` exceeds `limit`.
pub(crate) fn within(bound: BoundId, limit: Bound, actual: Actual) -> Result<(), Blocking> {
    match actual {
        Actual::Exact(total) if total <= limit.get() => Ok(()),
        actual => Err(Blocking::limit(LimitExceeded {
            bound,
            limit,
            actual,
        })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolver::limits::Group;
    use crate::resolver::locales::fixtures::stand_in;
    use crate::resolver::specification::Slot;

    const BOUND: BoundId = BoundId {
        group: Group::LocaleResolution,
        name: "maxLocaleOccurrences",
    };

    fn bound(value: u64) -> Bound {
        Bound::new(value).unwrap()
    }

    #[test]
    fn counts_are_checked_and_one_past_u64_is_past_every_bound() {
        assert_eq!(Tally::of(2).plus(Tally::of(3)), Tally(Some(5)));
        assert_eq!(Tally::exactly(7), Tally(Some(7)));
        assert_eq!(Tally(Some(u64::MAX)).plus(Tally::of(1)), Tally(None));
        assert_eq!(Tally(None).plus(Tally::of(0)), Tally(None));
        assert_eq!(Tally::of(0).plus(Tally(None)), Tally(None));
        assert_eq!(total([]), Tally(Some(0)));
        assert_eq!(
            total([Tally::of(1), Tally::of(2), Tally::of(3)]),
            Tally(Some(6))
        );
        assert_eq!(actual(Tally(Some(9)), bound(7)), Actual::Exact(9));
        assert_eq!(actual(Tally(None), bound(7)), Actual::AtLeast(8));
    }

    #[test]
    fn a_missing_part_is_measured_only_once_what_is_visible_exceeds_the_bound() {
        let causes = stand_in(Slot::Coverage);
        assert_eq!(
            measure(Tally(Some(3)), None, bound(5)),
            Ok(Actual::Exact(3))
        );
        assert_eq!(
            measure(Tally(Some(9)), None, bound(5)),
            Ok(Actual::Exact(9))
        );
        assert_eq!(
            measure(Tally(Some(5)), Some(causes.clone()), bound(5)),
            Err(causes.clone())
        );
        assert_eq!(
            measure(Tally(Some(6)), Some(causes.clone()), bound(5)),
            Ok(Actual::AtLeast(6))
        );
        assert_eq!(
            measure(Tally(None), Some(causes), bound(5)),
            Ok(Actual::AtLeast(6))
        );
        assert_eq!(measure(Tally(None), None, bound(5)), Ok(Actual::AtLeast(6)));
    }

    #[test]
    fn a_bound_admits_its_exact_value_and_reports_the_first_over() {
        assert_eq!(within(BOUND, bound(5), Actual::Exact(5)), Ok(()));
        for actual in [Actual::Exact(6), Actual::AtLeast(6)] {
            assert_eq!(
                within(BOUND, bound(5), actual),
                Err(Blocking::limit(LimitExceeded {
                    bound: BOUND,
                    limit: bound(5),
                    actual,
                }))
            );
        }
    }
}
