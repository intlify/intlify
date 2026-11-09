// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's closed pre-invocation boundaries (D15:1106, 3127-3148).
//!
//! A binding or adapter admits each raw component of one Resolver Invocation
//! Input Set at its boundary before the resolver starts. A boundary admits a
//! component as its normalized value or refuses it with exactly one reason its
//! row of 015's boundary table lists: the first of the boundary's checks that
//! fails decides (D15:3146). Each component is admitted on its own, and a
//! refused one never becomes a resolver outcome, Finding, or Evidence. The
//! capability boundary runs first, because every boundary that reads raw input
//! takes its bounds from the admitted capability (D15:1106).

pub(crate) mod capability;
