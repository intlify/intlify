// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::error::Error;
use std::fmt;

use crate::LinkLimitCounter;

/// Closed grammar categories used by checked scalar and identity constructors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueGrammar {
    /// A required scalar or sequence is empty.
    Empty,
    /// A scalar does not satisfy its exact contract grammar.
    Invalid,
    /// A sequence contains a non-canonical adjacent value.
    NonCanonicalOrder,
    /// A sequence contains a forbidden duplicate value.
    Duplicate,
}

/// Construction-local structural limit categories used before an owning artifact context exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueLimitKind {
    /// One scalar or segment exceeded its fixed byte ceiling.
    Bytes,
    /// One collection exceeded its fixed item ceiling.
    Items,
    /// One complete identity exceeded its fixed aggregate byte ceiling.
    TotalBytes,
    /// One parsed key or pattern exceeded its fixed token ceiling.
    Tokens,
}

/// Exact evidence for a checked value's structural limit failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueLimit {
    kind: ValueLimitKind,
    limit: u64,
    attempted: u64,
}

impl ValueLimit {
    pub(crate) const fn new(kind: ValueLimitKind, limit: u64, attempted: u64) -> Self {
        Self {
            kind,
            limit,
            attempted,
        }
    }

    /// Return the structural resource being bounded.
    #[must_use]
    pub const fn kind(&self) -> ValueLimitKind {
        self.kind
    }

    /// Return the inclusive structural ceiling.
    #[must_use]
    pub const fn limit(&self) -> u64 {
        self.limit
    }

    /// Return the first canonical attempted value above the ceiling.
    #[must_use]
    pub const fn attempted(&self) -> u64 {
        self.attempted
    }
}

/// Range invariant rejected by a checked value constructor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValueRangeError {
    /// A UTF-8 span begins after it ends.
    ReversedUtf8Span,
}

/// Closed failure returned by common checked-value constructors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValueConstructionError {
    /// The complete submitted value violates its scalar or collection grammar.
    Grammar(ValueGrammar),
    /// A fixed structural ceiling was exceeded.
    StructuralLimit(ValueLimit),
    /// A shared linker field limit was exceeded at its protocol ceiling.
    FieldLimit {
        /// The closed counter identifying the field.
        counter: LinkLimitCounter,
        /// The inclusive protocol ceiling.
        limit: u64,
        /// The canonical first attempted value.
        attempted: u64,
    },
    /// A complete pair of values violates an ordering/range invariant.
    Range(ValueRangeError),
}

impl ValueConstructionError {
    pub(crate) const fn field_limit(counter: LinkLimitCounter, limit: u64, attempted: u64) -> Self {
        Self::FieldLimit {
            counter,
            limit,
            attempted,
        }
    }
}

impl fmt::Display for ValueConstructionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Grammar(grammar) => {
                write!(formatter, "invalid checked-value grammar: {grammar:?}")
            }
            Self::StructuralLimit(evidence) => write!(
                formatter,
                "checked-value {:?} limit {} exceeded at {}",
                evidence.kind, evidence.limit, evidence.attempted
            ),
            Self::FieldLimit {
                counter,
                limit,
                attempted,
            } => write!(
                formatter,
                "{} limit {limit} exceeded at {attempted}",
                counter.as_str()
            ),
            Self::Range(range) => write!(formatter, "invalid checked-value range: {range:?}"),
        }
    }
}

impl Error for ValueConstructionError {}
