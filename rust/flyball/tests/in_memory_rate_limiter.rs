//! Behaviour of `InMemoryRateLimiter`, driven through the `RateLimiter` trait
//! with a `ManualClock`, so every expectation is exact.

use std::num::{NonZeroU32, NonZeroUsize};
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};
use std::time::Duration;

use flyball::{
    Clock, Decision, Error, InMemoryRateLimiter, ManualClock, MonotonicClock, Quota, RateLimiter,
};

fn ready<F: Future>(future: F) -> F::Output {
    let mut future = pin!(future);
    match future
        .as_mut()
        .poll(&mut Context::from_waker(Waker::noop()))
    {
        Poll::Ready(output) => output,
        Poll::Pending => panic!("flyball futures never wait"),
    }
}

fn n(count: u32) -> NonZeroU32 {
    NonZeroU32::new(count).unwrap()
}

/// 3 per second: one unit back every 333,333,334 ns, burst 3.
fn three_per_second() -> (InMemoryRateLimiter<ManualClock>, ManualClock) {
    let clock: ManualClock = ManualClock::new();
    let limiter: InMemoryRateLimiter<ManualClock> =
        InMemoryRateLimiter::new(Quota::per_second(n(3))).with_clock(clock.clone());
    (limiter, clock)
}

fn check(limiter: &dyn RateLimiter, key: &str) -> Decision {
    ready(limiter.check(key)).unwrap()
}

fn check_n(limiter: &dyn RateLimiter, key: &str, cost: u32) -> Decision {
    ready(limiter.check_n(key, cost)).unwrap()
}

const INTERVAL: Duration = Duration::from_nanos(333_333_334);

#[test]
fn a_fresh_key_gets_exactly_its_burst_at_once() {
    let (limiter, _clock) = three_per_second();
    let expected: Vec<Decision> = vec![
        Decision::Allowed { remaining: 2 },
        Decision::Allowed { remaining: 1 },
        Decision::Allowed { remaining: 0 },
        Decision::Limited {
            retry_after: INTERVAL,
        },
    ];
    let actual: Vec<Decision> = (0..4).map(|_| check(&limiter, "a")).collect();
    assert_eq!(expected, actual);
}

#[test]
fn retry_after_is_exactly_when_the_same_call_passes() {
    let (limiter, clock) = three_per_second();
    for _ in 0..3 {
        assert!(check(&limiter, "a").is_allowed());
    }
    clock.advance(INTERVAL.saturating_sub(Duration::from_nanos(1)));
    let expected: Decision = Decision::Limited {
        retry_after: Duration::from_nanos(1),
    };
    assert_eq!(expected, check(&limiter, "a"));
    clock.advance(Duration::from_nanos(1));
    assert_eq!(Decision::Allowed { remaining: 0 }, check(&limiter, "a"));
}

#[test]
fn retry_after_accounts_for_the_whole_cost() {
    let (limiter, clock) = three_per_second();
    assert!(check_n(&limiter, "a", 3).is_allowed());
    clock.advance(INTERVAL);
    let expected: Decision = Decision::Limited {
        retry_after: INTERVAL,
    };
    assert_eq!(expected, check_n(&limiter, "a", 2));
    clock.advance(INTERVAL);
    assert_eq!(
        Decision::Allowed { remaining: 0 },
        check_n(&limiter, "a", 2)
    );
}

#[test]
fn refused_calls_never_push_the_wait_further_out() {
    // The bug this guards against: a limiter that resets its refill clock on
    // every call starves a caller that polls faster than one interval, forever.
    let clock: ManualClock = ManualClock::new();
    let limiter: InMemoryRateLimiter<ManualClock> =
        InMemoryRateLimiter::new(Quota::per_second(n(4))).with_clock(clock.clone());
    for _ in 0..4 {
        assert!(check(&limiter, "a").is_allowed());
    }
    let mut allowed: u32 = 0;
    // Poll every quarter interval for 12 intervals: the rate still delivers 12.
    for _ in 0..48 {
        clock.advance(Duration::from_micros(62_500));
        if check(&limiter, "a").is_allowed() {
            allowed += 1;
        }
    }
    assert_eq!(12, allowed);
}

#[test]
fn over_any_span_a_key_gets_its_burst_plus_the_rate_and_never_more() {
    let (limiter, clock) = three_per_second();
    let step: Duration = Duration::from_millis(7);
    let steps: u32 = 10_000;
    let mut allowed: u64 = 0;
    for _ in 0..steps {
        if check(&limiter, "a").is_allowed() {
            allowed += 1;
        }
        clock.advance(step);
    }
    let elapsed: u128 = (step * (steps - 1)).as_nanos();
    let ceiling: u64 = 3 + u64::try_from(elapsed / INTERVAL.as_nanos()).unwrap();
    assert!(allowed <= ceiling, "{allowed} > {ceiling}");
    // Polling every 7 ms wastes at most one step per unit, never a whole unit.
    assert!(allowed + 1 >= ceiling, "{allowed} far below {ceiling}");
}

#[test]
fn keys_are_independent() {
    let (limiter, _clock) = three_per_second();
    assert!(check_n(&limiter, "a", 3).is_allowed());
    assert!(!check(&limiter, "a").is_allowed());
    assert_eq!(Decision::Allowed { remaining: 2 }, check(&limiter, "b"));
}

#[test]
fn cost_spends_that_many_units_at_once() {
    let (limiter, _clock) = three_per_second();
    assert_eq!(
        Decision::Allowed { remaining: 1 },
        check_n(&limiter, "a", 2)
    );
    let expected: Decision = Decision::Limited {
        retry_after: INTERVAL,
    };
    assert_eq!(expected, check_n(&limiter, "a", 2));
    assert_eq!(
        Decision::Allowed { remaining: 0 },
        check_n(&limiter, "a", 1)
    );
}

#[test]
fn a_cost_above_the_burst_is_never_allowed_and_spends_nothing() {
    let (limiter, _clock) = three_per_second();
    assert_eq!(Decision::Never, check_n(&limiter, "a", 4));
    assert_eq!(Decision::Never, check_n(&limiter, "a", u32::MAX));
    assert!(limiter.is_empty());
    assert_eq!(
        Decision::Allowed { remaining: 0 },
        check_n(&limiter, "a", 3)
    );
}

#[test]
fn a_zero_cost_reports_what_is_left_without_spending_or_storing() {
    let (limiter, _clock) = three_per_second();
    assert_eq!(
        Decision::Allowed { remaining: 3 },
        check_n(&limiter, "a", 0)
    );
    assert!(limiter.is_empty());
    assert!(check(&limiter, "a").is_allowed());
    assert_eq!(
        Decision::Allowed { remaining: 2 },
        check_n(&limiter, "a", 0)
    );
    assert_eq!(Decision::Allowed { remaining: 1 }, check(&limiter, "a"));
}

#[test]
fn an_idle_key_is_forgotten_once_its_quota_is_fully_back() {
    let (limiter, clock) = three_per_second();
    assert!(check(&limiter, "a").is_allowed());
    assert!(check(&limiter, "b").is_allowed());
    assert_eq!(2, limiter.len());
    clock.advance(INTERVAL.saturating_sub(Duration::from_nanos(1)));
    limiter.prune();
    assert_eq!(2, limiter.len());
    clock.advance(Duration::from_nanos(1));
    limiter.prune();
    assert!(limiter.is_empty());
    // Forgetting changes nothing a caller can see.
    assert_eq!(Decision::Allowed { remaining: 2 }, check(&limiter, "a"));
}

#[test]
fn inserts_sweep_idle_keys_so_memory_tracks_active_keys() {
    let clock: ManualClock = ManualClock::new();
    let limiter: InMemoryRateLimiter<ManualClock> =
        InMemoryRateLimiter::new(Quota::per_second(n(3)))
            .with_max_keys(NonZeroUsize::new(1_000_000).unwrap())
            .with_clock(clock.clone());
    for i in 0..200_000 {
        assert!(check(&limiter, &format!("old:{i}")).is_allowed());
    }
    clock.advance(Duration::from_secs(1));
    for i in 0..200_000 {
        assert!(check(&limiter, &format!("new:{i}")).is_allowed());
    }
    // Every old key went idle; sweeps keep the table near the live set.
    let len: usize = limiter.len();
    assert!(len < 300_000, "{len} keys stored for 200,000 active");
}

#[test]
fn max_keys_is_a_hard_cap_and_only_new_keys_are_refused() {
    let clock: ManualClock = ManualClock::new();
    let limiter: InMemoryRateLimiter<ManualClock> =
        InMemoryRateLimiter::new(Quota::per_second(n(3)))
            .with_max_keys(NonZeroUsize::new(2).unwrap())
            .with_clock(clock.clone());
    assert!(check(&limiter, "a").is_allowed());
    assert!(check(&limiter, "b").is_allowed());
    assert!(matches!(ready(limiter.check("c")), Err(Error::Full)));
    assert_eq!(2, limiter.len());
    // Tracked keys keep working, and a zero cost never needs a slot.
    assert_eq!(Decision::Allowed { remaining: 1 }, check(&limiter, "a"));
    assert_eq!(
        Decision::Allowed { remaining: 3 },
        check_n(&limiter, "c", 0)
    );
    // "b" goes idle first ("a" spent twice), which frees exactly one slot.
    clock.advance(INTERVAL);
    assert!(check(&limiter, "c").is_allowed());
    assert!(matches!(ready(limiter.check("d")), Err(Error::Full)));
}

#[test]
fn concurrent_callers_cannot_spend_more_than_the_burst() {
    let clock: ManualClock = ManualClock::new();
    let limiter: Arc<InMemoryRateLimiter<ManualClock>> =
        Arc::new(InMemoryRateLimiter::new(Quota::per_minute(n(100))).with_clock(clock));
    let threads: Vec<std::thread::JoinHandle<u32>> = (0..8)
        .map(|_| {
            let limiter: Arc<InMemoryRateLimiter<ManualClock>> = Arc::clone(&limiter);
            std::thread::spawn(move || {
                let mut allowed: u32 = 0;
                for _ in 0..1_000 {
                    if check(limiter.as_ref(), "shared").is_allowed() {
                        allowed += 1;
                    }
                }
                allowed
            })
        })
        .collect();
    let total: u32 = threads.into_iter().map(|t| t.join().unwrap()).sum();
    assert_eq!(100, total);
}

#[test]
fn concurrent_keys_each_get_their_own_burst() {
    let limiter: Arc<InMemoryRateLimiter<ManualClock>> =
        Arc::new(InMemoryRateLimiter::new(Quota::per_minute(n(10))).with_clock(ManualClock::new()));
    let threads: Vec<std::thread::JoinHandle<u32>> = (0..8)
        .map(|t| {
            let limiter: Arc<InMemoryRateLimiter<ManualClock>> = Arc::clone(&limiter);
            std::thread::spawn(move || {
                let mut allowed: u32 = 0;
                for i in 0..2_000 {
                    if check(limiter.as_ref(), &format!("{t}:{}", i % 100)).is_allowed() {
                        allowed += 1;
                    }
                }
                allowed
            })
        })
        .collect();
    for thread in threads {
        assert_eq!(100 * 10, thread.join().unwrap());
    }
}

#[test]
fn a_clock_at_its_limit_stops_time_instead_of_overflowing() {
    let clock: ManualClock = ManualClock::new();
    let limiter: InMemoryRateLimiter<ManualClock> =
        InMemoryRateLimiter::new(Quota::per_day(NonZeroU32::MAX)).with_clock(clock.clone());
    clock.advance(Duration::MAX);
    assert!(check_n(&limiter, "a", u32::MAX).is_allowed());
    assert!(!check(&limiter, "a").is_allowed());
    clock.advance(Duration::from_secs(1));
    assert!(!check(&limiter, "a").is_allowed());
}

#[test]
fn the_largest_quota_and_cost_do_not_overflow() {
    let clock: ManualClock = ManualClock::new();
    let quota: Quota = Quota::new(n(1), Duration::from_hours(24 * 365 * 100))
        .unwrap()
        .with_burst(n(1))
        .unwrap();
    let limiter: InMemoryRateLimiter<ManualClock> =
        InMemoryRateLimiter::new(quota).with_clock(clock.clone());
    assert!(check(&limiter, "a").is_allowed());
    let expected: Decision = Decision::Limited {
        retry_after: quota.interval(),
    };
    assert_eq!(expected, check(&limiter, "a"));
}

#[test]
fn works_behind_dyn_and_across_await_points() {
    let limiter: Arc<dyn RateLimiter> = Arc::new(InMemoryRateLimiter::new(Quota::per_second(n(1))));
    let first: Decision = ready(async { limiter.check("a").await }).unwrap();
    assert_eq!(Decision::Allowed { remaining: 0 }, first);
    assert!(!ready(limiter.check("a")).unwrap().is_allowed());
}

#[test]
fn a_check_does_nothing_until_awaited() {
    let (limiter, _clock) = three_per_second();
    let future = limiter.check("a");
    drop(future);
    assert!(limiter.is_empty());
}

#[test]
fn the_monotonic_clock_is_the_default() {
    let limiter: InMemoryRateLimiter<MonotonicClock> =
        InMemoryRateLimiter::new(Quota::per_second(n(1)));
    assert!(MonotonicClock::new().now() < Duration::from_secs(60));
    assert_eq!(Quota::per_second(n(1)), limiter.quota());
}

#[test]
fn errors_say_what_to_do() {
    let full: String = Error::Full.to_string();
    assert!(full.contains("treat the request as limited"), "{full}");
    let backend: Error = Error::Backend("connection reset".into());
    assert_eq!(
        "rate limiter backend failed: connection reset",
        backend.to_string()
    );
    assert!(std::error::Error::source(&backend).is_some());
}
