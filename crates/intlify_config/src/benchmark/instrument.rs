// @license MIT
// @author kazuya kawaguchi (a.k.a. kazupon)

//! What one interval is measured with.
//!
//! A duration run reads its clock at the interval's two marks. A memory run
//! counts what the interval requests through the global allocator between an
//! observer's marks. Either way the interval is the same production boundary:
//! the input is prepared before the start mark and the output is retained past
//! the end mark. How each mark is read belongs to `intlify_measurement`; the
//! observer is a seam this owner is handed, so no allocator is named here.

use intlify_measurement::acquisition::memory::{
    observe, MemoryFailure, MemoryObserver, ObserverDescription,
};
use intlify_measurement::measurement::{Aggregation, Metric};

use super::clock::{Clock, ClockDescription, MonotonicClock};
use super::measure::{measure, MeasurementFailure};
use super::quantity::Quantity;

/// One interval's quantity and the output it retained.
pub(super) struct Interval<Output> {
    pub(super) quantity: Quantity,
    pub(super) output: Output,
}

/// What one interval can be measured with.
pub(super) trait Instrument {
    /// Why an interval yields no quantity.
    type Failure;

    /// How a sample aggregates its repetitions' quantities.
    const AGGREGATION: Aggregation;

    /// Measure exactly one invocation.
    fn interval<Input, Output>(
        &self,
        input: Input,
        invoke: fn(Input) -> Output,
    ) -> Result<Interval<Output>, Self::Failure>;

    /// Why an interval whose prerequisite the preparation lacks yields none.
    fn unavailable() -> Self::Failure;
}

/// A sample's aggregate once one more repetition's quantity is in it, or
/// `None` when the exact quantity domain cannot hold it. A peak is the largest
/// quantity, never their sum and never divided by the repetition count.
pub(super) fn aggregate(kind: Aggregation, total: Quantity, next: Quantity) -> Option<Quantity> {
    match kind {
        Aggregation::BatchTotal => total.checked_add(next).ok(),
        Aggregation::PeakOverBatch => Some(total.max(next)),
    }
}

/// A duration run's clock. An interval's quantity is its nanoseconds, and a
/// sample's is their checked sum (`batch_total`).
pub(super) struct Timed<'c, C>(pub(super) &'c C);

impl<C: Clock> Instrument for Timed<'_, C> {
    type Failure = MeasurementFailure;
    const AGGREGATION: Aggregation = Aggregation::BatchTotal;

    fn interval<Input, Output>(
        &self,
        input: Input,
        invoke: fn(Input) -> Output,
    ) -> Result<Interval<Output>, MeasurementFailure> {
        let measured = measure(self.0, input, invoke)?;
        Ok(Interval {
            quantity: measured.duration,
            output: measured.output,
        })
    }

    fn unavailable() -> MeasurementFailure {
        MeasurementFailure::PrerequisiteUnavailable
    }
}

/// A memory run's observer. An interval's quantity is its peak live bytes,
/// and a sample's is the largest of them (`peak_over_batch`): never their sum,
/// and never divided by the repetition count.
pub(super) struct Peaked<'o>(pub(super) &'o dyn MemoryObserver);

/// Why a memory interval yields no peak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PeakFailure {
    PrerequisiteUnavailable,
    Memory(MemoryFailure),
}

impl Instrument for Peaked<'_> {
    type Failure = PeakFailure;
    const AGGREGATION: Aggregation = Aggregation::PeakOverBatch;

    fn interval<Input, Output>(
        &self,
        input: Input,
        invoke: fn(Input) -> Output,
    ) -> Result<Interval<Output>, PeakFailure> {
        let observed = observe(self.0, input, invoke).map_err(PeakFailure::Memory)?;
        Ok(Interval {
            quantity: observed.peak_bytes,
            output: observed.output,
        })
    }

    fn unavailable() -> PeakFailure {
        PeakFailure::PrerequisiteUnavailable
    }
}

/// The instrument a run measures every interval with, acquired before its
/// first case. An observer comes with the description it gives of itself.
pub(super) enum RunInstrument {
    Clock(MonotonicClock),
    Observer {
        observer: Box<dyn MemoryObserver>,
        description: ObserverDescription,
    },
}

/// What a run's instrument says about itself. Every descriptor the run
/// records is checked against it, never against what a record claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum InstrumentDescription {
    Clock(ClockDescription),
    Observer(ObserverDescription),
}

impl RunInstrument {
    pub(super) fn description(&self) -> InstrumentDescription {
        match self {
            Self::Clock(clock) => InstrumentDescription::Clock(clock.description()),
            Self::Observer { description, .. } => InstrumentDescription::Observer(*description),
        }
    }
}

impl InstrumentDescription {
    /// The metric every interval of the run measures.
    pub(super) const fn metric(self) -> Metric {
        match self {
            Self::Clock(_) => Metric::WallDuration,
            Self::Observer(_) => Metric::PeakLiveBytes,
        }
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::benchmark::clock::tests::ScriptedClock;
    use intlify_measurement::acquisition::memory::Allocation;
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    /// An observer whose reports the test writes in advance.
    ///
    /// A real observer cannot report a contradictory interval on demand, so
    /// the cases that must not become a peak need this double. Once the
    /// written reports run out it repeats its last one, for runs too long to
    /// write out.
    pub(crate) struct ScriptedObserver {
        reports: RefCell<VecDeque<Allocation>>,
        last: Cell<Option<Allocation>>,
        runs: bool,
        intervals: Cell<usize>,
    }

    impl ScriptedObserver {
        pub(crate) fn new(reports: impl IntoIterator<Item = Allocation>) -> Self {
            Self {
                reports: RefCell::new(reports.into_iter().collect()),
                last: Cell::new(None),
                runs: true,
                intervals: Cell::new(0),
            }
        }

        /// An observer that returns without running the interval.
        pub(crate) fn idle(report: Allocation) -> Self {
            Self {
                runs: false,
                ..Self::new([report])
            }
        }

        pub(crate) fn intervals(&self) -> usize {
            self.intervals.get()
        }
    }

    impl MemoryObserver for ScriptedObserver {
        fn observe(&self, interval: &mut dyn FnMut()) -> Allocation {
            if self.runs {
                self.intervals.set(self.intervals.get() + 1);
                interval();
            }
            let report = self.reports.borrow_mut().pop_front();
            let report = report.or(self.last.get()).expect("unexpected observation");
            self.last.set(Some(report));
            report
        }
    }

    pub(crate) fn report(peak_bytes: u64, net_bytes: i64) -> Allocation {
        Allocation {
            peak_bytes,
            net_bytes,
        }
    }

    pub(crate) const DESCRIPTION: ObserverDescription = ObserverDescription {
        provider: "test-provider",
        provider_revision: "9.9.9",
        identity: "intlify-config-test-observer",
        revision: "0",
        counting: "scripted-reports",
        limitations: &["reports-are-written-by-the-test"],
    };

    #[test]
    fn a_duration_interval_is_its_nanoseconds_and_a_sample_their_checked_sum() {
        type Duration<'c> = Timed<'c, ScriptedClock>;
        let clock = ScriptedClock::nanos([20, 31]);
        let interval = Timed(&clock).interval(7_u8, |input| input + 1).unwrap();
        assert_eq!(interval.quantity, Quantity::new(11));
        assert_eq!(interval.output, 8);
        assert_eq!(clock.remaining(), 0);
        assert_eq!(Duration::AGGREGATION, Aggregation::BatchTotal);
        assert_eq!(
            aggregate(Duration::AGGREGATION, Quantity::new(5), Quantity::new(6)),
            Some(Quantity::new(11))
        );
        assert_eq!(
            aggregate(
                Duration::AGGREGATION,
                Quantity::new(u64::MAX),
                Quantity::new(1)
            ),
            None
        );
        assert_eq!(
            Duration::unavailable(),
            MeasurementFailure::PrerequisiteUnavailable
        );
    }

    #[test]
    fn a_memory_interval_is_its_peak_and_a_sample_the_largest_peak() {
        let observer = ScriptedObserver::new([report(4096, 64)]);
        let interval = Peaked(&observer).interval(7_u8, |input| input + 1).unwrap();
        assert_eq!(interval.quantity, Quantity::new(4096));
        assert_eq!(interval.output, 8);
        assert_eq!(observer.intervals(), 1);
        assert_eq!(Peaked::AGGREGATION, Aggregation::PeakOverBatch);
        for (total, next) in [(4096, 1024), (1024, 4096)] {
            assert_eq!(
                aggregate(
                    Peaked::AGGREGATION,
                    Quantity::new(total),
                    Quantity::new(next)
                ),
                Some(Quantity::new(4096))
            );
        }
        assert_eq!(
            aggregate(
                Peaked::AGGREGATION,
                Quantity::new(u64::MAX),
                Quantity::new(u64::MAX)
            ),
            Some(Quantity::new(u64::MAX))
        );
        assert_eq!(Peaked::unavailable(), PeakFailure::PrerequisiteUnavailable);
    }

    #[test]
    fn a_memory_interval_that_frees_earlier_storage_panics_or_never_runs_has_no_peak() {
        let observer = ScriptedObserver::new([report(128, -1)]);
        assert_eq!(
            Peaked(&observer).interval((), |()| ()).err(),
            Some(PeakFailure::Memory(MemoryFailure::NegativeNet))
        );
        let observer = ScriptedObserver::new([report(0, 0)]);
        assert_eq!(
            Peaked(&observer)
                .interval((), |()| panic!("test-owned failure"))
                .err(),
            Some(PeakFailure::Memory(MemoryFailure::InvocationPanicked))
        );
        let observer = ScriptedObserver::idle(report(0, 0));
        assert_eq!(
            Peaked(&observer).interval((), |()| ()).err(),
            Some(PeakFailure::Memory(MemoryFailure::NotInvoked))
        );
        assert_eq!(observer.intervals(), 0);
    }

    #[test]
    fn a_scripted_observer_repeats_its_last_report_once_the_script_runs_out() {
        let observer = ScriptedObserver::new([report(1, 0), report(2, 0)]);
        let peaks = (0..4)
            .map(|_| Peaked(&observer).interval((), |()| ()).unwrap().quantity)
            .collect::<Vec<_>>();
        assert_eq!(peaks, [1, 2, 2, 2].map(Quantity::new));
        assert_eq!(observer.intervals(), 4);
    }

    #[test]
    fn an_observer_is_described_as_it_describes_itself() {
        let instrument = RunInstrument::Observer {
            observer: Box::new(ScriptedObserver::new([report(0, 0)])),
            description: DESCRIPTION,
        };
        assert_eq!(
            instrument.description(),
            InstrumentDescription::Observer(DESCRIPTION)
        );
        assert_eq!(instrument.description().metric(), Metric::PeakLiveBytes);
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn a_clock_is_described_as_it_was_acquired() {
        let clock = MonotonicClock::acquire().unwrap();
        let description = clock.description();
        assert_eq!(
            RunInstrument::Clock(clock).description(),
            InstrumentDescription::Clock(description)
        );
        assert_eq!(
            InstrumentDescription::Clock(description).metric(),
            Metric::WallDuration
        );
    }
}
