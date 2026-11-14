use std::fmt;
use std::time::Duration;

/// The answer to one check.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use]
pub enum Decision {
    /// The cost was spent. `remaining` is how much more the key could spend
    /// right now.
    Allowed { remaining: u32 },
    /// Nothing was spent. The same call succeeds once `retry_after` has
    /// passed, provided nothing else spends that key's quota first.
    Limited { retry_after: Duration },
    /// The cost exceeds the quota's burst, so this call could never succeed.
    /// Nothing was spent.
    Never,
}

impl Decision {
    #[must_use]
    pub const fn is_allowed(&self) -> bool {
        matches!(self, Self::Allowed { .. })
    }
}

/// A check that could not be decided. Whether to let the request through is
/// the caller's call: sign-ins should usually refuse, a cheap read may not.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// The limiter is tracking as many keys as `max_keys` allows and none has
    /// gone idle yet, so a new key cannot be tracked. Keys it already tracks
    /// are still decided normally.
    Full,
    /// The store behind the limiter failed (a network limiter's connection,
    /// for example).
    Backend(Box<dyn std::error::Error + Send + Sync>),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Full => f.write_str(
                "rate limiter is full: it is tracking max_keys active keys, so this new key was not \
                 checked; treat the request as limited, or raise max_keys if this is real traffic",
            ),
            Self::Backend(error) => write!(f, "rate limiter backend failed: {error}"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Full => None,
            Self::Backend(error) => Some(error.as_ref()),
        }
    }
}
