// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::any::Any;
use std::sync::Arc;

use crate::artifact::{ArtifactBuilder, MessageEntry};
use crate::registry::ResolvedHostFormat;
use crate::{InternalResourceErrorReason, ResourceError, ResourcePhase};

pub(crate) type AdapterArtifactState = Arc<dyn Any + Send + Sync>;

// The concrete JSON adapter starts constructing typed plans in Milestone 6.
#[allow(dead_code)]
pub(crate) struct AdapterReescapePlan {
    measured_len: u64,
    state: Box<dyn Any + Send + Sync>,
}

#[allow(dead_code)]
impl AdapterReescapePlan {
    pub(crate) fn new(measured_len: u64, state: Box<dyn Any + Send + Sync>) -> Self {
        Self {
            measured_len,
            state,
        }
    }

    pub(crate) const fn measured_len(&self) -> u64 {
        self.measured_len
    }

    pub(crate) fn state(&self) -> &(dyn Any + Send + Sync) {
        self.state.as_ref()
    }
}

pub(crate) trait HostAdapter: Send + Sync {
    fn extract(
        &self,
        resolved: &ResolvedHostFormat,
        source: &Arc<str>,
        builder: &mut ArtifactBuilder,
        phase: ResourcePhase,
    ) -> Result<AdapterArtifactState, ResourceError>;

    fn plan_reescape(
        &self,
        artifact_state: &(dyn Any + Send + Sync),
        entry: &MessageEntry,
        formatted_message: &str,
        phase: ResourcePhase,
    ) -> Result<AdapterReescapePlan, ResourceError>;

    fn materialize(
        &self,
        artifact_state: &(dyn Any + Send + Sync),
        entry: &MessageEntry,
        formatted_message: &str,
        plan: &AdapterReescapePlan,
        phase: ResourcePhase,
    ) -> Result<String, ResourceError>;
}

pub(crate) struct PendingJsonAdapter;

impl HostAdapter for PendingJsonAdapter {
    fn extract(
        &self,
        _resolved: &ResolvedHostFormat,
        _source: &Arc<str>,
        _builder: &mut ArtifactBuilder,
        phase: ResourcePhase,
    ) -> Result<AdapterArtifactState, ResourceError> {
        Err(ResourceError::internal(
            InternalResourceErrorReason::AdapterInvariantFailed,
            phase,
            None,
        ))
    }

    fn plan_reescape(
        &self,
        _artifact_state: &(dyn Any + Send + Sync),
        _entry: &MessageEntry,
        _formatted_message: &str,
        phase: ResourcePhase,
    ) -> Result<AdapterReescapePlan, ResourceError> {
        Err(ResourceError::internal(
            InternalResourceErrorReason::AdapterInvariantFailed,
            phase,
            None,
        ))
    }

    fn materialize(
        &self,
        _artifact_state: &(dyn Any + Send + Sync),
        _entry: &MessageEntry,
        _formatted_message: &str,
        _plan: &AdapterReescapePlan,
        phase: ResourcePhase,
    ) -> Result<String, ResourceError> {
        Err(ResourceError::internal(
            InternalResourceErrorReason::AdapterInvariantFailed,
            phase,
            None,
        ))
    }
}
