// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Exact JSON numbers, as Draft 7's numeric keywords read them.
//!
//! Draft 7 compares numbers by their mathematical values (core 4.2.3), and a
//! JSON number may be written with any number of digits and any exponent. The
//! shared reader keeps each number's text, so revision `"0"` reads that text
//! as a decimal: a sign, the significant digits without a leading or trailing
//! zero, and the exponent of the last digit, an integer of any size. Equal
//! numbers read to one decimal however they are written, so comparisons, the
//! integer test, and the counts of the length keywords are exact.
//!
//! `multipleOf` divides exactly as well. Write the number as `a * 10^p` and
//! the divisor as `b * 10^q`, with `a` and `b` their significant digits, and
//! let `k = p - q`. The number is a multiple when it is zero, or when `k` is
//! not negative and `b` divides `a * 10^k`; a negative `k` would ask `a`,
//! which does not end in a zero, to be divisible by ten. `b` has fewer than
//! `4 * len(b)` factors of two and fewer of five, so raising `k` past that
//! changes nothing, and the remainder is read digit by digit over at most
//! `len(a) + 4 * len(b)` digits. One capacity keeps that linear in the
//! number's length: a divisor has at most [`MAX_DIVISOR_DIGITS`] significant
//! digits, so every remainder fits in 64 bits.

use std::cmp::Ordering;
use std::fmt;

/// The most significant digits a `multipleOf` divisor may have.
pub(crate) const MAX_DIVISOR_DIGITS: usize = 19;

/// A JSON number's exact value: `digits * 10^exponent`, below zero when
/// `negative`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Decimal {
    /// Whether the number is below zero; never for zero.
    negative: bool,
    /// The ASCII significant digits, the first and the last not zero; none
    /// for zero.
    digits: Vec<u8>,
    /// The exponent of the last digit; zero for zero.
    exponent: Integer,
}

/// A `multipleOf` divisor: a positive number of at most
/// [`MAX_DIVISOR_DIGITS`] significant digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Divisor {
    /// The significant digits.
    significand: u64,
    /// The exponent of the last digit.
    exponent: Integer,
    /// `4 * len(b)`: more factors of ten than `b` has of two or of five.
    cover: u64,
}

/// Why a number is not a `multipleOf` divisor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DivisorFailure {
    /// The number is zero or below it, which Draft 7's meta-schema refuses.
    NotPositive,
    /// The number has more than [`MAX_DIVISOR_DIGITS`] significant digits.
    Digits,
}

impl Decimal {
    /// Read a JSON number's text: an optional minus sign, an integer part
    /// without a leading zero, an optional fraction, and an optional exponent.
    ///
    /// Returns `None` when the text is not a JSON number.
    pub(crate) fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        let negative = bytes.first() == Some(&b'-');
        let mut index = usize::from(negative);
        let whole_start = index;
        match bytes.get(index) {
            Some(b'0') => index += 1,
            Some(b'1'..=b'9') => index = skip_digits(bytes, index),
            _ => return None,
        }
        let whole = &bytes[whole_start..index];
        let mut fraction: &[u8] = &[];
        if bytes.get(index) == Some(&b'.') {
            let start = index + 1;
            index = skip_digits(bytes, start);
            if index == start {
                return None;
            }
            fraction = &bytes[start..index];
        }
        let mut written = Integer::zero();
        if matches!(bytes.get(index), Some(b'e' | b'E')) {
            index += 1;
            let below = bytes.get(index) == Some(&b'-');
            if matches!(bytes.get(index), Some(b'-' | b'+')) {
                index += 1;
            }
            let start = index;
            index = skip_digits(bytes, start);
            if index == start {
                return None;
            }
            written = Integer::from_digits(below, &bytes[start..index]);
        }
        if index != bytes.len() {
            return None;
        }
        let significand: Vec<u8> = whole.iter().chain(fraction).copied().collect();
        let Some(first) = significand.iter().position(|digit| *digit != b'0') else {
            return Some(Self::zero());
        };
        let last = significand
            .iter()
            .rposition(|digit| *digit != b'0')
            .unwrap_or(first);
        let trailing = significand.len() - 1 - last;
        // The last kept digit sits `trailing` places above the last written
        // one, which sits `fraction.len()` places below the written exponent.
        let shift = if trailing >= fraction.len() {
            Integer::from_count(false, trailing - fraction.len())
        } else {
            Integer::from_count(true, fraction.len() - trailing)
        };
        Some(Self {
            negative,
            digits: significand[first..=last].to_vec(),
            exponent: written.plus(&shift),
        })
    }

    fn zero() -> Self {
        Self {
            negative: false,
            digits: Vec::new(),
            exponent: Integer::zero(),
        }
    }

    /// Whether the number has no fractional part, as Draft 7's `integer`
    /// asks.
    pub(crate) fn is_integer(&self) -> bool {
        self.digits.is_empty() || !self.exponent.is_negative()
    }

    /// The number as a count of the length keywords, when it is a
    /// non-negative integer. A count past [`u64::MAX`] reads as `u64::MAX`,
    /// which no length reaches either.
    pub(crate) fn to_count(&self) -> Option<u64> {
        if self.negative || self.exponent.is_negative() {
            return None;
        }
        if self.digits.is_empty() {
            return Some(0);
        }
        let zeros = match self.exponent.to_u64() {
            Some(zeros) if zeros < 20 => zeros,
            _ => return Some(u64::MAX),
        };
        let mut count = 0_u64;
        let digits = self.digits.iter().map(|digit| u64::from(digit - b'0'));
        for digit in digits.chain((0..zeros).map(|_| 0)) {
            match count
                .checked_mul(10)
                .and_then(|count| count.checked_add(digit))
            {
                Some(next) => count = next,
                None => return Some(u64::MAX),
            }
        }
        Some(count)
    }

    /// The number as a `multipleOf` divisor.
    pub(crate) fn divisor(&self) -> Result<Divisor, DivisorFailure> {
        if self.negative || self.digits.is_empty() {
            return Err(DivisorFailure::NotPositive);
        }
        if self.digits.len() > MAX_DIVISOR_DIGITS {
            return Err(DivisorFailure::Digits);
        }
        let significand = self
            .digits
            .iter()
            .fold(0_u64, |value, digit| value * 10 + u64::from(digit - b'0'));
        Ok(Divisor {
            significand,
            exponent: self.exponent.clone(),
            cover: 4 * self.digits.len() as u64,
        })
    }

    /// Whether dividing the number by `divisor` leaves an integer.
    pub(crate) fn is_multiple_of(&self, divisor: &Divisor) -> bool {
        if self.digits.is_empty() {
            return true;
        }
        let shift = self.exponent.plus(&divisor.exponent.negated());
        if shift.is_negative() {
            return false;
        }
        let zeros = shift
            .to_u64()
            .map_or(divisor.cover, |shift| shift.min(divisor.cover));
        let modulus = u128::from(divisor.significand);
        let mut remainder = 0_u128;
        for digit in &self.digits {
            remainder = (remainder * 10 + u128::from(digit - b'0')) % modulus;
        }
        for _ in 0..zeros {
            remainder = remainder * 10 % modulus;
        }
        remainder == 0
    }

    /// -1, 0, or 1 as the number is below, at, or above zero.
    fn signum(&self) -> i8 {
        match (self.digits.is_empty(), self.negative) {
            (true, _) => 0,
            (false, true) => -1,
            (false, false) => 1,
        }
    }

    /// The exponent of the place just above the first digit.
    fn order(&self) -> Integer {
        self.exponent
            .plus(&Integer::from_count(false, self.digits.len()))
    }
}

impl Ord for Decimal {
    fn cmp(&self, other: &Self) -> Ordering {
        let sign = self.signum().cmp(&other.signum());
        if sign != Ordering::Equal || self.digits.is_empty() {
            return sign;
        }
        // Without a trailing zero, the longer of two digit strings that agree
        // on the shorter is the larger.
        let magnitude = self
            .order()
            .cmp(&other.order())
            .then_with(|| self.digits.cmp(&other.digits));
        if self.negative {
            magnitude.reverse()
        } else {
            magnitude
        }
    }
}

impl PartialOrd for Decimal {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// The canonical text: the sign, the significant digits, and the exponent
/// when it is not zero, so `1.50` is `15e-1` and `-0.0` is `0`. It is one
/// text for each value, and a JSON number.
impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.digits.is_empty() {
            return formatter.write_str("0");
        }
        if self.negative {
            formatter.write_str("-")?;
        }
        formatter.write_str(std::str::from_utf8(&self.digits).map_err(|_| fmt::Error)?)?;
        if self.exponent.magnitude.is_empty() {
            Ok(())
        } else {
            write!(formatter, "e{}", self.exponent)
        }
    }
}

/// The index just past the ASCII digits from `start`.
fn skip_digits(bytes: &[u8], start: usize) -> usize {
    start
        + bytes[start..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count()
}

/// An integer of any size, as an exponent may be written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Integer {
    /// Whether the integer is below zero; never for zero.
    negative: bool,
    /// The ASCII digits of the magnitude, the most significant first and not
    /// zero; none for zero.
    magnitude: Vec<u8>,
}

impl Integer {
    fn zero() -> Self {
        Self {
            negative: false,
            magnitude: Vec::new(),
        }
    }

    /// The integer `digits` spell, leading zeros and all, below zero when
    /// `negative`.
    fn from_digits(negative: bool, digits: &[u8]) -> Self {
        let first = digits
            .iter()
            .position(|digit| *digit != b'0')
            .unwrap_or(digits.len());
        let magnitude = digits[first..].to_vec();
        Self {
            negative: negative && !magnitude.is_empty(),
            magnitude,
        }
    }

    fn from_count(negative: bool, count: usize) -> Self {
        Self::from_digits(negative, count.to_string().as_bytes())
    }

    fn is_negative(&self) -> bool {
        self.negative
    }

    fn negated(&self) -> Self {
        Self {
            negative: !self.negative && !self.magnitude.is_empty(),
            magnitude: self.magnitude.clone(),
        }
    }

    fn plus(&self, other: &Self) -> Self {
        if self.negative == other.negative {
            return Self {
                negative: self.negative,
                magnitude: add_magnitudes(&self.magnitude, &other.magnitude),
            };
        }
        match compare_magnitudes(&self.magnitude, &other.magnitude) {
            Ordering::Equal => Self::zero(),
            Ordering::Greater => Self {
                negative: self.negative,
                magnitude: subtract_magnitudes(&self.magnitude, &other.magnitude),
            },
            Ordering::Less => Self {
                negative: other.negative,
                magnitude: subtract_magnitudes(&other.magnitude, &self.magnitude),
            },
        }
    }

    /// The integer as a `u64`, when it is not negative and fits.
    fn to_u64(&self) -> Option<u64> {
        if self.negative {
            return None;
        }
        self.magnitude.iter().try_fold(0_u64, |value, digit| {
            value.checked_mul(10)?.checked_add(u64::from(digit - b'0'))
        })
    }
}

impl Ord for Integer {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self.negative, other.negative) {
            (false, true) => Ordering::Greater,
            (true, false) => Ordering::Less,
            (false, false) => compare_magnitudes(&self.magnitude, &other.magnitude),
            (true, true) => compare_magnitudes(&other.magnitude, &self.magnitude),
        }
    }
}

impl PartialOrd for Integer {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Integer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.magnitude.is_empty() {
            return formatter.write_str("0");
        }
        if self.negative {
            formatter.write_str("-")?;
        }
        formatter.write_str(std::str::from_utf8(&self.magnitude).map_err(|_| fmt::Error)?)
    }
}

/// Compare two magnitudes without a leading zero: the longer is the larger.
fn compare_magnitudes(left: &[u8], right: &[u8]) -> Ordering {
    left.len().cmp(&right.len()).then_with(|| left.cmp(right))
}

fn add_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut sum = Vec::with_capacity(left.len().max(right.len()) + 1);
    let mut carry = 0_u8;
    let mut left = left.iter().rev();
    let mut right = right.iter().rev();
    loop {
        let (a, b) = (left.next(), right.next());
        if a.is_none() && b.is_none() && carry == 0 {
            break;
        }
        let digit = carry + a.map_or(0, |a| a - b'0') + b.map_or(0, |b| b - b'0');
        sum.push(b'0' + digit % 10);
        carry = digit / 10;
    }
    sum.reverse();
    sum
}

/// `left - right`, for `left` at least `right`.
fn subtract_magnitudes(left: &[u8], right: &[u8]) -> Vec<u8> {
    let mut difference = Vec::with_capacity(left.len());
    let mut borrow = 0_u8;
    let mut right = right.iter().rev();
    for a in left.iter().rev() {
        let subtrahend = borrow + right.next().map_or(0, |b| b - b'0');
        let minuend = a - b'0';
        if minuend >= subtrahend {
            difference.push(b'0' + minuend - subtrahend);
            borrow = 0;
        } else {
            difference.push(b'0' + minuend + 10 - subtrahend);
            borrow = 1;
        }
    }
    while difference.last() == Some(&b'0') {
        difference.pop();
    }
    difference.reverse();
    difference
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decimal(text: &str) -> Decimal {
        Decimal::parse(text).unwrap_or_else(|| panic!("{text:?} is a JSON number"))
    }

    fn integer(text: &str) -> Integer {
        match text.strip_prefix('-') {
            Some(digits) => Integer::from_digits(true, digits.as_bytes()),
            None => Integer::from_digits(false, text.as_bytes()),
        }
    }

    fn divisor(text: &str) -> Divisor {
        decimal(text)
            .divisor()
            .unwrap_or_else(|failure| panic!("{text:?} is not a divisor: {failure:?}"))
    }

    #[test]
    fn a_number_reads_to_its_significant_digits_and_the_exponent_of_the_last() {
        for (text, canonical) in [
            ("0", "0"),
            ("-0", "0"),
            ("-0.0e-5", "0"),
            ("0e99999999999999999999", "0"),
            ("1", "1"),
            ("-7", "-7"),
            ("10", "1e1"),
            ("1.50", "15e-1"),
            ("0.001", "1e-3"),
            ("0.0100", "1e-2"),
            ("-12.3400e-2", "-1234e-4"),
            ("1E+2", "1e2"),
            ("1e-0", "1"),
            ("100e-2", "1"),
            ("2.5E-3", "25e-4"),
            ("120e-1", "12"),
            ("1.2e1", "12"),
            (
                "123456789012345678901234567890",
                "12345678901234567890123456789e1",
            ),
            ("1e00000000000000000000000000001", "1e1"),
            ("7e99999999999999999999999", "7e99999999999999999999999"),
            ("25e-99999999999999999999999", "25e-99999999999999999999999"),
            ("1.5e99999999999999999999999", "15e99999999999999999999998"),
            ("10e-99999999999999999999999", "1e-99999999999999999999998"),
            ("0.5e-9", "5e-10"),
        ] {
            assert_eq!(decimal(text).to_string(), canonical, "{text:?}");
            // The canonical text is a JSON number that reads to the same value.
            assert_eq!(decimal(canonical), decimal(text), "{text:?}");
        }
        let negative = decimal("-0.25");
        assert!(negative.negative);
        assert_eq!(negative.digits, b"25");
        assert_eq!(negative.exponent, integer("-2"));
        let zero = decimal("-0.000");
        assert!(!zero.negative);
        assert!(zero.digits.is_empty());
        assert_eq!(zero.exponent, Integer::zero());
    }

    #[test]
    fn a_text_that_is_not_a_json_number_reads_to_nothing() {
        for text in [
            "",
            "-",
            "+1",
            "01",
            "00",
            "-01",
            "1.",
            ".1",
            "-.1",
            "1.e1",
            "1e",
            "1e+",
            "1e-",
            "1E",
            "1ee1",
            "1e1e1",
            "1e1.5",
            "1e+-1",
            "0x1",
            "1 ",
            " 1",
            "1\n",
            "NaN",
            "Infinity",
            "-Infinity",
            "1_000",
            "1,5",
            "--1",
            "\u{661}",
            "1\u{661}",
            "1.5.5",
            "e1",
            "-e1",
        ] {
            assert_eq!(Decimal::parse(text), None, "{text:?}");
        }
    }

    #[test]
    fn numbers_compare_by_their_values() {
        let ascending = [
            "-1e100000000000000000000",
            "-2e99999999999999999999",
            "-1e99999999999999999999",
            "-1e400",
            "-11",
            "-10",
            "-2",
            "-1.5",
            "-1.05",
            "-1",
            "-0.5",
            "-1e-400",
            "-1e-99999999999999999999",
            "0",
            "1e-99999999999999999999",
            "1e-400",
            "0.1",
            "0.15",
            "0.2",
            "1",
            "1.000000000000000000001",
            "1.05",
            "1.5",
            "2",
            "10",
            "11",
            "99",
            "1e2",
            "1e400",
            "1e99999999999999999999",
            "2e99999999999999999999",
            "1e100000000000000000000",
        ];
        let values: Vec<Decimal> = ascending.iter().map(|text| decimal(text)).collect();
        for (i, left) in values.iter().enumerate() {
            for (j, right) in values.iter().enumerate() {
                assert_eq!(
                    left.cmp(right),
                    i.cmp(&j),
                    "{} and {}",
                    ascending[i],
                    ascending[j]
                );
            }
        }
        for equal in [
            ["1", "1.0", "10e-1", "0.1e1"],
            ["0", "-0", "0e7", "-0.000"],
            ["-25e-3", "-0.025", "-2.50e-2", "-0.0000025e4"],
        ] {
            for left in equal {
                for right in equal {
                    assert_eq!(decimal(left).cmp(&decimal(right)), Ordering::Equal);
                    assert_eq!(decimal(left), decimal(right));
                }
            }
        }
        assert_eq!(
            decimal("1").partial_cmp(&decimal("2")),
            Some(Ordering::Less)
        );
    }

    #[test]
    fn an_integer_has_no_fractional_part() {
        for text in [
            "0",
            "-0.0",
            "1",
            "1.0",
            "1e2",
            "10e-1",
            "1.5e1",
            "-3",
            "1e99999999999999999999",
        ] {
            assert!(decimal(text).is_integer(), "{text:?}");
        }
        for text in [
            "0.5",
            "1.5",
            "1e-1",
            "15e-2",
            "-0.1",
            "1e-99999999999999999999",
            "1.0000000000000000001",
        ] {
            assert!(!decimal(text).is_integer(), "{text:?}");
        }
    }

    #[test]
    fn a_count_is_a_non_negative_integer_saturating_at_the_largest_u64() {
        for (text, count) in [
            ("0", 0),
            ("-0", 0),
            ("5", 5),
            ("5.0", 5),
            ("1e3", 1000),
            ("1e19", 10_000_000_000_000_000_000),
            ("1844674407370955161e1", 18_446_744_073_709_551_610),
            ("18446744073709551615", u64::MAX),
            ("1.8446744073709551615e19", u64::MAX),
            ("18446744073709551616", u64::MAX),
            ("1844674407370955162e1", u64::MAX),
            ("2e19", u64::MAX),
            ("1e20", u64::MAX),
            ("123456789012345678901234567890", u64::MAX),
            ("1e99999999999999999999", u64::MAX),
        ] {
            assert_eq!(decimal(text).to_count(), Some(count), "{text:?}");
        }
        for text in ["-1", "0.5", "1e-1", "-0.5", "-1e99999999999999999999"] {
            assert_eq!(decimal(text).to_count(), None, "{text:?}");
        }
    }

    #[test]
    fn a_divisor_is_positive_with_at_most_nineteen_significant_digits() {
        for (text, significand, exponent) in [
            ("1", 1, "0"),
            ("0.01", 1, "-2"),
            ("2.50", 25, "-1"),
            ("1e-99999999999999999999", 1, "-99999999999999999999"),
            ("9999999999999999999", 9_999_999_999_999_999_999, "0"),
            ("1234567890123456789e5", 1_234_567_890_123_456_789, "5"),
            ("12345678901234567890", 1_234_567_890_123_456_789, "1"),
        ] {
            let divisor = divisor(text);
            assert_eq!(divisor.significand, significand, "{text:?}");
            assert_eq!(divisor.exponent, integer(exponent), "{text:?}");
        }
        assert_eq!(divisor("7").cover, 4);
        assert_eq!(divisor("9999999999999999999").cover, 76);
        for (text, failure) in [
            ("0", DivisorFailure::NotPositive),
            ("-0", DivisorFailure::NotPositive),
            ("-1", DivisorFailure::NotPositive),
            ("-0.5", DivisorFailure::NotPositive),
            ("12345678901234567891", DivisorFailure::Digits),
            ("1.0000000000000000001", DivisorFailure::Digits),
            ("-12345678901234567891", DivisorFailure::NotPositive),
        ] {
            assert_eq!(decimal(text).divisor(), Err(failure), "{text:?}");
        }
    }

    #[test]
    fn a_multiple_leaves_an_integer_when_divided() {
        for (number, by, multiple) in [
            ("0", "7", true),
            ("-0.0", "0.3", true),
            ("10", "5", true),
            ("10", "3", false),
            ("0.3", "0.1", true),
            ("0.0075", "0.0001", true),
            ("0.00751", "0.0001", false),
            ("1e308", "1e-308", true),
            ("3e1000", "1.5", true),
            ("1", "0.3", false),
            ("0.6", "0.3", true),
            ("-4.5", "1.5", true),
            ("-4.4", "1.5", false),
            ("1e-5", "1e-4", false),
            ("1e-4", "1e-5", true),
            ("1", "1e-99999999999999999999", true),
            ("3", "1e-99999999999999999999", true),
            ("1e99999999999999999999", "3", false),
            ("3e99999999999999999999", "3", true),
            ("2e99999999999999999999", "1024", true),
            ("1e9", "1024", false),
            ("1e10", "1024", true),
            ("9999999999999999999e5", "9999999999999999999", true),
            ("19999999999999999998", "9999999999999999999", true),
            ("19999999999999999997", "9999999999999999999", false),
            ("12345678901234567890", "3", true),
            ("12345678901234567891", "3", false),
            ("1e-99999999999999999999", "1e-99999999999999999998", false),
        ] {
            assert_eq!(
                decimal(number).is_multiple_of(&divisor(by)),
                multiple,
                "{number:?} by {by:?}"
            );
        }
    }

    #[test]
    fn the_cover_holds_every_factor_of_two_and_of_five_a_divisor_has() {
        // 2^63 and 5^27 have nineteen digits: the most factors of two and of
        // five a divisor can have.
        for (by, factors) in [("9223372036854775808", 63), ("7450580596923828125", 27)] {
            let divisor = divisor(by);
            assert!(decimal(&format!("1e{factors}")).is_multiple_of(&divisor));
            assert!(!decimal(&format!("1e{}", factors - 1)).is_multiple_of(&divisor));
            assert!(decimal("1e99999999999999999999").is_multiple_of(&divisor));
        }
        // Past the cover, more factors of ten are not read: a shift as large
        // as `u64::MAX` costs four zeros per digit of the divisor.
        assert!(!decimal("1e18446744073709551615").is_multiple_of(&divisor("3")));
        assert!(decimal("3e18446744073709551615").is_multiple_of(&divisor("3")));
    }

    #[test]
    fn a_remainder_reads_every_digit_of_a_long_number() {
        let power = format!("1{}", "0".repeat(100_000));
        assert!(decimal(&power).is_multiple_of(&divisor("1e5")));
        assert!(decimal(&power).is_multiple_of(&divisor("1024")));
        // 10^100000 = 10^4 (mod 7), since 10^6 = 1 (mod 7).
        assert!(!decimal(&power).is_multiple_of(&divisor("7")));
        assert!(decimal(&format!("{power}e-4")).is_multiple_of(&divisor("1e99996")));
        let sevens = "7".repeat(100_000);
        assert!(decimal(&sevens).is_multiple_of(&divisor("7")));
        assert!(!decimal(&format!("{sevens}1")).is_multiple_of(&divisor("7")));
    }

    #[test]
    fn an_integer_of_any_size_adds_and_compares() {
        for (left, right, sum) in [
            ("0", "0", "0"),
            ("0", "-5", "-5"),
            ("9", "1", "10"),
            ("999", "1", "1000"),
            ("-999", "-1", "-1000"),
            ("1000", "-1", "999"),
            ("-1000", "1", "-999"),
            ("1", "-1000", "-999"),
            ("-1", "1000", "999"),
            ("5", "-5", "0"),
            ("-5", "5", "0"),
            ("12345678901234567890", "-12345678901234567891", "-1"),
            ("100000000000000000000", "-1", "99999999999999999999"),
            ("10", "-9", "1"),
        ] {
            assert_eq!(
                integer(left).plus(&integer(right)),
                integer(sum),
                "{left} + {right}"
            );
            assert_eq!(integer(left).plus(&integer(right)).to_string(), sum);
        }
        assert_eq!(integer("-0"), Integer::zero());
        assert_eq!(integer("-0").negated(), Integer::zero());
        assert_eq!(integer("0007"), integer("7"));
        assert_eq!(integer("5").negated(), integer("-5"));
        assert_eq!(integer("-5").negated(), integer("5"));
        let ascending = ["-100", "-99", "-10", "-1", "0", "1", "9", "10", "99", "100"];
        for (i, left) in ascending.iter().enumerate() {
            for (j, right) in ascending.iter().enumerate() {
                assert_eq!(
                    integer(left).cmp(&integer(right)),
                    i.cmp(&j),
                    "{left} {right}"
                );
            }
        }
        assert_eq!(
            integer("1").partial_cmp(&integer("2")),
            Some(Ordering::Less)
        );
        assert_eq!(integer("18446744073709551615").to_u64(), Some(u64::MAX));
        assert_eq!(integer("18446744073709551616").to_u64(), None);
        assert_eq!(integer("0").to_u64(), Some(0));
        assert_eq!(integer("-1").to_u64(), None);
        assert_eq!(Integer::from_count(true, 0), Integer::zero());
        assert_eq!(Integer::from_count(true, 12), integer("-12"));
    }
}
