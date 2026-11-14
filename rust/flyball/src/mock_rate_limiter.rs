//! A seeded [`RateLimiter`] for tests. Neither limiter logs, so neither does
//! the mock.

use std::collections::VecDeque;
use std::fmt::Debug;
use std::sync::RwLock;

use crate::{Check, Decision, Error, RateLimiter};

/// The arguments of one [`RateLimiter::check_n`] call.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckNArgs {
    pub key: String,
    pub cost: u32,
}

/// One call the mock received, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum MockCall {
    CheckN(CheckNArgs),
}

type Seeds<A, T> = RwLock<Vec<(A, VecDeque<Result<T, Error>>)>>;

/// Answers each call with a response seeded for exactly its arguments, and
/// fails the test when dropped with a seeded response left unused.
///
/// [`RateLimiter::check`] is not seeded on its own: it calls `check_n` with a
/// cost of one, so seed that.
pub struct MockRateLimiter {
    check_n_seeds: Seeds<CheckNArgs, Decision>,
    calls: RwLock<Vec<MockCall>>,
}

impl MockRateLimiter {
    #[must_use]
    pub fn new() -> Self {
        Self {
            check_n_seeds: RwLock::new(Vec::new()),
            calls: RwLock::new(Vec::new()),
        }
    }

    /// Queues `response` for a `check_n(key, cost)` call. Seeding the same
    /// arguments again queues another response; calls take them in order.
    ///
    /// # Panics
    /// If a previous holder of the seed lock panicked.
    #[must_use]
    pub fn with_check_n_response(
        self,
        key: &str,
        cost: u32,
        response: Result<Decision, Error>,
    ) -> Self {
        let args: CheckNArgs = CheckNArgs {
            key: key.to_owned(),
            cost,
        };
        push_seed(&self.check_n_seeds, args, response);
        self
    }

    /// Every call received so far, in order.
    ///
    /// # Panics
    /// If a previous holder of the call-log lock panicked.
    #[must_use]
    pub fn recorded_calls(&self) -> Vec<MockCall> {
        self.calls.read().expect("mock call log poisoned").clone()
    }

    /// Fails unless every seeded response has been used. Also runs on drop;
    /// call it directly to check at a chosen point, or when the mock is held
    /// somewhere that outlives the test.
    ///
    /// # Panics
    /// When a seeded response is left, naming each one.
    pub fn assert_all_responses_consumed(&self) {
        let lines: Vec<String> = self.unconsumed_report();
        assert!(
            lines.is_empty(),
            "MockRateLimiter has unconsumed seeded responses:\n{}",
            lines.join("\n")
        );
    }

    fn unconsumed_report(&self) -> Vec<String> {
        let mut lines: Vec<String> = Vec::new();
        collect_unconsumed(&self.check_n_seeds, "MockRateLimiter::check_n", &mut lines);
        lines
    }
}

impl Default for MockRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter for MockRateLimiter {
    fn check_n<'a>(&'a self, key: &'a str, cost: u32) -> Check<'a> {
        Check::new(async move {
            let args: CheckNArgs = CheckNArgs {
                key: key.to_owned(),
                cost,
            };
            self.calls
                .write()
                .expect("mock call log poisoned")
                .push(MockCall::CheckN(args.clone()));
            take_seed(&self.check_n_seeds, &args).unwrap_or_else(|| {
                panic!(
                    "mock_no_response_configured: MockRateLimiter::check_n: no response seeded for args {args:?}"
                )
            })
        })
    }
}

impl Drop for MockRateLimiter {
    fn drop(&mut self) {
        // A second panic while the test is already failing would abort the
        // process and bury the assertion that actually failed.
        if std::thread::panicking() {
            return;
        }
        self.assert_all_responses_consumed();
    }
}

fn push_seed<A: PartialEq, T>(seeds: &Seeds<A, T>, args: A, response: Result<T, Error>) {
    let mut seeds = seeds.write().expect("mock seed lock poisoned");
    match seeds.iter_mut().find(|(existing, _)| *existing == args) {
        Some((_, queue)) => queue.push_back(response),
        None => seeds.push((args, VecDeque::from([response]))),
    }
}

fn take_seed<A: PartialEq, T>(seeds: &Seeds<A, T>, args: &A) -> Option<Result<T, Error>> {
    let mut seeds = seeds.write().expect("mock seed lock poisoned");
    seeds
        .iter_mut()
        .find(|(existing, _)| existing == args)
        .and_then(|(_, queue)| queue.pop_front())
}

fn collect_unconsumed<A: Debug, T>(seeds: &Seeds<A, T>, method: &str, lines: &mut Vec<String>) {
    let seeds = seeds.read().expect("mock seed lock poisoned");
    for (args, queue) in seeds.iter().filter(|(_, queue)| !queue.is_empty()) {
        lines.push(format!(
            "  {method}: {} unconsumed response(s) for args {args:?}",
            queue.len()
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::test_support::ready;

    fn args(key: &str, cost: u32) -> CheckNArgs {
        CheckNArgs {
            key: key.to_owned(),
            cost,
        }
    }

    #[test]
    fn returns_the_seeded_response_and_records_the_call() {
        let mock: MockRateLimiter = MockRateLimiter::new().with_check_n_response(
            "ip:1",
            2,
            Ok(Decision::Allowed { remaining: 3 }),
        );
        let expected: Decision = Decision::Allowed { remaining: 3 };
        let actual: Decision = ready(mock.check_n("ip:1", 2)).unwrap();
        assert_eq!(expected, actual);
        let expected: Vec<MockCall> = vec![MockCall::CheckN(args("ip:1", 2))];
        assert_eq!(expected, mock.recorded_calls());
    }

    #[test]
    fn check_is_check_n_with_a_cost_of_one() {
        let mock: MockRateLimiter =
            MockRateLimiter::new().with_check_n_response("ip:1", 1, Ok(Decision::Never));
        assert_eq!(Decision::Never, ready(mock.check("ip:1")).unwrap());
        let expected: Vec<MockCall> = vec![MockCall::CheckN(args("ip:1", 1))];
        assert_eq!(expected, mock.recorded_calls());
    }

    #[test]
    fn seeded_errors_are_returned() {
        let mock: MockRateLimiter =
            MockRateLimiter::new().with_check_n_response("ip:1", 1, Err(Error::Full));
        assert!(matches!(ready(mock.check("ip:1")), Err(Error::Full)));
    }

    #[test]
    #[should_panic(expected = "no response seeded for args")]
    fn an_unseeded_call_panics() {
        let mock: MockRateLimiter = MockRateLimiter::new();
        let _: Result<Decision, Error> = ready(mock.check_n("ip:1", 1));
    }

    #[test]
    #[should_panic(expected = "no response seeded for args")]
    fn a_call_with_other_arguments_than_seeded_panics() {
        let mock: MockRateLimiter =
            MockRateLimiter::new().with_check_n_response("ip:1", 1, Ok(Decision::Never));
        let _: Result<Decision, Error> = ready(mock.check_n("ip:1", 2));
    }

    #[test]
    fn responses_for_the_same_arguments_come_back_in_seeded_order() {
        let limited: Decision = Decision::Limited {
            retry_after: Duration::from_secs(1),
        };
        let mock: MockRateLimiter = MockRateLimiter::new()
            .with_check_n_response("ip:1", 1, Ok(Decision::Allowed { remaining: 0 }))
            .with_check_n_response("ip:1", 1, Ok(limited));
        assert_eq!(
            Decision::Allowed { remaining: 0 },
            ready(mock.check("ip:1")).unwrap()
        );
        assert_eq!(limited, ready(mock.check("ip:1")).unwrap());
    }

    #[test]
    #[should_panic(expected = "unconsumed seeded responses")]
    fn an_unused_seed_fails_the_check() {
        let mock: MockRateLimiter =
            MockRateLimiter::new().with_check_n_response("ip:1", 1, Ok(Decision::Never));
        mock.assert_all_responses_consumed();
    }

    #[test]
    #[should_panic(expected = "unconsumed seeded responses")]
    fn an_unused_seed_fails_the_test_on_drop() {
        let _mock: MockRateLimiter =
            MockRateLimiter::new().with_check_n_response("ip:1", 1, Ok(Decision::Never));
    }
}
