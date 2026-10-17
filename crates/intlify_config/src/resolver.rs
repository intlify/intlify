// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Crate-private state of 015's Semantic Profile Resolver (015 Phase 3). Not a
//! `LocalizationProjectProfile`, a checked or blocked outcome, Resolution
//! Evidence, or a registry: each resolution phase records which of its checks it
//! evaluated and why the others were not, and hands later phases the values it
//! admitted. Phases that need construction authority or a confirmed Resource
//! Limit Policy take them as values that only tests build until the phases
//! producing them exist.

pub(crate) mod evaluation;
pub(crate) mod identity;
pub(crate) mod limits;
pub(crate) mod locales;
pub(crate) mod specification;
