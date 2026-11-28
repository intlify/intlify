// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The graph-admission Handoff Vectors for 020: whether the Delivery Unit
//! Graph artifacts a compiler transaction submits for its selected group
//! are admitted, and each admitted graph's roots, under the rules 015 fixes
//! (D15:2046-2063).
//!
//! Graph admission happens after one Deployment Compatibility Group is
//! selected. The finite non-empty graph set must partition the group's
//! members exactly: each graph applies to a non-empty subset of the group,
//! the subsets are pairwise disjoint, and their union is every member, so
//! one graph may serve several targets and the targets of a hydration
//! relation may use different graphs (D15:2055-2057). In each graph, units
//! and edges are duplicate-free, every endpoint names a unit, and self-edges
//! and cycles are invalid, while several roots and disconnected acyclic
//! components are valid (D15:2061). Every message-reference occurrence that
//! applies to a graph's targets binds to exactly one existing unit, never to
//! an implicit, nearest, or root unit (D15:2063).
//!
//! Each vector hands over `delivery`, whose Delivery Graph Specification
//! fixes what unit identity and an edge mean, and `deploymentGroups`, which
//! give the selected group's members; neither references a binding. Its
//! consumer inputs are the selected Group ID, the graphs, each with the
//! members of 017's Delivery Unit Graph body that these rules read (D17:740),
//! and each member's applicable reference occurrences, and its relationship
//! is each admitted graph's targets and roots, the units without an incoming
//! edge in their canonical order, or the one rule the graphs break. 020 owns
//! the form of both (D15:3330), which these spell in 015's words until it
//! defines it.

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{json, Map, Value};

/// The decisions every graph-admission vector rests on: the realized graph
/// as a transaction input beside the profile's delivery specifications, and
/// graph applicability partitioning the selected group.
pub(super) const DECISIONS: [&str; 2] = ["015-033", "015-034"];

/// One submitted graph: its identity, the targets it applies to, its units
/// in canonical order, its edges between unit ordinals, and its reference
/// bindings.
pub(super) struct Graph {
    pub(super) identity: &'static str,
    pub(super) targets: &'static [&'static str],
    pub(super) units: &'static [&'static [&'static str]],
    pub(super) edges: &'static [(u64, u64)],
    pub(super) bindings: &'static [(&'static str, u64)],
}

/// One vector: its name, whether the storefront with two groups produces
/// it, the selected group, the graphs, each member's applicable reference
/// occurrences, and the rule the graphs break, if any, as the table states
/// it.
pub(super) struct Vector {
    pub(super) name: &'static str,
    pub(super) two_groups: bool,
    pub(super) group: &'static str,
    pub(super) graphs: &'static [Graph],
    pub(super) references: &'static [(&'static str, &'static [&'static str])],
    pub(super) rejected: Option<&'static str>,
}

/// The references `browser` and `server` apply in the storefront's `web`.
const WEB_REFERENCES: &[(&str, &[&str])] = &[
    ("browser", &["cart-title", "checkout-title"]),
    ("server", &["checkout-title"]),
];

/// The reference `worker` applies in the two-group storefront's `jobs`.
const JOBS_REFERENCES: &[(&str, &[&str])] = &[("worker", &["digest-subject"])];

/// The whole-program graph for both of `web`'s targets.
const MAIN: Graph = Graph {
    identity: "storefront-main",
    targets: &["browser", "server"],
    units: &[&["main"]],
    edges: &[],
    bindings: &[("cart-title", 0), ("checkout-title", 0)],
};

/// The browser's graph: a checkout unit loadable after the app unit.
const BROWSER: Graph = Graph {
    identity: "storefront-browser",
    targets: &["browser"],
    units: &[&["app"], &["app", "checkout"]],
    edges: &[(0, 1)],
    bindings: &[("cart-title", 0), ("checkout-title", 1)],
};

/// The server's whole-program graph.
const SERVER: Graph = Graph {
    identity: "storefront-server",
    targets: &["server"],
    units: &[&["main"]],
    edges: &[],
    bindings: &[("checkout-title", 0)],
};

/// A graph for both of `web`'s targets with the browser's units and these
/// edges and bindings.
const fn web(edges: &'static [(u64, u64)], bindings: &'static [(&'static str, u64)]) -> Graph {
    Graph {
        identity: "storefront-web",
        targets: &["browser", "server"],
        units: &[&["app"], &["app", "checkout"]],
        edges,
        bindings,
    }
}

/// A vector over the storefront's one group `web`.
const fn on_web(
    name: &'static str,
    graphs: &'static [Graph],
    rejected: Option<&'static str>,
) -> Vector {
    Vector {
        name,
        two_groups: false,
        group: "web",
        graphs,
        references: WEB_REFERENCES,
        rejected,
    }
}

/// Each conclusion 015's graph rules give: admitted graph sets over `web`,
/// whose `browser` and `server` form a hydration relation, and over the
/// two-group storefront's `jobs`, which holds `worker`, and one vector for
/// each rule a graph set breaks.
pub(super) const VECTORS: [Vector; 18] = [
    on_web("one-graph-for-the-group", &[MAIN], None),
    on_web(
        "hydration-pair-with-different-graphs",
        &[BROWSER, SERVER],
        None,
    ),
    on_web(
        "several-roots-and-components",
        &[Graph {
            identity: "storefront-split",
            targets: &["browser", "server"],
            units: &[&["cart"], &["checkout"], &["checkout", "payment"]],
            edges: &[(1, 2)],
            bindings: &[("cart-title", 0), ("checkout-title", 2)],
        }],
        None,
    ),
    on_web("no-graph", &[], Some("no-graph")),
    on_web(
        "empty-applicability",
        &[
            MAIN,
            Graph {
                identity: "storefront-unused",
                targets: &[],
                units: &[&["main"]],
                edges: &[],
                bindings: &[],
            },
        ],
        Some("empty-applicability"),
    ),
    on_web(
        "unknown-target",
        &[Graph {
            identity: "storefront-main",
            targets: &["browser", "kiosk", "server"],
            units: &[&["main"]],
            edges: &[],
            bindings: &[("cart-title", 0), ("checkout-title", 0)],
        }],
        Some("target-outside-group"),
    ),
    on_web(
        "overlapping-applicability",
        &[MAIN, BROWSER],
        Some("overlapping-applicability"),
    ),
    on_web("uncovered-target", &[BROWSER], Some("uncovered-target")),
    on_web(
        "duplicate-node",
        &[Graph {
            identity: "storefront-main",
            targets: &["browser", "server"],
            units: &[&["main"], &["main"]],
            edges: &[],
            bindings: &[("cart-title", 0), ("checkout-title", 0)],
        }],
        Some("duplicate-node"),
    ),
    on_web(
        "duplicate-edge",
        &[web(
            &[(0, 1), (0, 1)],
            &[("cart-title", 0), ("checkout-title", 1)],
        )],
        Some("duplicate-edge"),
    ),
    on_web(
        "unknown-endpoint",
        &[web(&[(0, 2)], &[("cart-title", 0), ("checkout-title", 1)])],
        Some("unknown-endpoint"),
    ),
    on_web(
        "self-edge",
        &[web(
            &[(0, 1), (1, 1)],
            &[("cart-title", 0), ("checkout-title", 1)],
        )],
        Some("self-edge"),
    ),
    on_web(
        "cycle",
        &[web(
            &[(0, 1), (1, 0)],
            &[("cart-title", 0), ("checkout-title", 1)],
        )],
        Some("cycle"),
    ),
    on_web(
        "unknown-unit",
        &[web(&[(0, 1)], &[("cart-title", 0), ("checkout-title", 2)])],
        Some("unknown-unit"),
    ),
    on_web(
        "multiply-bound-reference",
        &[web(
            &[(0, 1)],
            &[
                ("cart-title", 0),
                ("checkout-title", 0),
                ("checkout-title", 1),
            ],
        )],
        Some("multiply-bound-reference"),
    ),
    on_web(
        "unbound-reference",
        &[web(&[(0, 1)], &[("checkout-title", 1)])],
        Some("unbound-reference"),
    ),
    Vector {
        name: "jobs-whole-program-graph",
        two_groups: true,
        group: "jobs",
        graphs: &[Graph {
            identity: "storefront-jobs",
            targets: &["worker"],
            units: &[&["main"]],
            edges: &[],
            bindings: &[("digest-subject", 0)],
        }],
        references: JOBS_REFERENCES,
        rejected: None,
    },
    Vector {
        name: "jobs-differently-grouped-target",
        two_groups: true,
        group: "jobs",
        graphs: &[Graph {
            identity: "storefront-jobs",
            targets: &["browser", "worker"],
            units: &[&["main"]],
            edges: &[],
            bindings: &[("cart-title", 0), ("digest-subject", 0)],
        }],
        references: JOBS_REFERENCES,
        rejected: Some("target-outside-group"),
    },
];

/// The selected group's partition: every graph non-empty, inside the group,
/// disjoint from the others, and together covering every member.
fn partition(vector: &Vector, members: &[&str]) -> Result<(), &'static str> {
    if vector.graphs.is_empty() {
        return Err("no-graph");
    }
    if vector.graphs.iter().any(|graph| graph.targets.is_empty()) {
        return Err("empty-applicability");
    }
    let mut covered = BTreeMap::<&str, usize>::new();
    for target in vector.graphs.iter().flat_map(|graph| graph.targets) {
        if !members.contains(target) {
            return Err("target-outside-group");
        }
        *covered.entry(target).or_default() += 1;
    }
    if covered.values().any(|count| *count > 1) {
        return Err("overlapping-applicability");
    }
    if members.iter().any(|member| !covered.contains_key(member)) {
        return Err("uncovered-target");
    }
    Ok(())
}

/// One graph's roots, the units no edge enters in their canonical order,
/// once its units, edges, and reference bindings are valid.
fn roots(vector: &Vector, graph: &Graph) -> Result<Vec<Value>, &'static str> {
    let units = graph.units.len();
    let distinct = graph.units.iter().collect::<BTreeSet<_>>();
    if distinct.len() != units {
        return Err("duplicate-node");
    }
    let ordinal = |unit: u64| usize::try_from(unit).ok().filter(|unit| *unit < units);
    let mut edges = Vec::new();
    for (from, to) in graph.edges {
        match (ordinal(*from), ordinal(*to)) {
            (Some(from), Some(to)) => edges.push((from, to)),
            _ => return Err("unknown-endpoint"),
        }
    }
    if edges.iter().any(|(from, to)| from == to) {
        return Err("self-edge");
    }
    if edges.iter().collect::<BTreeSet<_>>().len() != edges.len() {
        return Err("duplicate-edge");
    }
    // Kahn's order: a graph is acyclic when every unit leaves it.
    let mut entering = vec![0_usize; units];
    for (_, to) in &edges {
        entering[*to] += 1;
    }
    let sources = (0..units)
        .filter(|unit| entering[*unit] == 0)
        .collect::<Vec<_>>();
    let mut ready = sources.clone();
    let mut left = units;
    while let Some(unit) = ready.pop() {
        left -= 1;
        for (_, to) in edges.iter().filter(|(from, _)| *from == unit) {
            entering[*to] -= 1;
            if entering[*to] == 0 {
                ready.push(*to);
            }
        }
    }
    if left != 0 {
        return Err("cycle");
    }
    if graph
        .bindings
        .iter()
        .any(|(_, unit)| ordinal(*unit).is_none())
    {
        return Err("unknown-unit");
    }
    let applicable = vector
        .references
        .iter()
        .filter(|(target, _)| graph.targets.contains(target))
        .flat_map(|(_, references)| references.iter())
        .collect::<BTreeSet<_>>();
    for reference in applicable {
        match graph
            .bindings
            .iter()
            .filter(|(bound, _)| bound == reference)
            .count()
        {
            0 => return Err("unbound-reference"),
            1 => {}
            _ => return Err("multiply-bound-reference"),
        }
    }
    Ok(sources
        .into_iter()
        .map(|unit| json!(graph.units[unit]))
        .collect())
}

/// The consumer inputs and the relationship of `vector` against the facts
/// its producer hands over, checked against the rule the table states.
///
/// # Panics
///
/// Panics when the conclusion differs from the table's, or a graph's units
/// are not in canonical order, which the table rules out.
pub(super) fn relationship(vector: &Vector, facts: &Value) -> (Value, Value) {
    let graphs = vector
        .graphs
        .iter()
        .map(|graph| {
            assert!(
                graph.units.windows(2).all(|pair| pair[0] <= pair[1]),
                "{}",
                vector.name
            );
            json!({
                "identity": graph.identity,
                "targets": graph.targets,
                "units": graph.units.iter().map(|id| json!({ "id": id })).collect::<Vec<_>>(),
                "edges": graph
                    .edges
                    .iter()
                    .map(|(from, to)| json!({ "from": from, "to": to }))
                    .collect::<Vec<_>>(),
                "bindings": graph
                    .bindings
                    .iter()
                    .map(|(reference, unit)| json!({ "reference": reference, "unit": unit }))
                    .collect::<Vec<_>>(),
            })
        })
        .collect::<Vec<_>>();
    let references = vector
        .references
        .iter()
        .map(|(target, references)| ((*target).to_owned(), json!(references)))
        .collect::<Map<_, _>>();
    let inputs = json!({
        "selectedGroupId": vector.group,
        "graphs": graphs,
        "references": references,
    });
    let members = facts["deploymentGroups"][vector.group]["members"]
        .as_array()
        .expect("a selected group lists its members")
        .iter()
        .map(|member| member.as_str().expect("a member is a Target ID"))
        .collect::<Vec<_>>();
    let concluded = partition(vector, &members).and_then(|()| {
        let mut admitted = vector
            .graphs
            .iter()
            .map(|graph| {
                roots(vector, graph).map(|roots| {
                    json!({ "identity": graph.identity, "targets": graph.targets, "roots": roots })
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        admitted.sort_by(|a, b| a["identity"].as_str().cmp(&b["identity"].as_str()));
        Ok(admitted)
    });
    assert_eq!(
        concluded.as_ref().err().copied(),
        vector.rejected,
        "{}",
        vector.name
    );
    let relationship = match concluded {
        Ok(graphs) => json!({ "outcome": "admitted", "graphs": graphs }),
        Err(rule) => json!({ "outcome": "rejected", "rule": rule }),
    };
    (inputs, relationship)
}

#[cfg(test)]
mod tests {
    use super::super::tests::written;
    use super::*;

    /// The written graph-admission vectors.
    fn manifests() -> Vec<Value> {
        let (_, vectors) = written();
        vectors
            .files
            .iter()
            .map(|file| serde_json::from_slice::<Value>(&file.bytes).unwrap())
            .filter(|manifest| {
                manifest["id"]
                    .as_str()
                    .unwrap()
                    .starts_with("015-handoff-graph-admission-")
            })
            .collect()
    }

    /// Every rule 015's graph admission states that the vector's inputs
    /// break, each read again on its own from the vector alone, in no order.
    fn broken(facts: &Value, inputs: &Value) -> BTreeSet<&'static str> {
        let mut broken = BTreeSet::new();
        let graphs = inputs["graphs"].as_array().unwrap();
        let members = facts["deploymentGroups"][inputs["selectedGroupId"].as_str().unwrap()]
            ["members"]
            .as_array()
            .unwrap();
        let targets = graphs
            .iter()
            .flat_map(|graph| graph["targets"].as_array().unwrap())
            .map(|target| target.as_str().unwrap())
            .collect::<Vec<_>>();
        if graphs.is_empty() {
            broken.insert("no-graph");
        }
        if graphs
            .iter()
            .any(|graph| graph["targets"].as_array().unwrap().is_empty())
        {
            broken.insert("empty-applicability");
        }
        if targets
            .iter()
            .any(|target| !members.contains(&json!(target)))
        {
            broken.insert("target-outside-group");
        }
        let distinct = targets.iter().collect::<BTreeSet<_>>();
        if distinct.len() != targets.len() {
            broken.insert("overlapping-applicability");
        }
        // An empty set breaks its own rule; the cover is a non-empty set's.
        if !graphs.is_empty()
            && members
                .iter()
                .any(|member| !distinct.contains(&member.as_str().unwrap()))
        {
            broken.insert("uncovered-target");
        }
        for graph in graphs {
            let units = graph["units"].as_array().unwrap();
            let count = units.len() as u64;
            let ids = units
                .iter()
                .map(|unit| unit["id"].to_string())
                .collect::<Vec<_>>();
            if ids.iter().collect::<BTreeSet<_>>().len() != ids.len() {
                broken.insert("duplicate-node");
            }
            let edges = graph["edges"]
                .as_array()
                .unwrap()
                .iter()
                .map(|edge| (edge["from"].as_u64().unwrap(), edge["to"].as_u64().unwrap()))
                .collect::<Vec<_>>();
            if edges
                .iter()
                .any(|(from, to)| *from >= count || *to >= count)
            {
                broken.insert("unknown-endpoint");
            }
            if edges.iter().any(|(from, to)| from == to) {
                broken.insert("self-edge");
            }
            let mut sorted = edges.clone();
            sorted.sort_unstable();
            sorted.dedup();
            if sorted.len() != edges.len() {
                broken.insert("duplicate-edge");
            }
            if cyclic(count, &edges) {
                broken.insert("cycle");
            }
            let bindings = graph["bindings"].as_array().unwrap();
            if bindings
                .iter()
                .any(|binding| binding["unit"].as_u64().unwrap() >= count)
            {
                broken.insert("unknown-unit");
            }
            for target in graph["targets"].as_array().unwrap() {
                let applicable = inputs["references"][target.as_str().unwrap()]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                for reference in applicable {
                    let bound = bindings
                        .iter()
                        .filter(|binding| binding["reference"] == reference)
                        .count();
                    if bound == 0 {
                        broken.insert("unbound-reference");
                    }
                    if bound > 1 {
                        broken.insert("multiply-bound-reference");
                    }
                }
            }
        }
        broken
    }

    /// Whether a depth-first walk from some unit, following edges between
    /// known, distinct units, comes back to a unit on its path.
    fn cyclic(count: u64, edges: &[(u64, u64)]) -> bool {
        fn walk(
            unit: u64,
            edges: &[(u64, u64)],
            path: &mut Vec<u64>,
            done: &mut BTreeSet<u64>,
        ) -> bool {
            if path.contains(&unit) {
                return true;
            }
            if !done.insert(unit) {
                return false;
            }
            path.push(unit);
            let found = edges
                .iter()
                .filter(|(from, to)| *from == unit && from != to)
                .any(|(_, to)| walk(*to, edges, path, done));
            path.pop();
            found
        }
        let mut done = BTreeSet::new();
        (0..count).any(|unit| walk(unit, edges, &mut Vec::new(), &mut done))
    }

    #[test]
    fn each_vector_breaks_the_one_rule_it_expects_or_none() {
        let manifests = manifests();
        assert_eq!(manifests.len(), VECTORS.len());
        let mut rules = BTreeSet::new();
        for manifest in &manifests {
            let broken = broken(
                &manifest["producerProfileFacts"],
                &manifest["consumerInputs"]["json"],
            );
            let relationship = &manifest["expectedRelationship"]["json"];
            if relationship["outcome"] == "admitted" {
                assert!(broken.is_empty(), "{}: {broken:?}", manifest["id"]);
            } else {
                assert_eq!(broken.len(), 1, "{}: {broken:?}", manifest["id"]);
                assert_eq!(
                    relationship["rule"],
                    *broken.first().unwrap(),
                    "{}",
                    manifest["id"]
                );
                rules.insert(relationship["rule"].as_str().unwrap());
            }
        }
        // Every rule 015 states has its vector.
        assert_eq!(
            rules,
            [
                "cycle",
                "duplicate-edge",
                "duplicate-node",
                "empty-applicability",
                "multiply-bound-reference",
                "no-graph",
                "overlapping-applicability",
                "self-edge",
                "target-outside-group",
                "unbound-reference",
                "uncovered-target",
                "unknown-endpoint",
                "unknown-unit",
            ]
            .into()
        );
    }

    #[test]
    fn an_admitted_graph_s_roots_are_its_units_no_edge_enters_in_canonical_order() {
        for manifest in manifests() {
            let relationship = &manifest["expectedRelationship"]["json"];
            if relationship["outcome"] != "admitted" {
                continue;
            }
            let inputs = &manifest["consumerInputs"]["json"];
            let mut identities = Vec::new();
            for admitted in relationship["graphs"].as_array().unwrap() {
                let graph = inputs["graphs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|graph| graph["identity"] == admitted["identity"])
                    .unwrap();
                let units = graph["units"].as_array().unwrap();
                let entered = graph["edges"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|edge| edge["to"].as_u64().unwrap())
                    .collect::<BTreeSet<_>>();
                let mut roots = (0..units.len() as u64)
                    .filter(|unit| !entered.contains(unit))
                    .map(|unit| {
                        units[usize::try_from(unit).unwrap()]["id"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|token| token.as_str().unwrap().to_owned())
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                roots.sort();
                assert_eq!(admitted["roots"], json!(roots), "{}", manifest["id"]);
                assert_eq!(admitted["targets"], graph["targets"]);
                identities.push(admitted["identity"].as_str().unwrap());
            }
            let mut sorted = identities.clone();
            sorted.sort_unstable();
            assert_eq!(identities, sorted);
            assert_eq!(identities.len(), inputs["graphs"].as_array().unwrap().len());
        }
    }

    #[test]
    fn each_vector_hands_over_the_delivery_and_the_groups_to_020() {
        let mut shared = false;
        let mut hydration = false;
        for manifest in manifests() {
            let facts = manifest["producerProfileFacts"].as_object().unwrap();
            assert_eq!(
                facts.keys().collect::<Vec<_>>(),
                ["delivery", "deploymentGroups"]
            );
            assert_eq!(manifest["producerArtifactBindings"], json!([]));
            assert_eq!(manifest["verificationOwner"], "020");
            let inputs = &manifest["consumerInputs"]["json"];
            // The references name exactly the selected group's members.
            let members =
                &facts["deploymentGroups"][inputs["selectedGroupId"].as_str().unwrap()]["members"];
            let named = inputs["references"]
                .as_object()
                .unwrap()
                .keys()
                .map(|target| json!(target))
                .collect::<Vec<_>>();
            assert_eq!(json!(named), *members, "{}", manifest["id"]);
            if manifest["expectedRelationship"]["json"]["outcome"] == "admitted" {
                let graphs = inputs["graphs"].as_array().unwrap();
                shared |= graphs
                    .iter()
                    .any(|graph| graph["targets"].as_array().unwrap().len() > 1);
                // A hydration relation's two targets in different graphs.
                let relations = &facts["deploymentGroups"]
                    [inputs["selectedGroupId"].as_str().unwrap()]["hydrationRelations"];
                hydration |= relations.as_array().unwrap().iter().any(|relation| {
                    let of = |target: &Value| {
                        graphs
                            .iter()
                            .position(|graph| graph["targets"].as_array().unwrap().contains(target))
                    };
                    of(&relation["server"]) != of(&relation["client"])
                });
            }
        }
        assert!(shared && hydration);
    }
}
