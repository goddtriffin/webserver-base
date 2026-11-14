//! The accounts service's published signing keys, cached. An unknown `kid`
//! triggers one refetch (keys rotate), at most once a minute.

use std::sync::RwLock;
use std::time::{Duration, Instant};

use jsonwebtoken::DecodingKey;
use jsonwebtoken::jwk::JwkSet;

use crate::{Error, Result};

const REFRESH_EVERY: Duration = Duration::from_secs(3600);
const MIN_REFETCH: Duration = Duration::from_secs(60);

pub struct Keys {
    url: String,
    /// Pinned keys (tests, offline use) are never refetched.
    pinned: bool,
    cache: RwLock<Option<(Instant, JwkSet)>>,
}

impl Keys {
    pub fn remote(url: String) -> Self {
        Self {
            url,
            pinned: false,
            cache: RwLock::new(None),
        }
    }

    pub fn pinned(set: JwkSet) -> Self {
        Self {
            url: String::new(),
            pinned: true,
            cache: RwLock::new(Some((Instant::now(), set))),
        }
    }

    fn lookup(&self, kid: &str) -> Option<DecodingKey> {
        let cache = self.cache.read().ok()?;
        let (_, set) = cache.as_ref()?;
        set.find(kid)
            .and_then(|jwk| DecodingKey::from_jwk(jwk).ok())
    }

    fn age(&self) -> Option<Duration> {
        self.cache.read().ok()?.as_ref().map(|(at, _)| at.elapsed())
    }

    /// The key for `kid`, fetching the key set if it's missing or stale.
    pub async fn get(&self, http: &reqwest::Client, kid: &str) -> Result<DecodingKey> {
        let age: Option<Duration> = self.age();
        let stale: bool = age.is_none_or(|a| a > REFRESH_EVERY);
        if !stale && let Some(key) = self.lookup(kid) {
            return Ok(key);
        }
        if self.pinned {
            return self
                .lookup(kid)
                .ok_or_else(|| Error::InvalidToken(String::from("unknown signing key")));
        }
        // Unknown kid on a fresh cache: refetch, but not more than once a minute.
        if stale || age.is_some_and(|a| a > MIN_REFETCH) {
            let set: JwkSet = http
                .get(&self.url)
                .send()
                .await?
                .error_for_status()?
                .json()
                .await?;
            if let Ok(mut cache) = self.cache.write() {
                *cache = Some((Instant::now(), set));
            }
        }
        self.lookup(kid)
            .ok_or_else(|| Error::InvalidToken(String::from("unknown signing key")))
    }
}
