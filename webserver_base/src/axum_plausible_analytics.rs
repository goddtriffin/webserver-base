use crate::{BaseSettings, Environment};
use axum::http::{HeaderMap, StatusCode};
use plausible_rs::{EventHeaders, EventPayload, Plausible, PAGEVIEW_EVENT};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

#[derive(Debug, Serialize, Deserialize)]
pub struct RequestPayload {
    pub user_agent: String,
    pub url: String,
    pub referrer: String,
    pub screen_width: usize,
}

#[allow(clippy::module_name_repetitions)]
pub struct AxumPlausibleAnalyticsHandler {
    plausible_client: Plausible,
}

impl AxumPlausibleAnalyticsHandler {
    #[must_use]
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
        let real_client_ip: String = Self::resolve_true_client_ip_address(addr, headers);
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

    /// Determine the client's actual IP address (not the IP address of any Proxies).
    #[instrument(skip_all)]
    fn resolve_true_client_ip_address(addr: SocketAddr, headers: HeaderMap) -> String {
        if let Some(forwarded) = headers.get("Forwarded") {
            match forwarded.to_str() {
                Ok(forwarded) => {
                    warn!("TODO - handle the HTTP 'Forwarded' header: {forwarded}");
                }
                Err(to_str_error) => {
                    error!("failed to parse HTTP 'Forwarded' header value: {to_str_error}");
                }
            }
        }

        if let Some(x_real_ip) = headers.get("X-Real-IP") {
            match x_real_ip.to_str() {
                Ok(x_real_ip) => {
                    warn!("TODO - handle the HTTP 'X-Real-IP' header: {x_real_ip}");
                }
                Err(to_str_error) => {
                    error!("failed to parse HTTP 'X-Real-IP' header value: {to_str_error}");
                }
            }
        }

        if let Some(cf_connecting_ip) = headers.get("CF-Connecting-IP") {
            match cf_connecting_ip.to_str() {
                Ok(cf_connecting_ip) => {
                    warn!("TODO - handle the HTTP 'CF-Connecting-IP' header: {cf_connecting_ip}");
                }
                Err(to_str_error) => {
                    error!("failed to parse HTTP 'CF-Connecting-IP' header value: {to_str_error}");
                }
            }
        }

        if let Some(true_client_ip) = headers.get("True-Client-IP") {
            match true_client_ip.to_str() {
                Ok(true_client_ip) => {
                    warn!("TODO - handle the HTTP 'True-Client-IP' header: {true_client_ip}");
                }
                Err(to_str_error) => {
                    error!("failed to parse HTTP 'True-Client-IP' header value: {to_str_error}");
                }
            }
        }

        if let Some(x_forwarded_for) = headers.get("X-Forwarded-For") {
            match x_forwarded_for.to_str() {
                Ok(x_forwarded_for) => {
                    let parts: Vec<&str> = x_forwarded_for.split(',').collect();

                    // if there are multiple entries in this list, the left-most entry is the
                    // client's actual IP address (all other entries are Network Proxies)
                    let x_forwarded_for_client_ip: Option<&&str> = parts.first();
                    if let Some(x_forwarded_for_client_ip) = x_forwarded_for_client_ip {
                        let x_forwarded_for: String = (*x_forwarded_for_client_ip).to_string();
                        info!("found HTTP 'X-Forwarded-For' header: {x_forwarded_for}");
                        return x_forwarded_for;
                    };
                }
                Err(to_str_error) => {
                    error!("failed to parse HTTP 'X-Forwarded-For' header value: {to_str_error}");
                }
            }
        }

        // failed to get 'true' IP address - default to the SocketAddr (proxy IP)
        let axum_socketaddr_ip: String = addr.ip().to_string();
        warn!("Failed to resolve client's true IP address. Defaulting to Axum's 'SocketAddr' IP (which is usually the IP address of an intermediary Network Proxy): {axum_socketaddr_ip}");
        axum_socketaddr_ip
    }
}
