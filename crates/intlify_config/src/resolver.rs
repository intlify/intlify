// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Crate-private state of 015's Semantic Profile Resolver (015 Phase 3). Not a
//! `LocalizationProjectProfile`, a checked or blocked outcome, Resolution
//! Evidence, or a registry: each resolution phase records which of its checks it
//! evaluated and why the others were not, and hands later phases the values it
//! admitted. Phases that need construction authority or a confirmed Resource
//! Limit Policy take them as values. Phase 4 confirms the policy; until an
//! entry runs the phases in order, tests build both.

pub(crate) mod artifact_resolution;
pub(crate) mod bootstrap;
pub(crate) mod coverage;
pub(crate) mod delivery;
pub(crate) mod evaluation;
pub(crate) mod exact_resolution;
pub(crate) mod fallback;
pub(crate) mod identity;
pub(crate) mod limits;
pub(crate) mod locales;
pub(crate) mod negotiation;
pub(crate) mod resource_policy;
pub(crate) mod specification;
pub(crate) mod tally;
pub(crate) mod vocabulary;
