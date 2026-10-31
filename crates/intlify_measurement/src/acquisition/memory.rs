// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! Acquiring one peak of live memory.
//!
//! An observer counts the bytes requested through the global allocator, by
//! allocation `Layout`, on the calling thread only, from the start of one
//! interval to its end. The peak is the largest net growth over the start, and
//! the net is where the growth ends. An interval that frees storage allocated
//! before it can end below zero: it reached past its own boundary, so it is
//! refused rather than clamped to zero. Only the end is checked, so one that
//! frees earlier storage and then allocates more than it freed passes with a
//! peak that understates its growth; keeping earlier storage out of the
//! interval is the case's own boundary to keep.
//!
//! One interval is one invocation between the observer's marks. Preparing the
//! input and destroying the output are outside it; the output is retained
//! past the end mark, so building it is inside. The peak is the observed
//! maximum and is never divided by a repetition count.
//!
//! The observer is a seam: this module never installs an allocator. The one
//! implementation here, behind the non-default `allocation-counter` feature,
//! uses a crate that installs its own global allocator in every binary that
//! links it, which is why only a memory measurement's own binary enables it.

use std::hint::black_box;
use std::panic::{catch_unwind, AssertUnwindSafe};

use intlify_shared_json::quantity::Quantity;

use crate::identity::VersionedIdentity;

/// What one observed interval allocated on the calling thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Allocation {
    /// The largest net growth of live bytes over the start of the interval.
    pub peak_bytes: u64,
    /// The net change of live bytes at the end of the interval.
    pub net_bytes: i64,
}

/// An allocator observer an interval can be run under.
pub trait MemoryObserver {
    /// Run `interval` exactly once between the observer's marks, and report
    /// what it allocated on this thread.
    fn observe(&self, interval: &mut dyn FnMut()) -> Allocation;
}

/// What an observer provider reports about itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObserverDescription {
    /// The crate that counts.
    pub provider: &'static str,
    /// Its locked revision.
    pub provider_revision: &'static str,
    /// The observer's identity, which the environment records.
    pub identity: &'static str,
    /// The revision of that identity.
    pub revision: &'static str,
    /// What one count is.
    pub counting: &'static str,
    /// What the counts do not see, in the provider's own behavior.
    pub limitations: &'static [&'static str],
}

impl ObserverDescription {
    /// Present the observer's identity as the environment field it fills.
    #[must_use]
    pub fn observer(self) -> VersionedIdentity {
        VersionedIdentity::literal(self.identity, self.revision)
    }
}

/// Why one interval yields no peak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MemoryFailure {
    /// The invocation panicked.
    InvocationPanicked,
    /// The observer returned without running the interval.
    NotInvoked,
    /// The interval freed storage that existed before it, so its net growth
    /// fell below zero.
    NegativeNet,
}

/// One observed interval and the output it produced.
pub struct ObservedPeak<Output> {
    /// The largest net growth of live bytes over the start of the interval.
    pub peak_bytes: Quantity,
    /// The live bytes the interval still held at its end, such as the output.
    pub net_bytes: Quantity,
    pub output: Output,
}

/// Observe exactly one invocation.
///
/// The output is retained past the end mark, so its construction is inside
/// the interval and its destruction is outside. A panic is contained here and
/// becomes a failed observation rather than an unwinding harness.
pub fn observe<Observer, Input, Output>(
    observer: &Observer,
    input: Input,
    invoke: fn(Input) -> Output,
) -> Result<ObservedPeak<Output>, MemoryFailure>
where
    Observer: MemoryObserver + ?Sized,
{
    // Only finite owner-controlled operations call this helper. No live host
    // values, arbitrary callbacks, or panic payloads enter retained observations.
    let mut input = Some(black_box(input));
    let invoke = black_box(invoke);
    let mut output = None;
    let mut panicked = false;
    let allocation = observer.observe(&mut || {
        if let Some(input) = input.take() {
            match catch_unwind(AssertUnwindSafe(|| black_box(invoke(input)))) {
                Ok(value) => output = Some(value),
                Err(_) => panicked = true,
            }
        }
    });
    if panicked {
        return Err(MemoryFailure::InvocationPanicked);
    }
    let output = output.ok_or(MemoryFailure::NotInvoked)?;
    let net_bytes = u64::try_from(allocation.net_bytes).map_err(|_| MemoryFailure::NegativeNet)?;
    Ok(ObservedPeak {
        peak_bytes: Quantity::new(allocation.peak_bytes),
        net_bytes: Quantity::new(net_bytes),
        output,
    })
}

/// The revision of `allocation-counter` the observer counts through.
///
/// It is the locked dependency's own version, and a test fails until this
/// constant follows the lock.
#[cfg(feature = "allocation-counter")]
pub const ALLOCATION_COUNTER_REVISION: &str = "0.8.1";

/// The observer `allocation-counter` provides.
///
/// It counts the allocations and deallocations its global allocator sees on
/// the calling thread, by `Layout::size`. It does not count a reallocation in
/// place: a growing buffer is a new allocation and a release of the old one,
/// so for that moment both are live. Its counters are not checked for
/// overflow, and a nested observation adds its peak to the enclosing one, so
/// observations are never nested here.
#[cfg(feature = "allocation-counter")]
pub struct AllocationCounter;

#[cfg(feature = "allocation-counter")]
impl AllocationCounter {
    /// Describe the observer.
    #[must_use]
    pub const fn description() -> ObserverDescription {
        ObserverDescription {
            provider: "allocation-counter",
            provider_revision: ALLOCATION_COUNTER_REVISION,
            identity: "intlify-measurement-allocation-counter-observer",
            revision: "0",
            counting: "global-allocator-layout-size-on-the-calling-thread",
            limitations: &[
                "reallocation-counts-the-old-and-the-new-buffer-together",
                "counters-unchecked-for-overflow",
                "nested-observations-add-their-peaks",
            ],
        }
    }
}

#[cfg(feature = "allocation-counter")]
impl MemoryObserver for AllocationCounter {
    fn observe(&self, interval: &mut dyn FnMut()) -> Allocation {
        let counted = allocation_counter::measure(interval);
        Allocation {
            peak_bytes: counted.bytes_max,
            net_bytes: counted.bytes_current,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;
    use std::rc::Rc;

    /// An observer whose reports the test writes in advance.
    ///
    /// The real observer cannot report a contradictory interval on demand, so
    /// the cases that must not become a peak need this double.
    struct ScriptedObserver {
        reports: RefCell<VecDeque<Allocation>>,
        runs: bool,
        intervals: Cell<usize>,
    }

    impl ScriptedObserver {
        fn new(reports: impl IntoIterator<Item = Allocation>) -> Self {
            Self {
                reports: RefCell::new(reports.into_iter().collect()),
                runs: true,
                intervals: Cell::new(0),
            }
        }
    }

    impl MemoryObserver for ScriptedObserver {
        fn observe(&self, interval: &mut dyn FnMut()) -> Allocation {
            if self.runs {
                self.intervals.set(self.intervals.get() + 1);
                interval();
            }
            self.reports
                .borrow_mut()
                .pop_front()
                .expect("unexpected observation")
        }
    }

    fn report(peak_bytes: u64, net_bytes: i64) -> Allocation {
        Allocation {
            peak_bytes,
            net_bytes,
        }
    }

    #[test]
    fn a_peak_is_the_observed_maximum_with_the_output_retained_past_the_end_mark() {
        struct Output(Rc<Cell<u8>>);
        impl Drop for Output {
            fn drop(&mut self) {
                self.0.set(self.0.get() + 1);
            }
        }
        let dropped = Rc::new(Cell::new(0));
        let observer = ScriptedObserver::new([report(4096, 64)]);
        let observed = observe(&observer, &dropped, |counter| Output(Rc::clone(counter))).unwrap();
        assert_eq!(observed.peak_bytes, Quantity::new(4096));
        assert_eq!(observed.net_bytes, Quantity::new(64));
        assert_eq!(observer.intervals.get(), 1);
        assert_eq!(dropped.get(), 0);
        drop(observed);
        assert_eq!(dropped.get(), 1);
    }

    #[test]
    fn a_net_of_zero_is_a_peak_and_a_net_below_zero_is_not() {
        let observer = ScriptedObserver::new([report(0, 0), report(128, -1)]);
        let observed = observe(&observer, (), |()| ()).unwrap();
        assert_eq!(observed.peak_bytes, Quantity::new(0));
        assert_eq!(observed.net_bytes, Quantity::new(0));
        assert!(matches!(
            observe(&observer, (), |()| ()),
            Err(MemoryFailure::NegativeNet)
        ));
    }

    #[test]
    fn the_largest_count_is_kept_exactly() {
        let observer = ScriptedObserver::new([report(u64::MAX, i64::MAX)]);
        let observed = observe(&observer, (), |()| ()).unwrap();
        assert_eq!(observed.peak_bytes, Quantity::new(u64::MAX));
        assert_eq!(observed.net_bytes, Quantity::new(i64::MAX.unsigned_abs()));
    }

    #[test]
    fn a_panicking_invocation_is_not_a_zero_peak() {
        let observer = ScriptedObserver::new([report(0, 0)]);
        assert!(matches!(
            observe(&observer, (), |()| panic!("test-owned failure")),
            Err(MemoryFailure::InvocationPanicked)
        ));
        // The panic is reported even when the counts look like a clean interval.
        assert_eq!(observer.intervals.get(), 1);
    }

    #[test]
    fn an_observer_that_never_runs_the_interval_reports_no_peak() {
        let mut observer = ScriptedObserver::new([report(0, 0)]);
        observer.runs = false;
        let invoked = Cell::new(false);
        assert!(matches!(
            observe(&observer, &invoked, |called| called.set(true)),
            Err(MemoryFailure::NotInvoked)
        ));
        assert!(!invoked.get());
    }

    #[test]
    fn a_description_presents_its_identity_at_its_own_revision() {
        let description = ObserverDescription {
            provider: "test-provider",
            provider_revision: "9.9.9",
            identity: "test-observer",
            revision: "3",
            counting: "test-counting",
            limitations: &[],
        };
        // The provider's revision is the crate's; the observer's is its own.
        assert_eq!(
            description.observer(),
            VersionedIdentity::literal("test-observer", "3")
        );
    }

    #[test]
    fn a_failure_is_spelled_as_a_reason_is() {
        assert_eq!(
            serde_json::to_value([
                MemoryFailure::InvocationPanicked,
                MemoryFailure::NotInvoked,
                MemoryFailure::NegativeNet
            ])
            .unwrap(),
            serde_json::json!(["invocation-panicked", "not-invoked", "negative-net"])
        );
    }

    #[cfg(feature = "allocation-counter")]
    mod allocation_counter_observer {
        use super::super::*;

        #[test]
        fn the_revision_is_the_locked_dependency() {
            let lock = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Cargo.lock"),
            )
            .expect("the workspace lock file");
            let locked = lock
                .split("[[package]]")
                .filter(|block| {
                    block
                        .lines()
                        .any(|line| line.trim() == "name = \"allocation-counter\"")
                })
                .filter_map(|block| {
                    block
                        .lines()
                        .find_map(|line| line.trim().strip_prefix("version = "))
                        .map(|version| version.trim_matches('"'))
                })
                .collect::<Vec<_>>();
            assert_eq!(locked, [ALLOCATION_COUNTER_REVISION]);
            assert_eq!(
                AllocationCounter::description().provider_revision,
                ALLOCATION_COUNTER_REVISION
            );
        }

        #[test]
        fn a_retained_output_is_both_the_peak_and_the_net() {
            let observed =
                observe(&AllocationCounter, 4096_usize, |size| vec![0_u8; size]).unwrap();
            assert_eq!(observed.peak_bytes, Quantity::new(4096));
            assert_eq!(observed.net_bytes, Quantity::new(4096));
            assert_eq!(observed.output.len(), 4096);
        }

        #[test]
        fn scratch_released_inside_the_interval_counts_in_the_peak_alone() {
            let observed = observe(&AllocationCounter, 1024_usize, |size| {
                let scratch = std::hint::black_box(vec![0_u8; size]);
                scratch.len()
            })
            .unwrap();
            assert_eq!(observed.peak_bytes, Quantity::new(1024));
            assert_eq!(observed.net_bytes, Quantity::new(0));
            assert_eq!(observed.output, 1024);
        }

        #[test]
        fn two_scratch_buffers_one_after_the_other_peak_at_one() {
            // The peak is the largest live growth, not the sum of everything
            // the interval requested.
            let observed = observe(&AllocationCounter, 1024_usize, |size| {
                let first = std::hint::black_box(vec![0_u8; size]);
                drop(first);
                let second = std::hint::black_box(vec![0_u8; size]);
                second.len()
            })
            .unwrap();
            assert_eq!(observed.peak_bytes, Quantity::new(1024));
            assert_eq!(observed.net_bytes, Quantity::new(0));
        }

        #[test]
        fn the_observer_names_itself_apart_from_the_crate_it_counts_with() {
            let description = AllocationCounter::description();
            assert_eq!(description.provider, "allocation-counter");
            assert_eq!(
                description.observer(),
                VersionedIdentity::literal("intlify-measurement-allocation-counter-observer", "0")
            );
        }

        #[test]
        fn freeing_storage_from_before_the_interval_is_refused() {
            let before = vec![0_u8; 64];
            assert!(matches!(
                observe(&AllocationCounter, before, drop),
                Err(MemoryFailure::NegativeNet)
            ));
        }

        #[test]
        fn a_reallocation_counts_the_old_and_the_new_buffer_at_once() {
            // The provider's documented limitation, pinned: growing eight
            // bytes to sixteen holds twenty-four for a moment.
            let observed = observe(&AllocationCounter, 8_usize, |size| {
                let mut buffer = vec![0_u8; size];
                buffer.reserve_exact(size);
                buffer.capacity()
            })
            .unwrap();
            assert_eq!(observed.output, 16);
            assert_eq!(observed.peak_bytes, Quantity::new(24));
            assert_eq!(observed.net_bytes, Quantity::new(0));
        }
    }
}
