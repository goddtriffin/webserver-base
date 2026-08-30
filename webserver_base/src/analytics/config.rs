//! What the proxies need to know about their upstreams.

use crate::env::{self, EnvError};

/// The environment variable holding the Plausible script id.
pub const ENV_ANALYTICS_ID: &str = "WSB_ANALYTICS_ID";

/// The environment variable holding the browser Sentry DSN.
pub const ENV_SENTRY_BROWSER_DSN: &str = "WSB_SENTRY_BROWSER_DSN";

/// Plausible's script host.
pub const PLAUSIBLE_HOST: &str = "https://plausible.io";

/// Sentry's loader-script CDN.
pub const SENTRY_CDN_HOST: &str = "https://js.sentry-cdn.com";

/// Which Plausible site events belong to.
///
/// Modern Plausible identifies a site by the id embedded in its script URL
/// rather than a `data-domain` attribute, so this is the whole configuration.
/// Deriving it from the request's hostname instead would split `www.` and
/// apex traffic into two sites Plausible does not both know about, and would
/// let a staging deploy pour into production's numbers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsConfig {
    script_id: String,
}

impl AnalyticsConfig {
    /// Events for the site this script id belongs to.
    #[must_use]
    pub fn new(script_id: impl Into<String>) -> Self {
        Self {
            script_id: script_id.into(),
        }
    }

    /// Reads [`ENV_ANALYTICS_ID`].
    ///
    /// # Errors
    ///
    /// [`EnvError`] if it is unset or blank.
    pub fn from_env() -> Result<Self, EnvError> {
        Ok(Self::new(env::required(ENV_ANALYTICS_ID)?))
    }

    /// The configured script id.
    #[must_use]
    pub fn script_id(&self) -> &str {
        &self.script_id
    }

    /// The upstream script URL this proxy fetches.
    #[must_use]
    pub fn upstream_script_url(&self) -> String {
        format!("{PLAUSIBLE_HOST}/js/{}.js", self.script_id)
    }

    /// The upstream event endpoint this proxy forwards to.
    #[must_use]
    pub fn upstream_event_url() -> String {
        format!("{PLAUSIBLE_HOST}/api/event")
    }
}

/// A parsed Sentry DSN.
///
/// Only the pieces the tunnel needs: which host to relay to, and which project.
/// Because a frontend declares exactly one browser DSN, the tunnel never has to
/// parse an envelope to discover where an event belongs — which keeps it a dumb
/// byte relay and stops it being usable as an open relay to arbitrary projects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentryDsn {
    public_key: String,
    host: String,
    project_id: String,
}

/// Why a DSN could not be understood.
#[derive(Debug, thiserror::Error)]
#[error("`{dsn}` is not a Sentry DSN; expected `https://<key>@<host>/<project id>`")]
pub struct SentryDsnParseError {
    dsn: String,
}

impl SentryDsn {
    /// Parses `https://<public key>@<host>/<project id>`.
    ///
    /// # Errors
    ///
    /// [`SentryDsnParseError`] if the DSN is not in that shape.
    pub fn parse(dsn: &str) -> Result<Self, SentryDsnParseError> {
        let malformed = || SentryDsnParseError {
            dsn: dsn.to_string(),
        };

        let scheme_end: usize = dsn.find("://").ok_or_else(malformed)? + 3;
        let rest: &str = &dsn[scheme_end..];
        let (public_key, remainder) = rest.split_once('@').ok_or_else(malformed)?;
        let (host, project_id) = remainder.rsplit_once('/').ok_or_else(malformed)?;

        if public_key.is_empty() || host.is_empty() || project_id.is_empty() {
            return Err(malformed());
        }

        Ok(Self {
            public_key: public_key.to_string(),
            host: host.to_string(),
            project_id: project_id.to_string(),
        })
    }

    /// The upstream loader script for this DSN's public key.
    #[must_use]
    pub fn upstream_script_url(&self) -> String {
        format!("{SENTRY_CDN_HOST}/{}.min.js", self.public_key)
    }

    /// Where envelopes are relayed, verbatim.
    #[must_use]
    pub fn upstream_envelope_url(&self) -> String {
        format!("https://{}/api/{}/envelope/", self.host, self.project_id)
    }
}

#[cfg(test)]
mod tests {
    use super::{AnalyticsConfig, SentryDsn};

    #[test]
    fn the_plausible_script_url_embeds_the_site_id() {
        let config: AnalyticsConfig = AnalyticsConfig::new("pa-1qi0TQEvpewNxVHboeeOC");

        let expected: String = String::from("https://plausible.io/js/pa-1qi0TQEvpewNxVHboeeOC.js");
        let actual: String = config.upstream_script_url();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_dsn_yields_its_loader_script_and_envelope_endpoint() {
        let dsn: SentryDsn =
            SentryDsn::parse("https://abc123@o4504844394627072.ingest.sentry.io/4504862450515968")
                .expect("a well-formed DSN");

        let expected: String = String::from("https://js.sentry-cdn.com/abc123.min.js");
        let actual: String = dsn.upstream_script_url();
        assert_eq!(expected, actual);

        let expected: String = String::from(
            "https://o4504844394627072.ingest.sentry.io/api/4504862450515968/envelope/",
        );
        let actual: String = dsn.upstream_envelope_url();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_malformed_dsn_is_rejected_at_boot_rather_than_at_the_first_error() {
        assert!(SentryDsn::parse("not-a-dsn").is_err());
        assert!(SentryDsn::parse("https://o123.ingest.sentry.io/456").is_err());
        assert!(SentryDsn::parse("https://key@host/").is_err());
        assert!(SentryDsn::parse("https://@host/456").is_err());
    }
}
