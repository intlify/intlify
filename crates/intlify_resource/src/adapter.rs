// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

use std::any::Any;
use std::sync::Arc;

mod json;

pub(crate) use json::JsonAdapter;

use crate::artifact::{ArtifactBuilder, MessageEntry};
use crate::registry::ResolvedHostFormat;
use crate::{ResourceError, ResourcePhase};

pub(crate) type AdapterArtifactState = Arc<dyn Any + Send + Sync>;

pub(crate) struct AdapterReescapePlan {
    measured_len: u64,
    state: Box<dyn Any + Send + Sync>,
}

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
