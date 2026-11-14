# flyball

Keyed rate limiting behind one trait, with zero dependencies. Part of
[toddstack](https://github.com/goddtriffin/toddstack).

Every project depends on the `RateLimiter` trait and picks an implementation at
startup:

| type | use it for |
|---|---|
| `InMemoryRateLimiter` | one process: exact, memory-bounded, no network |
| `MockRateLimiter` (feature `mock`) | tests: seeded answers, recorded calls |

A shared store (Redis, Memcached) can implement the same trait later, for
projects that run several replicas, without any caller changing.

## Install

```toml
[dependencies]
flyball = "0.1"

[dev-dependencies]
flyball = { version = "0.1", features = ["mock"] }
```

Enable `mock` under `[dev-dependencies]` only. Cargo unifies features, so
enabling it under `[dependencies]` ships the mock in the production binary.

## Use

One limiter per kind of limit. Each enforces one `Quota` on every key it sees.

```rust
use std::num::NonZeroU32;
use flyball::{Decision, InMemoryRateLimiter, Quota, RateLimiter};

let sign_ins = InMemoryRateLimiter::new(Quota::per_minute(NonZeroU32::new(30).unwrap()));

match sign_ins.check(&client_ip).await {
    Ok(Decision::Allowed { remaining }) => { /* go on */ }
    Ok(Decision::Limited { retry_after }) => { /* 429, Retry-After */ }
    Ok(Decision::Never) => { /* the cost exceeds the burst: can never pass */ }
    Err(error) => { /* undecided: refuse for sign-ins, maybe allow for cheap reads */ }
}
```

- `check_n(key, cost)` spends several units at once (a bulk request). A cost
  of zero reports `remaining` without spending or storing anything.
- **A refused call spends nothing.** Callers that retry too fast never push
  their own wait further out, and `retry_after` is exact: the same call passes
  at that moment unless something else spends the key first.
- **Check the requester's key before the target's.** When a request is limited
  per address *and* per account, check the address first. A request refused
  for its address then never spends the account's quota, so one address can't
  lock someone else out of their account.
- **Keys are opaque**, but they sit in memory (or a shared store). Hash
  personal data such as email addresses before using it as a key.

## Quotas

`Quota::per_second/per_minute/per_hour/per_day(count)` or
`Quota::new(count, period)`, then optionally `.with_burst(burst)`.

- The sustained rate is `count / period`, enforced continuously: one unit
  comes back every `period / count` (rounded up a nanosecond at most, so the
  rate is never exceeded).
- `burst` (default `count`) is how much an idle key may spend at once.
- The exact guarantee: over any span `w`, a key gets at most
  `burst + w / interval`. With the default burst an idle key can spend a full
  period's worth at once and then keep pace with the rate, so up to
  `2 × count` can land inside one `period`. Lower the burst when that matters.
- `new` and `with_burst` return `None` for a zero period or a burst window
  over ~146 years, so a bad quota fails at startup, not mid-request.

## InMemoryRateLimiter

GCRA: each key stores one timestamp, the moment its quota is fully back.

- **Exact under concurrency.** Keys are spread over shards by a per-limiter
  random hash. A tracked key is spent with one atomic compare-and-swap under a
  shared lock, reading the clock between load and swap, so writes to a key
  commit in clock order and none is decided against a stale time. Callers
  only wait on each other to add or sweep keys.
- **Fast.** About 50 ns a check on one thread (half of it reading the
  monotonic clock), with no allocation.
- **Memory is bounded twice.** A key is forgotten as soon as its quota is fully
  back (it is then indistinguishable from a new key), swept in amortised
  constant time, so the table holds about twice the active keys (plus a fixed
  floor of about a thousand per shard, below which sweeping isn't worth it). And
  `with_max_keys(n)` (default `DEFAULT_MAX_KEYS`, 100,000, about 10 MB) is a
  hard cap: past it, new keys get `Error::Full` while tracked keys keep
  working. It never evicts a live key, since that would hand an attacker a
  fresh quota.
- **One process.** Each replica enforces the full quota on its own, and a
  restart forgets everything (which only lets people retry sooner).
- The clock is monotonic, so a wall-clock change neither resets nor freezes
  quotas. Tests pass a `ManualClock` via `with_clock` and advance it by hand.
- `prune()` forgets idle keys immediately; `len()` counts stored keys. Neither
  is needed for correctness.

## MockRateLimiter

```rust
let limiter = MockRateLimiter::new()
    .with_check_n_response("ip:203.0.113.7", 1, Ok(Decision::Never));
```

- Seed `check_n` (a `check` is `check_n` with cost 1). Each call takes the next
  response seeded for exactly its arguments; an unseeded call panics.
- `recorded_calls()` lists every call in order.
- Dropping the mock with a seeded response unused fails the test.
  `assert_all_responses_consumed()` checks at a chosen point, or when the mock
  is held somewhere that outlives the test.
