use std::num::NonZeroU32;
use std::time::Duration;

/// The longest burst window a [`Quota`] may span: a quarter of what a `u64` of
/// nanoseconds holds (~146 years). The headroom is what lets the limiter add a
/// whole window to "now" without ever overflowing.
pub(crate) const MAX_TOLERANCE: u64 = u64::MAX / 4;

/// How much one key may spend: a sustained rate plus a burst.
///
/// `count` per `period` is the sustained rate. `burst` is how much an idle key
/// may spend back to back; it defaults to `count`.
///
/// The exact guarantee: over any span of time `w`, one key is allowed at most
/// `burst + w / interval` units. So with the default burst a key that has been
/// idle can spend a full period's worth at once and then keep pace with the
/// rate, which means up to `2 × count` inside one `period` when that period
/// starts right after a quiet one. Use [`Quota::with_burst`] to tighten that.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Quota {
    /// Nanoseconds one unit of cost occupies. Rounded up, so the sustained rate
    /// can only ever be a hair under `count / period`, never over it.
    interval: u64,
    burst: u32,
}

impl Quota {
    /// `count` per `period`, with a burst of `count`.
    ///
    /// `None` when `period` is zero or longer than ~146 years.
    #[must_use]
    pub fn new(count: NonZeroU32, period: Duration) -> Option<Self> {
        let period: u64 = u64::try_from(period.as_nanos())
            .ok()
            .filter(|&nanos: &u64| nanos > 0)?;
        Self::spread(count, period).checked()
    }

    #[must_use]
    pub fn per_second(count: NonZeroU32) -> Self {
        Self::spread(count, 1_000_000_000)
    }

    #[must_use]
    pub fn per_minute(count: NonZeroU32) -> Self {
        Self::spread(count, 60 * 1_000_000_000)
    }

    #[must_use]
    pub fn per_hour(count: NonZeroU32) -> Self {
        Self::spread(count, 3_600 * 1_000_000_000)
    }

    #[must_use]
    pub fn per_day(count: NonZeroU32) -> Self {
        Self::spread(count, 86_400 * 1_000_000_000)
    }

    /// The same rate with a different burst.
    ///
    /// `None` when `burst` units would span more than ~146 years.
    #[must_use]
    pub fn with_burst(self, burst: NonZeroU32) -> Option<Self> {
        Self {
            burst: burst.get(),
            ..self
        }
        .checked()
    }

    /// How much an idle key may spend at once. A single call costing more than
    /// this is answered [`Decision::Never`](crate::Decision::Never).
    #[must_use]
    pub const fn burst(&self) -> u32 {
        self.burst
    }

    /// The time one unit of cost takes to come back.
    #[must_use]
    pub const fn interval(&self) -> Duration {
        Duration::from_nanos(self.interval)
    }

    pub(crate) const fn interval_nanos(&self) -> u64 {
        self.interval
    }

    /// The burst window in nanoseconds. Never above [`MAX_TOLERANCE`], which
    /// every constructor enforces.
    pub(crate) fn tolerance_nanos(&self) -> u64 {
        self.interval * u64::from(self.burst)
    }

    /// Infallible for every period up to a day: `count × ceil(period / count)`
    /// is at most `period + count`, far below [`MAX_TOLERANCE`].
    fn spread(count: NonZeroU32, period: u64) -> Self {
        Self {
            interval: period.div_ceil(u64::from(count.get())),
            burst: count.get(),
        }
    }

    fn checked(self) -> Option<Self> {
        u64::from(self.burst)
            .checked_mul(self.interval)
            .filter(|&tolerance: &u64| tolerance <= MAX_TOLERANCE)
            .map(|_| self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THREE: NonZeroU32 = NonZeroU32::new(3).unwrap();

    #[test]
    fn the_interval_rounds_up_so_the_rate_never_exceeds_the_quota() {
        let quota: Quota = Quota::per_second(THREE);
        assert_eq!(Duration::from_nanos(333_333_334), quota.interval());
        assert!(quota.interval() * 3 >= Duration::from_secs(1));
    }

    #[test]
    fn more_than_one_per_nanosecond_still_gets_a_nonzero_interval() {
        let quota: Option<Quota> = Quota::new(NonZeroU32::MAX, Duration::from_nanos(1));
        let expected: Option<Duration> = Some(Duration::from_nanos(1));
        assert_eq!(expected, quota.map(|q: Quota| q.interval()));
    }

    #[test]
    fn a_zero_period_is_rejected() {
        assert_eq!(None, Quota::new(THREE, Duration::ZERO));
    }

    #[test]
    fn a_window_too_long_to_add_to_now_is_rejected() {
        assert_eq!(None, Quota::new(THREE, Duration::MAX));
        let daily: Quota = Quota::per_day(NonZeroU32::new(1).unwrap());
        assert_eq!(None, daily.with_burst(NonZeroU32::MAX));
    }

    #[test]
    fn the_burst_defaults_to_the_count_and_can_be_changed() {
        let quota: Quota = Quota::per_minute(THREE);
        assert_eq!(3, quota.burst());
        let expected: Option<u32> = Some(1);
        let actual: Option<u32> = quota
            .with_burst(NonZeroU32::new(1).unwrap())
            .map(|q: Quota| q.burst());
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_largest_daily_quota_fits() {
        let quota: Quota = Quota::per_day(NonZeroU32::MAX);
        assert!(quota.tolerance_nanos() <= MAX_TOLERANCE);
    }
}
