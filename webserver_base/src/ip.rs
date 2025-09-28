use std::{collections::HashMap, net::SocketAddr};

use axum::http::HeaderMap;
use tracing::{error, info, instrument};

/// Determine the client's actual IP address (not the IP address of any Proxies).
///
/// # Panics
///
/// Panics if the client's actual IP address cannot be determined.
#[instrument(skip_all)]
pub fn resolve_true_client_ip_address(socket_addr: SocketAddr, header_map: &HeaderMap) -> String {
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
            info!("chose HTTP '{prioritized_header}' header for true client IP: '{header_value}'");
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
