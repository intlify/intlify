// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Checked delivery-unit topology for message placement.
//!
//! This module owns canonical graph nodes, `parent -> child` edges, real-root
//! derivation, and graph-local resource admission. It does not infer a graph
//! from artifacts, platform chunks, paths, or configuration.

use std::collections::{BTreeMap, BTreeSet};

use intlify_contract::{DeliveryUnitId, LinkLimitCounter, LinkLimitSubject, LinkLimits};

use crate::validation::{arithmetic_overflow, check_exact, check_first_over, usize_count};
use crate::{DeliveryEdgeEndpoint, InvalidRequestError, LinkOperationalError};

/// One directed loading-order edge from a parent to a child.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DeliveryUnitEdge {
    parent: DeliveryUnitId,
    child: DeliveryUnitId,
}

impl DeliveryUnitEdge {
    /// Compose one edge from independently checked delivery-unit identities.
    #[must_use]
    pub const fn new(parent: DeliveryUnitId, child: DeliveryUnitId) -> Self {
        Self { parent, child }
    }

    /// Return the parent endpoint.
    #[must_use]
    pub const fn parent(&self) -> &DeliveryUnitId {
        &self.parent
    }

    /// Return the child endpoint.
    #[must_use]
    pub const fn child(&self) -> &DeliveryUnitId {
        &self.child
    }
}

/// Immutable canonical finite delivery-unit DAG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryUnitGraph {
    nodes: Box<[DeliveryUnitId]>,
    edges: Box<[DeliveryUnitEdge]>,
    roots: Box<[DeliveryUnitId]>,
}

impl DeliveryUnitGraph {
    /// Validate submitted occurrences and construct one canonical graph.
    pub fn try_new(
        nodes: Vec<DeliveryUnitId>,
        edges: Vec<DeliveryUnitEdge>,
        limits: &LinkLimits,
    ) -> Result<Self, LinkOperationalError> {
        validate_graph_limits(&nodes, &edges, limits)?;

        let mut nodes = nodes;
        nodes.sort();
        if let Some(pair) = nodes.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(InvalidRequestError::DuplicateDeliveryUnit(pair[0].clone()).into());
        }

        let known_nodes = nodes.iter().collect::<BTreeSet<_>>();
        let mut edges = edges;
        edges.sort();

        for edge in &edges {
            if !known_nodes.contains(edge.parent()) {
                return Err(InvalidRequestError::UnknownDeliveryEdgeEndpoint {
                    endpoint: DeliveryEdgeEndpoint::Parent,
                    delivery_unit: edge.parent().clone(),
                }
                .into());
            }
            if !known_nodes.contains(edge.child()) {
                return Err(InvalidRequestError::UnknownDeliveryEdgeEndpoint {
                    endpoint: DeliveryEdgeEndpoint::Child,
                    delivery_unit: edge.child().clone(),
                }
                .into());
            }
        }

        if let Some(pair) = edges.windows(2).find(|pair| pair[0] == pair[1]) {
            return Err(InvalidRequestError::DuplicateDeliveryEdge {
                parent: pair[0].parent().clone(),
                child: pair[0].child().clone(),
            }
            .into());
        }
        if let Some(edge) = edges.iter().find(|edge| edge.parent() == edge.child()) {
            return Err(InvalidRequestError::DeliverySelfEdge(edge.parent().clone()).into());
        }

        let indegrees = graph_indegrees(&nodes, &edges);
        if has_cycle(&nodes, &edges, &indegrees) {
            return Err(InvalidRequestError::DeliveryGraphCycle.into());
        }
        let roots = nodes
            .iter()
            .zip(indegrees)
            .filter(|(_, indegree)| *indegree == 0)
            .map(|(node, _)| node.clone())
            .collect::<Vec<_>>();

        Ok(Self {
            nodes: nodes.into_boxed_slice(),
            edges: edges.into_boxed_slice(),
            roots: roots.into_boxed_slice(),
        })
    }

    /// Construct the ordinary checked built-in `["main"]` graph.
    pub fn single_main(limits: &LinkLimits) -> Result<Self, LinkOperationalError> {
        Self::try_new(vec![DeliveryUnitId::main()], Vec::new(), limits)
    }

    /// Return nodes in canonical identity order.
    #[must_use]
    pub const fn nodes(&self) -> &[DeliveryUnitId] {
        &self.nodes
    }

    /// Return directed edges in canonical `(parent, child)` order.
    #[must_use]
    pub const fn edges(&self) -> &[DeliveryUnitEdge] {
        &self.edges
    }

    /// Return real indegree-zero roots in canonical node order.
    #[must_use]
    pub const fn roots(&self) -> &[DeliveryUnitId] {
        &self.roots
    }

    /// Test whether one exact delivery-unit identity is a graph node.
    #[must_use]
    pub fn contains(&self, delivery_unit: &DeliveryUnitId) -> bool {
        self.nodes.binary_search(delivery_unit).is_ok()
    }

    pub(crate) fn revalidate(&self, limits: &LinkLimits) -> Result<(), LinkOperationalError> {
        validate_graph_limits(&self.nodes, &self.edges, limits)
    }
}

fn validate_graph_limits(
    nodes: &[DeliveryUnitId],
    edges: &[DeliveryUnitEdge],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    check_first_over(
        LinkLimitCounter::DeliveryGraphNodes,
        LinkLimitSubject::DeliveryGraph,
        usize_count(nodes.len()),
        limits,
    )?;
    check_first_over(
        LinkLimitCounter::DeliveryGraphEdges,
        LinkLimitSubject::DeliveryGraph,
        usize_count(edges.len()),
        limits,
    )?;

    let mut canonical_nodes = nodes.iter().collect::<Vec<_>>();
    canonical_nodes.sort();
    for node in &canonical_nodes {
        validate_delivery_unit(node, limits)?;
    }
    validate_aggregate_id_bytes(&canonical_nodes, limits)
}

fn validate_delivery_unit(
    delivery_unit: &DeliveryUnitId,
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let subject = LinkLimitSubject::DeliveryUnitGroup(delivery_unit.clone());
    check_first_over(
        LinkLimitCounter::DeliveryUnitSegments,
        subject.clone(),
        usize_count(delivery_unit.segments().len()),
        limits,
    )?;
    for segment in delivery_unit.segments() {
        check_first_over(
            LinkLimitCounter::DeliveryUnitSegmentBytes,
            subject.clone(),
            usize_count(segment.as_str().len()),
            limits,
        )?;
    }

    let mut total = 0_u64;
    for segment in delivery_unit.segments() {
        total = total
            .checked_add(usize_count(segment.as_str().len()))
            .ok_or_else(|| {
                arithmetic_overflow(LinkLimitCounter::DeliveryUnitBytes, subject.clone(), limits)
            })?;
        check_exact(
            LinkLimitCounter::DeliveryUnitBytes,
            subject.clone(),
            total,
            limits,
        )?;
    }
    Ok(())
}

fn validate_aggregate_id_bytes(
    canonical_nodes: &[&DeliveryUnitId],
    limits: &LinkLimits,
) -> Result<(), LinkOperationalError> {
    let mut total = 0_u64;
    let mut index = 0;
    while index < canonical_nodes.len() {
        let identity = canonical_nodes[index];
        let mut group_total = 0_u64;
        while index < canonical_nodes.len() && canonical_nodes[index] == identity {
            group_total = group_total
                .checked_add(delivery_unit_bytes(canonical_nodes[index]))
                .ok_or_else(|| {
                    arithmetic_overflow(
                        LinkLimitCounter::DeliveryGraphIdBytes,
                        LinkLimitSubject::DeliveryUnitGroup(identity.clone()),
                        limits,
                    )
                })?;
            index += 1;
        }
        total = total.checked_add(group_total).ok_or_else(|| {
            arithmetic_overflow(
                LinkLimitCounter::DeliveryGraphIdBytes,
                LinkLimitSubject::DeliveryUnitGroup(identity.clone()),
                limits,
            )
        })?;
        check_exact(
            LinkLimitCounter::DeliveryGraphIdBytes,
            LinkLimitSubject::DeliveryUnitGroup(identity.clone()),
            total,
            limits,
        )?;
    }
    Ok(())
}

fn delivery_unit_bytes(delivery_unit: &DeliveryUnitId) -> u64 {
    delivery_unit
        .segments()
        .iter()
        .map(|segment| usize_count(segment.as_str().len()))
        .sum()
}

fn graph_indegrees(nodes: &[DeliveryUnitId], edges: &[DeliveryUnitEdge]) -> Vec<usize> {
    let node_indexes = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node, index))
        .collect::<BTreeMap<_, _>>();
    let mut indegrees = vec![0; nodes.len()];
    for edge in edges {
        indegrees[node_indexes[edge.child()]] += 1;
    }
    indegrees
}

fn has_cycle(nodes: &[DeliveryUnitId], edges: &[DeliveryUnitEdge], indegrees: &[usize]) -> bool {
    let node_indexes = nodes
        .iter()
        .enumerate()
        .map(|(index, node)| (node, index))
        .collect::<BTreeMap<_, _>>();
    let mut children = BTreeMap::<&DeliveryUnitId, Vec<&DeliveryUnitId>>::new();
    for edge in edges {
        children
            .entry(edge.parent())
            .or_default()
            .push(edge.child());
    }
    let mut indegrees = indegrees.to_vec();
    let mut ready = nodes
        .iter()
        .enumerate()
        .filter(|(index, _)| indegrees[*index] == 0)
        .map(|(_, node)| node.clone())
        .collect::<BTreeSet<_>>();
    let mut visited = 0;

    while let Some(node) = ready.pop_first() {
        visited += 1;
        if let Some(node_children) = children.get(&node) {
            for child in node_children {
                let child_index = node_indexes[child];
                indegrees[child_index] -= 1;
                if indegrees[child_index] == 0 {
                    ready.insert((*child).clone());
                }
            }
        }
    }
    visited != nodes.len()
}

#[cfg(test)]
mod tests {
    use intlify_contract::{
        DeliveryUnitId, DeliveryUnitSegment, LinkLimitCounter, LinkLimitObservation, LinkLimits,
    };

    use super::{DeliveryUnitEdge, DeliveryUnitGraph};
    use crate::{InvalidRequestError, LinkOperationalError};

    fn unit(name: &str) -> DeliveryUnitId {
        DeliveryUnitId::try_new(vec![DeliveryUnitSegment::try_new(name).unwrap()]).unwrap()
    }

    #[test]
    fn graph_canonicalizes_parent_child_edges_and_derives_real_roots() {
        let root = unit("root");
        let child = unit("child");
        let detached = unit("detached");
        let graph = DeliveryUnitGraph::try_new(
            vec![root.clone(), detached.clone(), child.clone()],
            vec![DeliveryUnitEdge::new(root.clone(), child.clone())],
            &LinkLimits::default(),
        )
        .unwrap();

        assert_eq!(
            graph.nodes(),
            &[child.clone(), detached.clone(), root.clone()]
        );
        assert_eq!(graph.roots(), &[detached, root]);
        assert_eq!(graph.edges()[0].parent(), &unit("root"));
        assert_eq!(graph.edges()[0].child(), &child);
    }

    #[test]
    fn graph_rejects_unknown_duplicate_self_and_cyclic_edges() {
        let a = unit("a");
        let b = unit("b");
        let unknown = unit("unknown");

        assert!(matches!(
            DeliveryUnitGraph::try_new(
                vec![a.clone()],
                vec![DeliveryUnitEdge::new(a.clone(), unknown.clone())],
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::UnknownDeliveryEdgeEndpoint { .. }
            ))
        ));
        assert!(matches!(
            DeliveryUnitGraph::try_new(
                vec![a.clone(), a.clone()],
                Vec::new(),
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::DuplicateDeliveryUnit(_)
            ))
        ));
        assert!(matches!(
            DeliveryUnitGraph::try_new(
                vec![a.clone()],
                vec![DeliveryUnitEdge::new(a.clone(), a.clone())],
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::DeliverySelfEdge(_)
            ))
        ));
        assert!(matches!(
            DeliveryUnitGraph::try_new(
                vec![a.clone(), b.clone()],
                vec![
                    DeliveryUnitEdge::new(a.clone(), b.clone()),
                    DeliveryUnitEdge::new(b, a),
                ],
                &LinkLimits::default(),
            ),
            Err(LinkOperationalError::InvalidRequest(
                InvalidRequestError::DeliveryGraphCycle
            ))
        ));
    }

    #[test]
    fn graph_limit_precedence_counts_submitted_duplicate_occurrences() {
        let a = unit("a");
        let limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::DeliveryGraphNodes, 1)
            .unwrap();
        let error =
            DeliveryUnitGraph::try_new(vec![a.clone(), a], Vec::new(), &limits).unwrap_err();
        let LinkOperationalError::Limit(evidence) = error else {
            panic!("expected limit evidence");
        };
        assert_eq!(evidence.counter(), LinkLimitCounter::DeliveryGraphNodes);
        assert_eq!(evidence.observation(), LinkLimitObservation::Exact(2));
    }

    #[test]
    fn graph_distinguishes_per_id_and_aggregate_id_byte_limits() {
        let long = unit("long");
        let per_id_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::DeliveryUnitBytes, 3)
            .unwrap();
        let per_id =
            DeliveryUnitGraph::try_new(vec![long], Vec::new(), &per_id_limits).unwrap_err();
        let LinkOperationalError::Limit(per_id) = per_id else {
            panic!("expected per-ID limit evidence");
        };
        assert_eq!(per_id.counter(), LinkLimitCounter::DeliveryUnitBytes);
        assert_eq!(per_id.observation(), LinkLimitObservation::Exact(4));

        let first = unit("aa");
        let second = unit("bb");
        let aggregate_limits = LinkLimits::default()
            .try_with_limit(LinkLimitCounter::DeliveryGraphIdBytes, 3)
            .unwrap();
        let aggregate =
            DeliveryUnitGraph::try_new(vec![second.clone(), first], Vec::new(), &aggregate_limits)
                .unwrap_err();
        let LinkOperationalError::Limit(aggregate) = aggregate else {
            panic!("expected aggregate limit evidence");
        };
        assert_eq!(aggregate.counter(), LinkLimitCounter::DeliveryGraphIdBytes);
        assert_eq!(aggregate.observation(), LinkLimitObservation::Exact(4));
        assert_eq!(
            aggregate.subject(),
            &intlify_contract::LinkLimitSubject::DeliveryUnitGroup(second)
        );
    }

    #[test]
    fn built_in_graph_uses_normal_checked_construction() {
        let graph = DeliveryUnitGraph::single_main(&LinkLimits::default()).unwrap();
        assert_eq!(graph.nodes(), &[DeliveryUnitId::main()]);
        assert_eq!(graph.roots(), &[DeliveryUnitId::main()]);
        assert!(graph.edges().is_empty());
    }
}
