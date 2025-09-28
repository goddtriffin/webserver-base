use axum::http::{HeaderMap, StatusCode};
use plausible_rs::{EventHeaders, EventPayload, PAGEVIEW_EVENT, Plausible};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

use crate::{
    base_settings::{BaseSettings, Environment},
    ip::resolve_true_client_ip_address,
};

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestPayload {
    pub user_agent: String,
    pub url: String,
    pub referrer: String,
    pub screen_width: usize,
}

pub struct AxumPlausibleAnalyticsHandler {
    plausible_client: Plausible,
}

impl AxumPlausibleAnalyticsHandler {
    #[must_use]
    #[instrument(skip_all)]
    pub fn new_with_client(http_client: Client) -> Self {
        Self {
            plausible_client: Plausible::new_with_client(http_client),
        }
    }

    #[instrument(skip_all)]
    pub async fn handle(
        self: Arc<Self>,
        headers: HeaderMap,
        settings: BaseSettings,
        addr: SocketAddr,
        incoming_payload: RequestPayload,
    ) -> StatusCode {
        // generate payload
        let domain: String = if settings.environment == Environment::Production {
            settings.analytics_domain.clone()
        } else {
            String::from("test.toddgriffin.me")
        };
        let outgoing_payload: EventPayload = EventPayload::builder(
            domain,
            PAGEVIEW_EVENT.to_string(),
            incoming_payload.url.clone(),
        )
        .referrer(incoming_payload.referrer.clone())
        .screen_width(incoming_payload.screen_width)
        .build();

        // generate headers
        let real_client_ip: String = resolve_true_client_ip_address(addr, &headers);
        let headers: EventHeaders =
            EventHeaders::new(incoming_payload.user_agent.clone(), real_client_ip);

        info!(
            "Making Plausible Analytics calls with headers={:?} and body={:?}",
            headers.clone(),
            outgoing_payload.clone()
        );
        // post 'pageview' event
        match self.plausible_client.event(headers, outgoing_payload).await {
            Ok(bytes) => {
                info!(
                    "Plausible Analytics call was a success: {}",
                    String::from_utf8_lossy(&bytes)
                );
                StatusCode::OK
            }
            Err(e) => {
                error!("Failed Plausible Analytics call: {}", e);
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }
}
