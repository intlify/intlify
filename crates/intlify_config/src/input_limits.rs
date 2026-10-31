// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private input-accounting primitives. These are a subset of 015's capability
//! fields, not an Admitted Implementation Capability or a Resource Limit Policy.
//! Callers must supply every limit explicitly; tests own their finite values.

/// One `ResourceBoundValue`: a count from 1 through `u64::MAX - 1`, the domain
/// of every capability and `projectProfileResolution` bound. Its semantic form
/// is the count's eight bytes, big-endian, and JSON spells it as the count's
/// shortest decimal string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bound(u64);

/// Why a value is no `ResourceBoundValue`, as 015's capability boundary names
/// it for one bound. A Resource Limit Policy's reader refuses all three alike.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BoundFailure {
    /// Not the shortest decimal string of a count: empty, a character other
    /// than an ASCII digit, or a zero before another digit.
    NonCanonical,
    /// Zero.
    NonPositive,
    /// `u64::MAX` or more.
    Unrepresentable,
}

impl Bound {
    /// `value` as a bound, or why it cannot be one.
    pub(crate) const fn admit(value: u64) -> Result<Self, BoundFailure> {
        match value {
            0 => Err(BoundFailure::NonPositive),
            u64::MAX => Err(BoundFailure::Unrepresentable),
            value => Ok(Self(value)),
        }
    }

    pub(crate) const fn new(value: u64) -> Option<Self> {
        match Self::admit(value) {
            Ok(bound) => Some(bound),
            Err(_) => None,
        }
    }

    /// The bound a decimal string spells, judged in 015's order: the spelling,
    /// then zero, then the size. A canonical spelling too long for `u64` is
    /// unrepresentable rather than non-canonical.
    pub(crate) fn read(text: &str) -> Result<Self, BoundFailure> {
        let canonical = match text.as_bytes() {
            [] | [b'0', _, ..] => false,
            digits => digits.iter().all(u8::is_ascii_digit),
        };
        if !canonical {
            return Err(BoundFailure::NonCanonical);
        }
        // The text is a run of digits, so only overflow is left to fail.
        text.parse()
            .map_or(Err(BoundFailure::Unrepresentable), Self::admit)
    }

    pub(crate) fn parse(text: &str) -> Option<Self> {
        Self::read(text).ok()
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }

    /// The bound's semantic form.
    pub(crate) const fn to_be_bytes(self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    /// The bound whose semantic form `bytes` are, or why they are none.
    pub(crate) const fn from_be_bytes(bytes: [u8; 8]) -> Result<Self, BoundFailure> {
        Self::admit(u64::from_be_bytes(bytes))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RawInputLimits {
    pub(crate) max_file_bytes: Bound,
    pub(crate) max_parser_tokens: Bound,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[expect(clippy::struct_field_names, reason = "mirror 015's explicit bound IDs")]
pub(crate) struct ValueLimits {
    pub(crate) max_nodes: Bound,
    pub(crate) max_depth: Bound,
    pub(crate) max_collection_entries: Bound,
    pub(crate) max_total_string_bytes: Bound,
    pub(crate) max_single_string_bytes: Bound,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct InputLimits {
    pub(crate) raw: RawInputLimits,
    pub(crate) value: ValueLimits,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InputBound {
    FileBytes,
    ParserTokens,
    Nodes,
    Depth,
    CollectionEntries,
    TotalStringBytes,
    SingleStringBytes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CountRelation {
    Exact,
    AtLeast,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LimitViolation {
    pub(crate) bound: InputBound,
    pub(crate) limit: Bound,
    pub(crate) actual: u64,
    pub(crate) relation: CountRelation,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ValueCounts {
    pub(crate) nodes: u64,
    pub(crate) depth: u64,
    pub(crate) collection_entries: u64,
    pub(crate) total_string_bytes: u64,
    pub(crate) single_string_bytes: u64,
}

impl ValueLimits {
    /// Bound-centric summaries over a complete parsed domain, in 015 table order.
    /// These are not Finding records: occurrence projection is a later stage.
    pub(crate) fn violations(self, counts: ValueCounts) -> Vec<LimitViolation> {
        [
            (InputBound::Nodes, self.max_nodes, counts.nodes),
            (InputBound::Depth, self.max_depth, counts.depth),
            (
                InputBound::CollectionEntries,
                self.max_collection_entries,
                counts.collection_entries,
            ),
            (
                InputBound::TotalStringBytes,
                self.max_total_string_bytes,
                counts.total_string_bytes,
            ),
            (
                InputBound::SingleStringBytes,
                self.max_single_string_bytes,
                counts.single_string_bytes,
            ),
        ]
        .into_iter()
        .filter_map(|(bound, limit, actual)| {
            (actual > limit.get()).then_some(LimitViolation {
                bound,
                limit,
                actual,
                relation: CountRelation::Exact,
            })
        })
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bound_is_a_count_from_one_through_one_below_u64_max() {
        for value in [1, 2, u64::MAX - 1] {
            assert_eq!(Bound::admit(value).map(Bound::get), Ok(value));
            assert_eq!(Bound::new(value).map(Bound::get), Some(value));
        }
        assert_eq!(Bound::admit(0), Err(BoundFailure::NonPositive));
        assert_eq!(Bound::admit(u64::MAX), Err(BoundFailure::Unrepresentable));
        assert_eq!(Bound::new(0), None);
        assert_eq!(Bound::new(u64::MAX), None);
    }

    #[test]
    fn a_decimal_string_is_judged_by_its_spelling_then_zero_then_its_size() {
        for (text, value) in [
            ("1", 1),
            ("9", 9),
            ("10", 10),
            ("4096", 4096),
            ("18446744073709551614", u64::MAX - 1),
        ] {
            assert_eq!(Bound::read(text).map(Bound::get), Ok(value), "{text}");
        }
        for (text, failure) in [
            ("", BoundFailure::NonCanonical),
            ("00", BoundFailure::NonCanonical),
            ("01", BoundFailure::NonCanonical),
            ("+1", BoundFailure::NonCanonical),
            ("-1", BoundFailure::NonCanonical),
            (" 1", BoundFailure::NonCanonical),
            ("1 ", BoundFailure::NonCanonical),
            ("1.0", BoundFailure::NonCanonical),
            ("1e3", BoundFailure::NonCanonical),
            ("1_000", BoundFailure::NonCanonical),
            ("\u{ff11}", BoundFailure::NonCanonical),
            ("0", BoundFailure::NonPositive),
            ("18446744073709551615", BoundFailure::Unrepresentable),
            ("18446744073709551616", BoundFailure::Unrepresentable),
            ("100000000000000000000", BoundFailure::Unrepresentable),
            // The spelling decides before the size.
            ("018446744073709551615", BoundFailure::NonCanonical),
        ] {
            assert_eq!(Bound::read(text), Err(failure), "{text:?}");
        }
    }

    #[test]
    fn parse_keeps_the_bound_and_drops_the_failure() {
        assert_eq!(Bound::parse("42").map(Bound::get), Some(42));
        for text in ["", "0", "01", "18446744073709551615"] {
            assert_eq!(Bound::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn the_semantic_form_is_the_counts_eight_bytes_big_endian() {
        let bound = Bound::new(0x0102_0304_0506_0708).unwrap();
        assert_eq!(bound.to_be_bytes(), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(Bound::from_be_bytes([1, 2, 3, 4, 5, 6, 7, 8]), Ok(bound));
        let largest = [0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xfe];
        assert_eq!(
            Bound::from_be_bytes(largest).map(Bound::get),
            Ok(u64::MAX - 1)
        );
        assert_eq!(Bound::from_be_bytes([0; 8]), Err(BoundFailure::NonPositive));
        assert_eq!(
            Bound::from_be_bytes([0xff; 8]),
            Err(BoundFailure::Unrepresentable)
        );
    }
}
