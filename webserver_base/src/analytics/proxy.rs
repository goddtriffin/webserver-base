//! First-party proxies for Plausible and the Sentry browser SDK.
//!
//! Both are **byte relays**. Nothing is parsed, rewritten, or re-reported: the
//! upstream receives exactly what the browser sent, and the browser receives
//! exactly what the upstream returned, headers included. That matters most for
//! Sentry — a server that *interpreted* a client error and re-raised it through
//! its own SDK would file a browser problem as a server one, with the wrong
//! stack and the wrong context. Relaying the envelope keeps it a genuine
//! browser event.
//!
//! The point of proxying at all is that a same-origin request is
//! indistinguishable from the site's own assets. Plausible puts the cost of not
//! doing it at "typically between 5% and 25%" of visitors, depending on
//! audience.

use std::net::SocketAddr;

use axum::body::Bytes;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tracing::{error, instrument};

use super::ip::resolve_true_client_ip_address;

/// Fetches an upstream script and returns it with the upstream's own headers.
///
/// Content type and cache policy are echoed rather than invented, so the
/// proxied script behaves exactly as the vendor intends it to.
#[instrument(skip_all)]
pub async fn relay_script(client: &reqwest::Client, upstream: &str) -> Response {
    let response = match client.get(upstream).send().await {
        Ok(response) => response,
        Err(error) => {
            error!("failed to fetch `{upstream}`: {error}");
            return StatusCode::BAD_GATEWAY.into_response();
        }
    };

    let status: StatusCode =
        StatusCode::from_u16(response.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers: HeaderMap = HeaderMap::new();
    // ETag too, so a revalidation still costs nothing when the script has not
    // changed.
    for name in [header::CONTENT_TYPE, header::CACHE_CONTROL, header::ETAG] {
        if let Some(value) = response.headers().get(&name)
            && let Ok(value) = HeaderValue::from_bytes(value.as_bytes())
        {
            headers.insert(name, value);
        }
    }

    match response.bytes().await {
        Ok(body) => (status, headers, body).into_response(),
        Err(error) => {
            error!("failed to read `{upstream}`: {error}");
            StatusCode::BAD_GATEWAY.into_response()
        }
    }
}

/// Forwards a Plausible event, carrying the real visitor IP.
///
/// `X-Forwarded-For` is not optional: without it Plausible's bot filter rejects
/// proxied events outright, because every one of them would appear to originate
/// from this server.
#[instrument(skip_all)]
pub async fn relay_event(
    client: &reqwest::Client,
    upstream: &str,
    headers: &HeaderMap,
    peer: SocketAddr,
    body: Bytes,
) -> Response {
    let mut request = client.post(upstream).body(body).header(
        "X-Forwarded-For",
        resolve_true_client_ip_address(peer, headers),
    );
    if let Some(user_agent) = headers.get(header::USER_AGENT) {
        request = request.header(header::USER_AGENT, user_agent.clone());
    }
    if let Some(content_type) = headers.get(header::CONTENT_TYPE) {
        request = request.header(header::CONTENT_TYPE, content_type.clone());
    }

    match request.send().await {
        Ok(response) => StatusCode::from_u16(response.status().as_u16())
            .unwrap_or(StatusCode::ACCEPTED)
            .into_response(),
        Err(error) => {
            error!("failed to forward an analytics event: {error}");
            // A dropped pageview must never surface to the visitor.
            StatusCode::ACCEPTED.into_response()
        }
    }
}

/// Relays a Sentry envelope verbatim.
///
/// The body is passed through untouched — it is a complete envelope the browser
/// SDK built, carrying its own stack trace, breadcrumbs and release. The
/// destination comes from the configured DSN rather than from the envelope
/// header, so this cannot be pointed at an arbitrary Sentry project.
#[instrument(skip_all)]
pub async fn relay_envelope(client: &reqwest::Client, upstream: &str, body: Bytes) -> Response {
    match client
        .post(upstream)
        .header(header::CONTENT_TYPE, "application/x-sentry-envelope")
        .body(body)
        .send()
        .await
    {
        Ok(response) => StatusCode::from_u16(response.status().as_u16())
            .unwrap_or(StatusCode::OK)
            .into_response(),
        Err(error) => {
            error!("failed to relay a Sentry envelope: {error}");
            StatusCode::OK.into_response()
        }
    }
}
