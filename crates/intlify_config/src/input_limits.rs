// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Private input-accounting primitives. These are a subset of 015's capability
//! fields, not an Admitted Implementation Capability or a Resource Limit Policy.
//! Callers must supply every limit explicitly; tests own their finite values.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Bound(u64);

impl Bound {
    pub(crate) const fn new(value: u64) -> Option<Self> {
        if value == 0 || value == u64::MAX {
            None
        } else {
            Some(Self(value))
        }
    }

    pub(crate) fn parse(value: &str) -> Option<Self> {
        let bytes = value.as_bytes();
        if bytes.len() > 20
            || !bytes.first().is_some_and(|b| matches!(b, b'1'..=b'9'))
            || !bytes.iter().all(u8::is_ascii_digit)
        {
            return None;
        }
        Self::new(value.parse().ok()?)
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RawInputLimits {
    pub(crate) max_file_bytes: Bound,
    pub(crate) max_parser_tokens: Bound,
}

#[derive(Debug, Clone, Copy)]
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
