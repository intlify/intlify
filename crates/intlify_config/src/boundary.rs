// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's closed pre-invocation boundaries (D15:1106, 3134-3148).
//!
//! A binding or adapter admits each raw component of one Resolver Invocation
//! Input Set at its boundary before the resolver starts. A boundary admits a
//! component as its normalized value or refuses it with exactly one reason its
//! row of 015's boundary table lists: the first of the boundary's checks that
//! fails decides (D15:3152). Each component is admitted on its own, and a
//! refused one never becomes a resolver outcome, Finding, or Evidence. The
//! capability boundary runs first, because every boundary that reads raw input
//! takes its bounds from the admitted capability (D15:1106). A Safe Origin
//! Kind candidate is admitted at its own boundary beside its component's
//! value, and reads no bound (015-166). A selector is admitted from its
//! binding's closed report, not from a live host value (015-167). The Common
//! Invocation Input Set is admitted in one step that yields the admitted set
//! or every failure in the rank of its slots (015-168).

pub(crate) mod capability;
pub(crate) mod common;
pub(crate) mod origin;
pub(crate) mod selector;
pub(crate) mod specification;
pub(crate) mod verification;
