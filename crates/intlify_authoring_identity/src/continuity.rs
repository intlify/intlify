// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Design 016's continuity, newness and absence, checked against evidence.
//!
//! An Intent ID continues across an edit or a move only when the edit is
//! verified and carries exactly one old declaration onto exactly one current
//! one. A declaration is new only when the base has no history or the edit
//! shows its text was inserted. A declaration is gone only when a complete
//! inventory agrees with the host's membership and an edit or a removed unit
//! shows it went. Anything short of that stays unresolved: missing evidence
//! is never read as proof of the opposite.

mod bases;
mod edit;
mod evidence;
mod sources;

pub use bases::{
    verify_bases, BasisGap, BasisReport, BasisVerdict, ContinuityFailure, ContinuityInputs,
    PreviousUpdate,
};
pub use edit::{
    fate, inserted, is_edit_replay_profile, replay, RangeFate, ReplayGap, EDIT_REPLAY_PROFILE,
    EDIT_REPLAY_REVISION,
};
pub use sources::{RetainedSourceFailure, RetainedSources};
