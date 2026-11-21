// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! 015's closed pre-invocation boundaries (D15:1106, 3135-3151).
//!
//! A binding or adapter admits each raw component of one Resolver Invocation
//! Input Set at its boundary before the resolver starts. A boundary admits a
//! component as its normalized value or refuses it with exactly one reason its
//! row of 015's boundary table lists: the first of the boundary's checks that
//! fails decides (D15:3155). Each component is admitted on its own, and a
//! refused one never becomes a resolver outcome, Finding, or Evidence. The
//! capability boundary runs first, because every boundary that reads raw input
//! takes its bounds from the admitted capability (D15:1106). A Safe Origin
//! Kind candidate is admitted at its own boundary beside its component's
//! value, and reads no bound (015-166). A selector is admitted from its
//! binding's closed report, not from a live host value (015-167). The Common
//! Invocation Input Set is admitted in one step that yields the admitted set
//! or every failure in the rank of its slots (015-168). A file entry's source
//! identity and revision are admitted against its bytes (015-170), and its
//! locator candidate is admitted or treated as absence, never as a failure
//! (015-171), as a Programmatic Entry's or Call-Site Source's `safe-label` or
//! `safe-uri` candidate is. A Programmatic Entry Snapshot is admitted as its
//! canonical tree or refused by the first failing check of its bounds, its
//! closed model, its remaining value rules, and its digest (015-199,
//! 015-200), and the Programmatic Entry identity and revision are admitted
//! like a file's pair, against the digest of the admitted Snapshot (015-201).
//! An optional call-site sidecar is admitted with its span candidates as
//! submitted, or refused in the order of its row's reasons, its own
//! Call-Site Source pair checked against the sidecar's source bytes. A file
//! or programmatic Entry Admission Input Set is admitted in a step of its own
//! that works like the common one (015-172, 015-201); the steps share the
//! parts in `step`. The reference to the Resolver Construction Identity is
//! admitted at a boundary of its own, against the identity of the resolver the
//! invocation runs on (D15:3151).

pub(crate) mod call_site;
pub(crate) mod capability;
pub(crate) mod common;
pub(crate) mod construction_identity;
pub(crate) mod entry;
pub(crate) mod locator;
pub(crate) mod origin;
pub(crate) mod selector;
pub(crate) mod snapshot;
pub(crate) mod source;
pub(crate) mod specification;
pub(crate) mod step;
pub(crate) mod verification;
