//! Sentry and tracing, initialised in the one order that works.
//!
//! `sentry::init` binds its hub to the calling thread; threads spawned
//! afterwards inherit it. `#[tokio::main]` spawns its workers *before* running
//! the body of `main`, so initialising there never reaches them and every error
//! raised on a worker is dropped. Hence: initialise on the main thread, then
//! build the runtime — which is what [`bootstrap`](crate::bootstrap) does.
//!
//! [`ObservabilityGuard`] flushes on drop, so it must outlive the server.

use std::fmt::{self, Debug, Formatter};

use tracing::{info, instrument, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};

use crate::env::{self, EnvError};
use crate::environment::Environment;

/// The environment variable holding the Sentry DSN.
pub const ENV_SENTRY_DSN: &str = "WSB_SENTRY_SERVER_DSN";

/// The log filter fallback when `RUST_LOG` is unset.
pub const DEFAULT_LOG_FILTER: &str = "info";

/// Why observability could not be configured.
#[derive(Debug, thiserror::Error)]
pub enum ObservabilityError {
    /// Production without a DSN. Deliberately fatal: a production server with
    /// monitoring off looks healthy while reporting nothing.
    #[error(
        "environment variable `{ENV_SENTRY_DSN}` is required in production; \
         error monitoring cannot be disabled there"
    )]
    MissingDsnInProduction,

    /// An environment variable could not be read.
    #[error(transparent)]
    Env(#[from] EnvError),
}

/// How this process reports errors and logs.
#[derive(Clone)]
pub struct Observability {
    dsn: Option<String>,
    environment: Environment,
    log_filter: Option<String>,
    release: String,
}

/// The release string used when the application does not name itself.
///
/// Sentry's own `release_name!` resolves `CARGO_PKG_NAME` where it is *written*
/// — inside this crate — so every project that used it would report the same
/// release and Sentry could not tell one site's deploys from another's. The
/// application must supply its own; this is only the fallback.
pub const UNKNOWN_RELEASE: &str = "unknown";

impl Observability {
    /// Error monitoring pointed at `dsn`.
    #[must_use]
    pub fn new(environment: Environment, dsn: impl Into<String>) -> Self {
        Self {
            dsn: Some(dsn.into()),
            environment,
            log_filter: None,
            release: String::from(UNKNOWN_RELEASE),
        }
    }

    /// Tracing only, with no error monitoring. For local runs and tests.
    #[must_use]
    pub fn none(environment: Environment) -> Self {
        Self {
            dsn: None,
            environment,
            log_filter: None,
            release: String::from(UNKNOWN_RELEASE),
        }
    }

    /// Reads [`ENV_SENTRY_DSN`], which every environment must set.
    ///
    /// Required locally too, deliberately: a DSN exercised only in production
    /// is a DSN nobody has proved works. Point local runs at a development
    /// Sentry project.
    ///
    /// # Errors
    ///
    /// [`ObservabilityError::Env`] if it is unset or blank.
    pub fn from_env(environment: Environment) -> Result<Self, ObservabilityError> {
        // Required in every environment, not just production. A DSN that is
        // only exercised in production is a DSN nobody has proved works.
        let dsn: String = env::required(ENV_SENTRY_DSN)?;

        Ok(Self {
            dsn: Some(dsn),
            environment,
            log_filter: None,
            release: String::from(UNKNOWN_RELEASE),
        })
    }

    /// Names the running build, as `my-project@1.2.3`.
    ///
    /// This is what Sentry attributes issues to, so it must identify the
    /// *application*, not this library. Deriving it here is impossible: the
    /// crate metadata available inside this crate is this crate's own.
    #[must_use]
    pub fn with_release(mut self, release: impl Into<String>) -> Self {
        self.release = release.into();
        self
    }

    /// Overrides the `RUST_LOG` fallback. `RUST_LOG` itself stays unprefixed —
    /// it is an ecosystem convention.
    #[must_use]
    pub fn with_log_filter(mut self, log_filter: impl Into<String>) -> Self {
        self.log_filter = Some(log_filter.into());
        self
    }

    /// Whether error monitoring is configured.
    #[must_use]
    pub const fn has_error_monitoring(&self) -> bool {
        self.dsn.is_some()
    }

    /// Which deployment this is.
    #[must_use]
    pub const fn environment(&self) -> Environment {
        self.environment
    }

    /// Installs the tracing subscriber and, when a DSN is set, Sentry.
    ///
    /// Main thread, before any runtime. Hold the guard for the process.
    #[must_use]
    #[instrument(skip_all)]
    pub fn init(self) -> ObservabilityGuard {
        let sentry_guard: Option<sentry::ClientInitGuard> = self.dsn.as_ref().map(|dsn| {
            // `ClientOptions` is `#[non_exhaustive]` as of sentry 0.49, so it
            // has to be built by mutation rather than a struct expression.
            let mut options: sentry::ClientOptions = sentry::ClientOptions::default();
            // Not `sentry::release_name!()`: that macro reads the crate
            // metadata of wherever it is expanded, which here is this library —
            // so every project would report an identical release and Sentry
            // could not attribute an issue to the deploy that caused it.
            options.release = Some(self.release.clone().into());
            options.environment = Some(self.environment.as_str().into());
            options.attach_stacktrace = true;

            sentry::init((dsn.clone(), options))
        });

        let fallback: &str = self.log_filter.as_deref().unwrap_or(DEFAULT_LOG_FILTER);
        let directives: String =
            std::env::var(EnvFilter::DEFAULT_ENV).unwrap_or_else(|_| fallback.to_string());
        let filter: EnvFilter =
            EnvFilter::try_new(&directives).unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

        // A human reads the local log; a machine reads the production one.
        // `with_ansi(false)` in production because a log driver stores escape
        // codes verbatim and nothing downstream strips them.
        let format = if self.environment.is_production() {
            tracing_subscriber::fmt::layer()
                .json()
                .flatten_event(true)
                .with_current_span(true)
                .with_span_list(false)
                .with_ansi(false)
                .with_filter(filter)
                .boxed()
        } else {
            tracing_subscriber::fmt::layer().with_filter(filter).boxed()
        };

        tracing_subscriber::registry()
            .with(format)
            // Harmless without Sentry: the layer forwards to a disabled hub.
            .with(sentry::integrations::tracing::layer())
            .init();

        // The one thing no other log line can tell you: which build is running.
        // Everything else here is either inferable or already stated elsewhere,
        // so this stays to three fields.
        info!(
            release = %self.release,
            environment = %self.environment,
            log_filter = %directives,
            "starting"
        );

        if sentry_guard.is_none() {
            warn!("error monitoring is not configured; nothing will reach Sentry");
        }

        ObservabilityGuard {
            sentry: sentry_guard,
        }
    }
}

impl Debug for Observability {
    /// Never prints the DSN — it is a credential, and this type reaches logs.
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Observability")
            .field("environment", &self.environment)
            .field("release", &self.release)
            // The DSN itself is deliberately absent: it is a credential, and
            // this type reaches logs.
            .field("error_monitoring", &self.dsn.is_some())
            .field("log_filter", &self.log_filter)
            .finish()
    }
}

/// Keeps error monitoring alive, and flushes it on drop. Bind it to `_guard`,
/// never `_`, which would drop it immediately.
pub struct ObservabilityGuard {
    sentry: Option<sentry::ClientInitGuard>,
}

impl Debug for ObservabilityGuard {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("ObservabilityGuard")
            .field("error_monitoring", &self.sentry.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::Observability;
    use crate::environment::Environment;

    #[test]
    fn debug_output_never_contains_the_dsn() {
        let observability: Observability =
            Observability::new(Environment::Production, "https://sup3rs3cret@example.com/1");

        let actual: String = format!("{observability:?}");
        assert!(!actual.contains("sup3rs3cret"));
        assert!(actual.contains("error_monitoring: true"));
    }

    #[test]
    fn a_custom_log_filter_is_retained() {
        let observability: Observability =
            Observability::none(Environment::Local).with_log_filter("debug");

        let actual: String = format!("{observability:?}");
        assert!(actual.contains("debug"));
    }
}
