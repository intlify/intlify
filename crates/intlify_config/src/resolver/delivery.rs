// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's Phase 10: the placement the selected declaration resolves to under
//! the admitted Delivery Graph Specification and Delivery Placement Policy.
//! Revision "0" admits one effective mode, `duplicate`, which an omitted
//! placement resolves to as well. Structural admission already rejects every
//! other mode, a placement override, and a graph or physical output member
//! (015-137), so this phase reports nothing of its own. Not the host's
//! Delivery Unit Graph, which 020 admits per compiler transaction.

use crate::locale::VersionedIdentity;
use crate::model::{Delivery, DeliveryPlacement};

use super::evaluation::{
    both, Admitted, Blocking, Check, Evaluation, EvaluationInvariant, Phase, Subject, Unit,
};

/// The one unit Phase 10 records.
pub(crate) const UNIT: Unit = Unit {
    phase: Phase::DeliveryResolution,
    subject: Subject::DeliveryPlacement,
    check: Check::EffectiveMode,
};

/// The placement modes revision "0" admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PlacementMode {
    Duplicate,
}

/// The checked delivery policy: both admitted specifications and the
/// effective placement mode, which an omitted placement still records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryPolicy<Id> {
    graph: VersionedIdentity<Id>,
    placement: VersionedIdentity<Id>,
    mode: PlacementMode,
}

impl<Id> DeliveryPolicy<Id> {
    pub(crate) const fn graph(&self) -> &VersionedIdentity<Id> {
        &self.graph
    }

    pub(crate) const fn placement(&self) -> &VersionedIdentity<Id> {
        &self.placement
    }

    pub(crate) const fn mode(&self) -> PlacementMode {
        self.mode
    }
}

/// Phase 10 over the two delivery slots Phase 5 admitted and the `delivery`
/// field as structural admission left it.
pub(crate) fn resolve_delivery<Id: Clone>(
    evaluation: &mut Evaluation,
    graph: &Admitted<VersionedIdentity<Id>>,
    placement: &Admitted<VersionedIdentity<Id>>,
    delivery: Admitted<Option<&Delivery>>,
) -> Result<Admitted<DeliveryPolicy<Id>>, EvaluationInvariant> {
    evaluation.check(
        UNIT,
        both(both(graph.clone(), placement.clone()), delivery),
        |((graph, placement), delivery)| {
            let mode = match delivery.and_then(|delivery| delivery.placement.as_option()) {
                None | Some(DeliveryPlacement::Duplicate) => PlacementMode::Duplicate,
            };
            Ok::<_, Blocking>(DeliveryPolicy {
                graph,
                placement,
                mode,
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::resolver::evaluation::{Causes, Status};
    use crate::resolver::locales::fixtures::{stand_in, units};
    use crate::resolver::specification::Slot;

    type Id = &'static str;

    const GRAPH: VersionedIdentity<Id> = VersionedIdentity {
        identity: "intlify-delivery-graph",
        revision: "0",
    };
    const PLACEMENT: VersionedIdentity<Id> = VersionedIdentity {
        identity: "intlify-delivery-placement",
        revision: "0",
    };

    fn delivery(value: serde_json::Value) -> Delivery {
        serde_json::from_value(value).unwrap()
    }

    fn resolve(
        graph: &Admitted<VersionedIdentity<Id>>,
        placement: &Admitted<VersionedIdentity<Id>>,
        declared: Admitted<Option<&Delivery>>,
    ) -> (Evaluation, Admitted<DeliveryPolicy<Id>>) {
        let mut evaluation = Evaluation::default();
        let policy = resolve_delivery(&mut evaluation, graph, placement, declared).unwrap();
        (evaluation, policy)
    }

    #[test]
    fn omitted_and_explicit_duplicate_placement_resolve_to_one_policy() {
        let empty = delivery(json!({}));
        let explicit = delivery(json!({"placement": "duplicate"}));
        let resolved = [None, Some(&empty), Some(&explicit)].map(|declared| {
            let (evaluation, policy) = resolve(&Ok(GRAPH), &Ok(PLACEMENT), Ok(declared));
            assert_eq!(evaluation.status(&UNIT), Some(&Status::Evaluated));
            assert_eq!(evaluation.issues().count(), 0);
            policy.unwrap()
        });
        assert!(resolved.windows(2).all(|pair| pair[0] == pair[1]));
        let policy = &resolved[0];
        assert_eq!(
            (policy.graph(), policy.placement(), policy.mode()),
            (&GRAPH, &PLACEMENT, PlacementMode::Duplicate)
        );
        assert_eq!(UNIT.phase.id(), "delivery-resolution");
    }

    #[test]
    fn an_unadmitted_slot_or_field_leaves_the_placement_not_evaluated() {
        let graph = stand_in(Slot::DeliveryGraph);
        let placement = stand_in(Slot::DeliveryPlacement);
        let field = stand_in(Slot::Coverage);
        let declared = delivery(json!({"placement": "duplicate"}));
        let cases: [(Admitted<_>, Admitted<_>, Admitted<_>, Causes); 4] = [
            (
                Err(graph.clone()),
                Ok(PLACEMENT),
                Ok(Some(&declared)),
                graph.clone(),
            ),
            (
                Ok(GRAPH),
                Err(placement.clone()),
                Ok(None),
                placement.clone(),
            ),
            (Ok(GRAPH), Ok(PLACEMENT), Err(field.clone()), field.clone()),
            (
                Err(graph.clone()),
                Err(placement.clone()),
                Err(field.clone()),
                graph.union(placement).union(field),
            ),
        ];
        for (graph, placement, declared, causes) in cases {
            let (evaluation, policy) = resolve(&graph, &placement, declared);
            assert_eq!(
                evaluation.status(&UNIT),
                Some(&Status::NotEvaluated(causes.clone()))
            );
            assert_eq!(units(policy.as_ref().unwrap_err()), units(&causes));
        }
    }
}
