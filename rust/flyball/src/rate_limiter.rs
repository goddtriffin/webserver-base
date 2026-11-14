use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::{Decision, Error};

/// A keyed rate limiter. Each limiter enforces one [`Quota`](crate::Quota);
/// use one limiter per kind of limit (sign-ins per address, codes per email).
///
/// Keys are opaque. Prefer keys that don't hold personal data as-is (hash an
/// email address first), since a limiter may keep them in memory or send them
/// to a store.
pub trait RateLimiter: Send + Sync {
    /// Spends `cost` units of `key`'s quota if all of them are available.
    ///
    /// A refused call spends nothing, so callers that keep retrying too fast
    /// never push their own wait further out. A cost of zero reports
    /// [`Decision::Allowed`] with what is left, without spending or storing
    /// anything.
    fn check_n<'a>(&'a self, key: &'a str, cost: u32) -> Check<'a>;

    /// [`RateLimiter::check_n`] with a cost of one.
    fn check<'a>(&'a self, key: &'a str) -> Check<'a> {
        self.check_n(key, 1)
    }
}

/// The future a [`RateLimiter`] returns. Like every future it does nothing
/// until awaited.
///
/// A concrete type rather than a boxed future, so the trait stays usable as
/// `dyn RateLimiter` while the in-memory limiter answers without allocating.
#[must_use = "a check spends nothing until it is awaited"]
pub struct Check<'a>(State<'a>);

enum State<'a> {
    Local {
        limiter: &'a dyn Decide,
        key: &'a str,
        cost: u32,
    },
    Boxed(Pin<Box<dyn Future<Output = Result<Decision, Error>> + Send + 'a>>),
    Done,
}

/// A limiter that decides synchronously, in memory.
pub(crate) trait Decide: Sync {
    fn decide(&self, key: &str, cost: u32) -> Result<Decision, Error>;
}

impl<'a> Check<'a> {
    /// Wraps the future of a limiter that waits on I/O (a network store).
    pub fn new(future: impl Future<Output = Result<Decision, Error>> + Send + 'a) -> Self {
        Self(State::Boxed(Box::pin(future)))
    }

    pub(crate) fn local(limiter: &'a dyn Decide, key: &'a str, cost: u32) -> Self {
        Self(State::Local { limiter, key, cost })
    }
}

impl Future for Check<'_> {
    type Output = Result<Decision, Error>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this: &mut Self = self.get_mut();
        match std::mem::replace(&mut this.0, State::Done) {
            State::Local { limiter, key, cost } => Poll::Ready(limiter.decide(key, cost)),
            State::Boxed(mut future) => {
                let poll: Poll<Self::Output> = future.as_mut().poll(cx);
                if poll.is_pending() {
                    this.0 = State::Boxed(future);
                }
                poll
            }
            State::Done => panic!("a flyball Check was polled after it completed"),
        }
    }
}
