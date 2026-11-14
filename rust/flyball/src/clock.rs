use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Where a limiter reads the time.
pub trait Clock: Send + Sync {
    /// Time since a fixed origin. Must never go backwards: a clock that does
    /// hands out quota a second time.
    fn now(&self) -> Duration;
}

/// The process's monotonic clock. Wall-clock time is never used, so a system
/// clock change can neither reset nor freeze anyone's quota.
#[derive(Clone, Copy, Debug)]
pub struct MonotonicClock {
    origin: Instant,
}

impl MonotonicClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            origin: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now(&self) -> Duration {
        self.origin.elapsed()
    }
}

/// A clock that only moves when told to, for tests. Clones share one time, so
/// a test keeps a clone and advances the limiter's clock through it.
#[derive(Clone, Debug, Default)]
pub struct ManualClock {
    nanos: Arc<AtomicU64>,
}

impl ManualClock {
    /// Starts at zero.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Moves time forward, saturating at `u64::MAX` nanoseconds.
    pub fn advance(&self, by: Duration) {
        let by: u64 = u64::try_from(by.as_nanos()).unwrap_or(u64::MAX);
        // `try_update` only fails when the closure returns `None`; this one never does.
        let _: Result<u64, u64> =
            self.nanos
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |now: u64| {
                    Some(now.saturating_add(by))
                });
    }
}

impl Clock for ManualClock {
    fn now(&self) -> Duration {
        Duration::from_nanos(self.nanos.load(Ordering::SeqCst))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_of_a_manual_clock_share_one_time() {
        let clock: ManualClock = ManualClock::new();
        let clone: ManualClock = clock.clone();
        clone.advance(Duration::from_secs(2));
        assert_eq!(Duration::from_secs(2), clock.now());
    }

    #[test]
    fn a_manual_clock_saturates_instead_of_wrapping() {
        let clock: ManualClock = ManualClock::new();
        clock.advance(Duration::MAX);
        clock.advance(Duration::from_secs(1));
        assert_eq!(Duration::from_nanos(u64::MAX), clock.now());
    }

    #[test]
    fn the_monotonic_clock_never_goes_backwards() {
        let clock: MonotonicClock = MonotonicClock::new();
        let first: Duration = clock.now();
        let second: Duration = clock.now();
        assert!(second >= first);
    }
}
