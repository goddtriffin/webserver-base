use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::{BuildHasher, BuildHasherDefault, Hash, Hasher, RandomState};
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use crate::quota::MAX_TOLERANCE;
use crate::rate_limiter::Decide;
use crate::{Check, Clock, Decision, Error, MonotonicClock, Quota, RateLimiter};

/// Keys tracked at most, unless [`InMemoryRateLimiter::with_max_keys`] says
/// otherwise. Roughly 100 bytes each with a short key, so about 10 MB.
pub const DEFAULT_MAX_KEYS: NonZeroUsize = NonZeroUsize::new(100_000).unwrap();

/// Below this many keys a shard never sweeps: scanning a small table costs
/// more than the memory it would free.
const MIN_SWEEP: usize = 1_024;

/// A shard only exists if it can hold this many keys, so the per-shard split
/// of `max_keys` stays fair and small limits stay exact (one shard).
const MIN_KEYS_PER_SHARD: usize = 1_024;

const MAX_SHARDS: usize = 256;

/// The clock is clamped here, so `now + tolerance + cost` can never overflow
/// (every term is at most [`MAX_TOLERANCE`], a quarter of `u64::MAX`). That is
/// ~292 years of uptime; past it time stops, which only ever limits more.
const MAX_NOW: u64 = u64::MAX - 2 * MAX_TOLERANCE;

/// An exact rate limiter kept in this process's memory (GCRA).
///
/// - **Exact.** Each key stores one timestamp: when its quota will be fully
///   back (its theoretical arrival time). There is no window edge to game and
///   no partial refill to lose, so a key gets precisely what its [`Quota`]
///   allows, even under concurrent calls.
/// - **Bounded.** A key is forgotten once its quota is fully back, since it
///   is then indistinguishable from a key never seen. Idle keys are swept in
///   amortised constant time, and at most `max_keys` are ever stored; past
///   that, new keys get [`Error::Full`] instead of evicting someone else's
///   state (eviction would hand an attacker a fresh quota).
/// - **Concurrent.** Keys are spread over independently locked shards by a
///   per-limiter random hash, so nobody can aim keys at one shard. A key
///   already tracked is decided under a shared lock with one atomic
///   compare-and-swap, so callers only wait on each other to add or sweep
///   keys.
///
/// State lives in one process. Several replicas each enforce the full quota,
/// and a restart forgets everything, which errs toward letting people retry
/// sooner.
pub struct InMemoryRateLimiter<C: Clock = MonotonicClock> {
    quota: Quota,
    clock: C,
    hasher: RandomState,
    shards: Box<[Padded]>,
    /// `shards.len() - 1`; the length is a power of two.
    mask: u64,
}

/// One shard per cache line pair, so a lock taken on one core doesn't
/// invalidate its neighbours' on every other core (false sharing).
#[repr(align(128))]
struct Padded(RwLock<Shard>);

struct Shard {
    /// Each key's theoretical arrival time, in nanoseconds of the clock.
    /// Atomic so tracked keys can be spent under the shared lock.
    tats: HashMap<Key, AtomicU64, BuildHasherDefault<Prehashed>>,
    /// This shard's share of `max_keys`.
    capacity: usize,
    /// Sweep when an insert finds this many keys. Never above `capacity`.
    sweep_at: usize,
    /// No stored timestamp is earlier than this, so no key can be swept before
    /// it. Lets a full shard refuse in O(1) instead of rescanning. Spending
    /// only moves timestamps later, so it stays a lower bound.
    earliest: u64,
}

impl InMemoryRateLimiter<MonotonicClock> {
    #[must_use]
    pub fn new(quota: Quota) -> Self {
        Self::build(quota, MonotonicClock::new(), DEFAULT_MAX_KEYS)
    }
}

impl<C: Clock> InMemoryRateLimiter<C> {
    /// Caps how many keys are stored at once, which caps memory. A builder
    /// step: any state tracked so far is dropped.
    #[must_use]
    pub fn with_max_keys(self, max_keys: NonZeroUsize) -> Self {
        Self::build(self.quota, self.clock, max_keys)
    }

    /// Reads time from `clock` instead. A builder step: any state tracked so
    /// far is dropped.
    #[must_use]
    pub fn with_clock<D: Clock>(self, clock: D) -> InMemoryRateLimiter<D> {
        let max_keys: usize = self.shards.iter().map(|s| read(s).capacity).sum();
        let max_keys: NonZeroUsize = NonZeroUsize::new(max_keys).unwrap_or(DEFAULT_MAX_KEYS);
        InMemoryRateLimiter::build(self.quota, clock, max_keys)
    }

    #[must_use]
    pub const fn quota(&self) -> Quota {
        self.quota
    }

    /// Keys currently stored, including idle ones not yet swept.
    #[must_use]
    pub fn len(&self) -> usize {
        self.shards.iter().map(|s| read(s).tats.len()).sum()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Forgets every idle key now, instead of waiting for inserts to sweep.
    /// Never needed for correctness or for the memory bound.
    pub fn prune(&self) {
        for shard in &self.shards {
            let mut shard: RwLockWriteGuard<'_, Shard> = write(shard);
            let now: u64 = self.now();
            shard.sweep(now);
        }
    }

    fn build(quota: Quota, clock: C, max_keys: NonZeroUsize) -> Self {
        let count: usize = shard_count(max_keys.get());
        let base: usize = max_keys.get() / count;
        let extra: usize = max_keys.get() % count;
        let shards: Box<[Padded]> = (0..count)
            .map(|i: usize| {
                let capacity: usize = base + usize::from(i < extra);
                Padded(RwLock::new(Shard {
                    tats: HashMap::default(),
                    capacity,
                    sweep_at: MIN_SWEEP.min(capacity),
                    earliest: u64::MAX,
                }))
            })
            .collect();
        Self {
            quota,
            clock,
            hasher: RandomState::new(),
            shards,
            mask: u64::try_from(count - 1).unwrap_or(0),
        }
    }

    fn now(&self) -> u64 {
        u64::try_from(self.clock.now().as_nanos())
            .unwrap_or(u64::MAX)
            .min(MAX_NOW)
    }

    /// Spends from a tracked key. Lock-free: the clock is read after the
    /// timestamp is loaded and the result is only stored if nobody stored in
    /// between, so writes to one key commit in clock order and each decision
    /// is exact for the moment it read.
    fn spend(&self, tat: &AtomicU64, cost: u32) -> Decision {
        let mut current: u64 = tat.load(Ordering::Acquire);
        loop {
            let now: u64 = self.now();
            let (decision, next) = self.evaluate(current.max(now), now, cost);
            let Some(next) = next else {
                return decision;
            };
            match tat.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => return decision,
                Err(actual) => current = actual,
            }
        }
    }

    /// The decision for a key whose quota is fully back at `tat` (never
    /// before `now`), and the timestamp to store when units are spent.
    fn evaluate(&self, tat: u64, now: u64, cost: u32) -> (Decision, Option<u64>) {
        let interval: u64 = self.quota.interval_nanos();
        let tolerance: u64 = self.quota.tolerance_nanos();
        // No overflow: tat <= now + tolerance, cost * interval <= tolerance,
        // and now <= MAX_NOW leaves room for both.
        let next: u64 = tat + u64::from(cost) * interval;
        let ahead: u64 = next - now;
        if ahead > tolerance {
            let retry_after: Duration = Duration::from_nanos(ahead - tolerance);
            return (Decision::Limited { retry_after }, None);
        }
        let remaining: u32 =
            u32::try_from((tolerance - ahead) / interval).unwrap_or(self.quota.burst());
        let store: Option<u64> = (cost > 0).then_some(next);
        (Decision::Allowed { remaining }, store)
    }
}

impl<C: Clock> Decide for InMemoryRateLimiter<C> {
    fn decide(&self, key: &str, cost: u32) -> Result<Decision, Error> {
        if cost > self.quota.burst() {
            return Ok(Decision::Never);
        }
        // One keyed hash picks the shard and places the key in its table.
        let hash: u64 = self.hasher.hash_one(key);
        let probe: &dyn Probe = &(hash, key);
        let index: usize = usize::try_from((hash >> SHARD_SHIFT) & self.mask).unwrap_or(0);
        let shard: &Padded = &self.shards[index];
        if let Some(tat) = read(shard).tats.get(probe) {
            return Ok(self.spend(tat, cost));
        }
        let mut shard: RwLockWriteGuard<'_, Shard> = write(shard);
        // Someone may have added the key between the two locks.
        if let Some(tat) = shard.tats.get(probe) {
            return Ok(self.spend(tat, cost));
        }
        // An absent key and a key whose quota is fully back are the same.
        let now: u64 = self.now();
        let (decision, next) = self.evaluate(now, now, cost);
        if let Some(next) = next {
            shard.insert(
                Key {
                    hash,
                    text: Box::from(key),
                },
                next,
                now,
            )?;
        }
        Ok(decision)
    }
}

impl<C: Clock> RateLimiter for InMemoryRateLimiter<C> {
    fn check_n<'a>(&'a self, key: &'a str, cost: u32) -> Check<'a> {
        Check::local(self, key, cost)
    }
}

impl Shard {
    fn insert(&mut self, key: Key, tat: u64, now: u64) -> Result<(), Error> {
        if self.tats.len() >= self.sweep_at {
            self.sweep(now);
            if self.tats.len() >= self.capacity {
                return Err(Error::Full);
            }
        }
        self.tats.insert(key, AtomicU64::new(tat));
        self.earliest = self.earliest.min(tat);
        Ok(())
    }

    /// Drops every key whose quota is fully back. Scans only when one could
    /// be, then waits for the table to double before scanning again, so
    /// sweeping costs amortised O(1) per insert and the table holds at most
    /// about twice the active keys.
    fn sweep(&mut self, now: u64) {
        if now >= self.earliest {
            self.tats
                .retain(|_, tat: &mut AtomicU64| *tat.get_mut() > now);
            self.earliest = self
                .tats
                .values_mut()
                .map(|tat: &mut AtomicU64| *tat.get_mut())
                .min()
                .unwrap_or(u64::MAX);
            let floor: usize = self.tats.len().max(MIN_SWEEP);
            if self.tats.capacity() > 4 * floor {
                self.tats.shrink_to(2 * floor);
            }
        }
        self.sweep_at = (2 * self.tats.len()).max(MIN_SWEEP).min(self.capacity);
    }
}

/// The shard comes from bits 40 to 47 of the key's hash. The table uses the
/// low bits for buckets and the top seven as tags, so keys sharing a shard
/// still spread over the whole table.
const SHARD_SHIFT: u32 = 40;

/// A stored key with its hash, so the hash that chose the shard is reused by
/// the table instead of computed again.
struct Key {
    hash: u64,
    text: Box<str>,
}

/// What a table lookup compares: a stored [`Key`], or a borrowed key and
/// its hash, so looking up never allocates.
trait Probe {
    fn digest(&self) -> u64;
    fn text(&self) -> &str;
}

impl Probe for Key {
    fn digest(&self) -> u64 {
        self.hash
    }

    fn text(&self) -> &str {
        &self.text
    }
}

impl Probe for (u64, &str) {
    fn digest(&self) -> u64 {
        self.0
    }

    fn text(&self) -> &str {
        self.1
    }
}

impl<'a> Borrow<dyn Probe + 'a> for Key {
    fn borrow(&self) -> &(dyn Probe + 'a) {
        self
    }
}

impl Hash for dyn Probe + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.digest());
    }
}

impl PartialEq for dyn Probe + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.digest() == other.digest() && self.text() == other.text()
    }
}

impl Eq for dyn Probe + '_ {}

impl Hash for Key {
    fn hash<H: Hasher>(&self, state: &mut H) {
        state.write_u64(self.hash);
    }
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        (self as &dyn Probe) == (other as &dyn Probe)
    }
}

impl Eq for Key {}

/// Passes through the hash a [`Key`] already carries. Only ever fed one
/// `u64`, by the `Hash` impls above.
#[derive(Default)]
struct Prehashed(u64);

impl Hasher for Prehashed {
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, bytes: &[u8]) {
        // Unreachable from this module; folded in rather than dropped so a
        // stray caller still gets a usable hash.
        for &byte in bytes {
            self.0 = self.0.rotate_left(8) ^ u64::from(byte);
        }
    }

    fn write_u64(&mut self, hash: u64) {
        self.0 = hash;
    }
}

/// The table is a plain map of integers and every write is a single store, so
/// a panic elsewhere can't leave it half-updated; recovering is always safe.
fn read(shard: &Padded) -> RwLockReadGuard<'_, Shard> {
    shard.0.read().unwrap_or_else(PoisonError::into_inner)
}

fn write(shard: &Padded) -> RwLockWriteGuard<'_, Shard> {
    shard.0.write().unwrap_or_else(PoisonError::into_inner)
}

/// A power of two: four per core for low contention, but never so many that a
/// shard's share of `max_keys` drops below [`MIN_KEYS_PER_SHARD`].
fn shard_count(max_keys: usize) -> usize {
    let cores: usize = std::thread::available_parallelism().map_or(1, NonZeroUsize::get);
    let wanted: usize = cores.saturating_mul(4).next_power_of_two().min(MAX_SHARDS);
    let fits: usize = (max_keys / MIN_KEYS_PER_SHARD).max(1);
    let fits: usize = 1 << fits.ilog2();
    wanted.min(fits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_limits_use_one_shard_so_max_keys_is_exact() {
        assert_eq!(1, shard_count(1));
        assert_eq!(1, shard_count(2_047));
    }

    #[test]
    fn shards_are_a_power_of_two_and_each_holds_enough_keys() {
        for max_keys in [2_048, 10_000, 100_000, 10_000_000] {
            let count: usize = shard_count(max_keys);
            assert!(count.is_power_of_two());
            assert!(count <= MAX_SHARDS);
            assert!(max_keys / count >= MIN_KEYS_PER_SHARD);
        }
    }

    #[test]
    fn shard_capacities_add_up_to_max_keys_exactly() {
        let limiter: InMemoryRateLimiter =
            InMemoryRateLimiter::new(Quota::per_second(std::num::NonZeroU32::new(1).unwrap()))
                .with_max_keys(NonZeroUsize::new(1_000_003).unwrap());
        let total: usize = limiter.shards.iter().map(|s| read(s).capacity).sum();
        assert_eq!(1_000_003, total);
    }
}
