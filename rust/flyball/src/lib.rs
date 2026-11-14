//! Keyed rate limiting behind one trait.
//!
//! [`RateLimiter`] is the interface a project depends on;
//! [`InMemoryRateLimiter`] is the exact, memory-bounded implementation for a
//! single process; `MockRateLimiter` (feature `mock`) is for tests. A shared
//! store (Redis, Memcached) can implement the same trait later without
//! callers changing.
//!
//! ```
//! use std::num::NonZeroU32;
//!
//! use flyball::{Decision, InMemoryRateLimiter, Quota, RateLimiter};
//!
//! async fn may_sign_in(limiter: &dyn RateLimiter, address: &str) -> bool {
//!     matches!(limiter.check(address).await, Ok(Decision::Allowed { .. }))
//! }
//!
//! let limiter: InMemoryRateLimiter =
//!     InMemoryRateLimiter::new(Quota::per_minute(const { NonZeroU32::new(30).unwrap() }));
//! let _ = may_sign_in(&limiter, "203.0.113.7");
//! ```

mod clock;
mod decision;
mod in_memory_rate_limiter;
#[cfg(feature = "mock")]
mod mock_rate_limiter;
mod quota;
mod rate_limiter;

pub use clock::{Clock, ManualClock, MonotonicClock};
pub use decision::{Decision, Error};
pub use in_memory_rate_limiter::{DEFAULT_MAX_KEYS, InMemoryRateLimiter};
#[cfg(feature = "mock")]
pub use mock_rate_limiter::{CheckNArgs, MockCall, MockRateLimiter};
pub use quota::Quota;
pub use rate_limiter::{Check, RateLimiter};

#[cfg(test)]
mod test_support {
    use std::pin::pin;
    use std::task::{Context, Poll, Waker};

    /// Every future in this crate is ready on its first poll.
    pub fn ready<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        match future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
        {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("flyball futures never wait"),
        }
    }
}
