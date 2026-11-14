//! Working out who actually sent a request.

use std::net::SocketAddr;

use axum::http::HeaderMap;
use tracing::{debug, instrument, warn};

/// Headers consulted for the client's real address, most trusted first. The
/// socket address is the fallback: always present, but behind a proxy it is the
/// proxy.
pub const CLIENT_IP_HEADERS: [&str; 5] = [
    // Cloudflare
    "True-Client-IP",
    "CF-Connecting-IP",
    // conventional
    "X-Forwarded-For",
    "X-Real-IP",
    "Forwarded",
];

/// Resolves the client's address, preferring proxy headers over the socket.
///
/// In an `X-Forwarded-For` chain the left-most entry is the originating
/// client.
#[must_use]
#[instrument(skip_all)]
pub fn resolve_true_client_ip_address(socket_addr: SocketAddr, header_map: &HeaderMap) -> String {
    for header in CLIENT_IP_HEADERS {
        let Some(value) = header_map.get(header) else {
            continue;
        };
        let Ok(value) = value.to_str() else {
            warn!("failed to parse HTTP `{header}` header as text; skipping it");
            continue;
        };

        let candidate: &str = if header == "X-Forwarded-For" {
            value.split(',').next().unwrap_or_default()
        } else {
            value
        }
        .trim();

        if candidate.is_empty() {
            continue;
        }

        debug!("resolved client IP from HTTP `{header}` header");
        return candidate.to_string();
    }

    debug!("no proxy headers present; falling back to the socket address");
    socket_addr.ip().to_string()
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::http::{HeaderMap, HeaderValue};

    use super::resolve_true_client_ip_address;

    fn socket() -> SocketAddr {
        "10.0.0.1:44321".parse().expect("valid socket address")
    }

    #[test]
    fn with_no_headers_it_falls_back_to_the_socket() {
        let expected: String = String::from("10.0.0.1");
        let actual: String = resolve_true_client_ip_address(socket(), &HeaderMap::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn cloudflare_beats_the_conventional_headers() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert("X-Forwarded-For", HeaderValue::from_static("2.2.2.2"));
        headers.insert("True-Client-IP", HeaderValue::from_static("1.1.1.1"));

        let expected: String = String::from("1.1.1.1");
        let actual: String = resolve_true_client_ip_address(socket(), &headers);
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_left_most_forwarded_for_entry_is_the_client() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert(
            "X-Forwarded-For",
            HeaderValue::from_static("3.3.3.3, 70.41.3.18, 150.172.238.178"),
        );

        let expected: String = String::from("3.3.3.3");
        let actual: String = resolve_true_client_ip_address(socket(), &headers);
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_blank_header_is_skipped_rather_than_returned() {
        let mut headers: HeaderMap = HeaderMap::new();
        headers.insert("True-Client-IP", HeaderValue::from_static("   "));
        headers.insert("X-Real-IP", HeaderValue::from_static("4.4.4.4"));

        let expected: String = String::from("4.4.4.4");
        let actual: String = resolve_true_client_ip_address(socket(), &headers);
        assert_eq!(expected, actual);
    }
}
