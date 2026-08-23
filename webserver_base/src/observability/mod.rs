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
pub const ENV_SENTRY_DSN: &str = "WSB_SENTRY_DSN";

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
}

impl Observability {
    /// Error monitoring pointed at `dsn`.
    #[must_use]
    pub fn new(environment: Environment, dsn: impl Into<String>) -> Self {
        Self {
            dsn: Some(dsn.into()),
            environment,
            log_filter: None,
        }
    }

    /// Tracing only, with no error monitoring. For local runs and tests.
    #[must_use]
    pub const fn none(environment: Environment) -> Self {
        Self {
            dsn: None,
            environment,
            log_filter: None,
        }
    }

    /// Reads [`ENV_SENTRY_DSN`] — required in production, optional locally.
    ///
    /// # Errors
    ///
    /// [`ObservabilityError::MissingDsnInProduction`] when production has no
    /// DSN, or [`ObservabilityError::Env`] if the value is unreadable.
    pub fn from_env(environment: Environment) -> Result<Self, ObservabilityError> {
        let dsn: Option<String> = env::optional(ENV_SENTRY_DSN);

        if environment.is_production() && dsn.is_none() {
            return Err(ObservabilityError::MissingDsnInProduction);
        }

        Ok(Self {
            dsn,
            environment,
            log_filter: None,
        })
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
            options.release = sentry::release_name!();
            options.environment = Some(self.environment.as_str().into());
            options.attach_stacktrace = true;

            sentry::init((dsn.clone(), options))
        });

        let fallback: &str = self.log_filter.as_deref().unwrap_or(DEFAULT_LOG_FILTER);
        let filter: EnvFilter = EnvFilter::try_from_default_env()
            .or_else(|_| EnvFilter::try_new(fallback))
            .unwrap_or_else(|_| EnvFilter::new(DEFAULT_LOG_FILTER));

        tracing_subscriber::registry()
            .with(tracing_subscriber::fmt::layer().with_filter(filter))
            // Harmless without Sentry: the layer forwards to a disabled hub.
            .with(sentry::integrations::tracing::layer())
            .init();

        if sentry_guard.is_some() {
            info!(
                "error monitoring enabled for environment `{}`",
                self.environment
            );
        } else {
            warn!(
                "error monitoring disabled: `{ENV_SENTRY_DSN}` is not set (environment `{}`)",
                self.environment
            );
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
    use super::{Observability, ObservabilityError};
    use crate::environment::Environment;

    #[test]
    fn none_is_explicitly_unmonitored() {
        let observability: Observability = Observability::none(Environment::Local);

        let expected: bool = false;
        let actual: bool = observability.has_error_monitoring();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_dsn_can_be_supplied_directly_without_the_environment() {
        let observability: Observability =
            Observability::new(Environment::Production, "https://key@example.com/1");

        let expected: bool = true;
        let actual: bool = observability.has_error_monitoring();
        assert_eq!(expected, actual);

        let expected_environment: Environment = Environment::Production;
        let actual_environment: Environment = observability.environment();
        assert_eq!(expected_environment, actual_environment);
    }

    #[test]
    fn production_without_a_dsn_refuses_to_configure() {
        // WSB_SENTRY_DSN is not set in the test process, and setting env vars
        // is `unsafe`, which this crate forbids.
        let error: ObservabilityError = Observability::from_env(Environment::Production)
            .expect_err("production requires a DSN");
        assert!(matches!(error, ObservabilityError::MissingDsnInProduction));

        let expected: String = String::from(
            "environment variable `WSB_SENTRY_DSN` is required in production; \
             error monitoring cannot be disabled there",
        );
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn local_without_a_dsn_is_fine() {
        let observability: Observability =
            Observability::from_env(Environment::Local).expect("local does not require a DSN");

        let expected: bool = false;
        let actual: bool = observability.has_error_monitoring();
        assert_eq!(expected, actual);
    }

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
