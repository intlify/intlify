// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use crate::benchmark::construction::{PreparedConstruction, PreparedIdentity};
use crate::benchmark::quantity::Quantity;
use crate::construction::admission::ConstructionBoundId;

use super::{LogicalWork, WorkFailure, WorkKind, WorkStage, WorkValue};

impl LogicalWork {
    /// The input's totals in the construction-root bounds' units: the totals
    /// row 1 compares, whose counting the bounds' own tests check against
    /// independent counts. A total too large to count exceeds every bound
    /// and is at least the largest count.
    pub(super) fn construction(&mut self, prepared: &PreparedConstruction) {
        let totals = prepared.totals();
        for (kind, bound) in WorkKind::CONSTRUCTION
            .into_iter()
            .zip(ConstructionBoundId::ALL)
        {
            let observation = totals.get(bound).map_or(
                WorkValue::AtLeast {
                    value: Quantity::new(u64::MAX),
                },
                WorkValue::exact,
            );
            self.set(kind, WorkStage::PreparedInput, observation);
        }
    }

    /// The bytes the identity is the digest of.
    pub(super) fn construction_identity(
        &mut self,
        prepared: &PreparedIdentity,
    ) -> Result<(), WorkFailure> {
        let bytes = u64::try_from(prepared.preimage().len())
            .map_err(|_| WorkFailure::UnrepresentableCounter)?;
        self.set(
            WorkKind::ConstructionIdentityPreimageBytes,
            WorkStage::OperationResult,
            WorkValue::exact(bytes),
        );
        Ok(())
    }
}
