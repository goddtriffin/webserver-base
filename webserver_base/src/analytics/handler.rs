//! Forwarding a browser pageview to Plausible.

use std::net::SocketAddr;

use axum::http::{HeaderMap, StatusCode};
use plausible_rs::{EventHeaders, EventPayload, PAGEVIEW_EVENT, Plausible};
use serde::{Deserialize, Serialize};
use tracing::{debug, error, instrument};

use crate::env::{self, EnvError};
use crate::environment::Environment;

use super::ip::resolve_true_client_ip_address;

/// The environment variable holding the Plausible site domain.
pub const ENV_ANALYTICS_DOMAIN: &str = "WSB_ANALYTICS_DOMAIN";

/// Which Plausible property events belong to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnalyticsConfig {
    domain: String,
}

impl AnalyticsConfig {
    /// Events for `domain`, as registered in Plausible.
    #[must_use]
    pub fn new(domain: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
        }
    }

    /// Reads [`ENV_ANALYTICS_DOMAIN`].
    ///
    /// # Errors
    ///
    /// [`EnvError`] if it is unset or blank.
    pub fn from_env() -> Result<Self, EnvError> {
        Ok(Self::new(env::required(ENV_ANALYTICS_DOMAIN)?))
    }

    /// The configured domain.
    #[must_use]
    pub fn domain(&self) -> &str {
        &self.domain
    }
}

/// What the browser posts to the analytics endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsRequest {
    /// The browser's user agent, forwarded so Plausible can classify it.
    pub user_agent: String,
    /// The page that was viewed.
    pub url: String,
    /// Where the visitor came from.
    pub referrer: String,
    /// Viewport width, for Plausible's device breakdown.
    pub screen_width: usize,
}

/// Forwards pageviews to Plausible.
#[derive(Debug)]
pub struct AnalyticsHandler {
    client: Plausible,
    config: AnalyticsConfig,
    environment: Environment,
}

impl AnalyticsHandler {
    /// A handler for `config`.
    ///
    /// Plausible builds its own HTTP client rather than sharing this crate's:
    /// `plausible-rs` pins `reqwest` 0.12 while this crate is on 0.13, so a
    /// `Client` cannot cross that boundary.
    #[must_use]
    pub fn new(config: AnalyticsConfig, environment: Environment) -> Self {
        Self {
            client: Plausible::new(),
            config,
            environment,
        }
    }

    /// The configured domain.
    #[must_use]
    pub fn domain(&self) -> &str {
        self.config.domain()
    }

    /// Forwards one pageview, or drops it when running locally. A dropped local
    /// event answers `200` — dropping it was the intent.
    #[instrument(skip_all)]
    pub async fn handle(
        &self,
        headers: &HeaderMap,
        socket_addr: SocketAddr,
        request: AnalyticsRequest,
    ) -> StatusCode {
        if self.environment.is_local() {
            debug!(
                "dropping analytics event for `{}`: not running in production",
                request.url
            );
            return StatusCode::OK;
        }

        let payload: EventPayload = EventPayload::builder(
            self.config.domain.clone(),
            PAGEVIEW_EVENT.to_string(),
            request.url.clone(),
        )
        .referrer(request.referrer.clone())
        .screen_width(request.screen_width)
        .build();

        let client_ip: String = resolve_true_client_ip_address(socket_addr, headers);
        let event_headers: EventHeaders = EventHeaders::new(request.user_agent.clone(), client_ip);

        match self.client.event(event_headers, payload).await {
            Ok(_) => {
                debug!("forwarded analytics pageview for `{}`", request.url);
                StatusCode::OK
            }
            Err(error) => {
                error!("failed to forward analytics pageview: {error}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::http::{HeaderMap, StatusCode};

    use super::{AnalyticsConfig, AnalyticsHandler, AnalyticsRequest};
    use crate::environment::Environment;

    fn request() -> AnalyticsRequest {
        AnalyticsRequest {
            user_agent: String::from("test-agent"),
            url: String::from("https://www.example.com/"),
            referrer: String::new(),
            screen_width: 1920,
        }
    }

    #[test]
    fn the_domain_round_trips() {
        let config: AnalyticsConfig = AnalyticsConfig::new("example.com");

        let expected: String = String::from("example.com");
        let actual: String = config.domain().to_string();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn a_local_event_is_dropped_without_any_network_call() {
        let handler: AnalyticsHandler =
            AnalyticsHandler::new(AnalyticsConfig::new("example.com"), Environment::Local);
        let socket: SocketAddr = "127.0.0.1:1234".parse().expect("valid socket address");

        let expected: StatusCode = StatusCode::OK;
        let actual: StatusCode = handler.handle(&HeaderMap::new(), socket, request()).await;
        assert_eq!(expected, actual);
    }
}
