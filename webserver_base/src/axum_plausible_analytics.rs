use axum::http::{HeaderMap, StatusCode};
use plausible_rs::{EventHeaders, EventPayload, PAGEVIEW_EVENT, Plausible};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{error, info, instrument, warn};

use crate::base_settings::{BaseSettings, Environment};

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
    fn resolve_true_client_ip_address(socket_addr: SocketAddr, header_map: HeaderMap) -> String {
        // prioritized list of HTTP headers that may contain the client's true IP address
        // (top-most entry is the most trusted)
        let prioritized_headers: Vec<&str> = vec![
            // Cloudflare
            "True-Client-IP",
            "CF-Connecting-IP",
            // standard
            "X-Forwarded-For",
            "X-Real-IP",
            "Forwarded",
            // backup (Axum's 'SocketAddr' IP; definitely a proxy)
            "socket_addr",
        ];

        // get the value of each prioritized header (if it exists)
        let prioritized_headers_values: HashMap<&str, Option<String>> = prioritized_headers.iter().map(|prioritized_header| {
            if *prioritized_header == "socket_addr" {
                // this is a backup in the case we fail to get 'true' IP address
                // (usually a Proxy IP)
                return (*prioritized_header, Some(socket_addr.ip().to_string()));
            }

            let header_value: Option<String> = match header_map.get(*prioritized_header) {
                // prioritized header exists in the HeaderMap
                Some(header_value) => {
                    match header_value.to_str() {
                        // successfully parsed HTTP header value
                        Ok(header_value) => {
                            if *prioritized_header == "X-Forwarded-For" {
                                info!("full HTTP 'X-Forwarded-For' header IP list: {header_value}");

                                // 'X-Forwarded-For' header may contain multiple IP addresses
                                let parts: Vec<&str> = header_value.split(',').collect();

                                // if there are multiple entries in this list, the left-most entry is the
                                // client's actual IP address (all other entries are Network Proxies)
                                let x_forwarded_for_client_ip: Option<&&str> = parts.first();
                                if let Some(x_forwarded_for_client_ip) = x_forwarded_for_client_ip {
                                    let x_forwarded_for: String = (*x_forwarded_for_client_ip).to_string();
                                    Some(x_forwarded_for)
                                } else {
                                    // zero IP addresses found in 'X-Forwarded-For' header
                                    None
                                }
                            } else {
                                // all other headers are single IP addresses
                                Some(header_value.to_string())
                            }
                        }
                        // failed to parse HTTP header value
                        Err(to_str_error) => {
                            error!("failed to parse HTTP '{prioritized_header}' header value: {to_str_error}");
                            None
                        }
                    }
                }
                // prioritized header does not exist in the HeaderMap
                None => {
                    None
                },
            };
            (*prioritized_header, header_value)
        }).collect();
        info!(
            "Client IP headers: {:?}",
            prioritized_headers_values.clone()
        );

        // choose the first prioritized header that exists
        for prioritized_header in prioritized_headers {
            if let Some(Some(header_value)) = prioritized_headers_values.get(prioritized_header) {
                info!(
                    "chose HTTP '{prioritized_header}' header for true client IP: '{header_value}'"
                );
                return header_value.to_string();
            }
        }

        // THIS SHOULD NEVER BE HIT because 'socket_addr' always exists
        error!("failed to find any prioritized HTTP headers for true client IP address");
        prioritized_headers_values
            .get("socket_addr")
            .unwrap()
            .as_ref()
            .unwrap()
            .to_string()
    }
}
