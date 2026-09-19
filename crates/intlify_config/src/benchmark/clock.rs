// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! The clock this owner measures with.
//!
//! The provider, the raw-domain subtraction, and the conversion rule belong to
//! `intlify_measurement`: they are 026's measurement method rather than this
//! owner's choice. This module only narrows them to the visibility used here.

pub(super) use intlify_measurement::acquisition::{
    Clock, ClockDescription, ClockFailure, MonotonicClock,
};

#[cfg(test)]
pub(super) use intlify_measurement::acquisition::Tick;

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use intlify_measurement::acquisition::Tick;
    use std::cell::RefCell;
    use std::collections::VecDeque;

    /// A clock whose readings the test writes in advance.
    ///
    /// The real provider cannot produce a reversed or failing read on demand,
    /// so the cases that must not become a measured sample need this double.
    pub(crate) struct ScriptedClock(RefCell<VecDeque<Result<Tick, ClockFailure>>>);

    impl ScriptedClock {
        pub(crate) fn new(values: impl IntoIterator<Item = Result<Tick, ClockFailure>>) -> Self {
            Self(RefCell::new(values.into_iter().collect()))
        }
        pub(crate) fn nanos(values: impl IntoIterator<Item = i64>) -> Self {
            Self::new(values.into_iter().map(|nanos| Tick::new(0, nanos)))
        }
        pub(crate) fn remaining(&self) -> usize {
            self.0.borrow().len()
        }
    }

    impl Clock for ScriptedClock {
        fn read(&self) -> Result<Tick, ClockFailure> {
            self.0
                .borrow_mut()
                .pop_front()
                .expect("unexpected clock read")
        }
    }
}
