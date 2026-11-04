//! Request ids and Prometheus metrics, on for every server.

use std::net::SocketAddr;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use axum::Router;
use axum::extract::{MatchedPath, Request};
use axum::http::{HeaderName, HeaderValue, Method};
use axum::middleware::Next;
use axum::response::Response;
use axum::routing::get;
use metrics_exporter_prometheus::{
    Matcher, PrometheusBuilder, PrometheusHandle, PrometheusRecorder,
};
use tokio::net::TcpListener;
use tower_http::request_id::{MakeRequestId, MakeRequestUuid, RequestId};
use tracing::{error, info};

use super::shutdown::Shutdown;

/// The port `/metrics` is served on, inside the container, in production only.
///
/// Fixed on purpose: every container has its own network namespace, so the same
/// port never collides, and it is never published, so only the scraper on the
/// host (the container's bridge address) can reach it. Not served locally, where
/// two projects running at once would collide.
pub const METRICS_PORT: u16 = 9000;

/// The request id header: reused when nginx sent one, generated otherwise, and
/// echoed on the response.
pub const REQUEST_ID_HEADER: HeaderName = HeaderName::from_static("x-request-id");

/// Upper bound on an incoming request id. nginx's `$request_id` is 32 hex
/// characters; anything longer or stranger is replaced rather than logged.
const MAX_REQUEST_ID_LEN: usize = 64;

/// Seconds. Sized for page loads: a few milliseconds to a slow ten seconds.
const DURATION_BUCKETS: [f64; 11] = [
    0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0,
];

/// Histograms are only drained into their buckets by upkeep.
const UPKEEP_INTERVAL: Duration = Duration::from_secs(5);

static RELEASE: OnceLock<String> = OnceLock::new();
static HANDLE: OnceLock<PrometheusHandle> = OnceLock::new();

/// Records the application's release (`name@version`) for `build_info`.
pub(crate) fn set_release(release: &str) {
    let _ = RELEASE.set(release.to_owned());
}

/// The process-wide recorder, installed on first use.
///
/// A second server in the same process (tests) shares it rather than fighting
/// over the one global recorder slot.
pub(crate) fn handle() -> &'static PrometheusHandle {
    HANDLE.get_or_init(|| {
        let recorder: PrometheusRecorder = PrometheusBuilder::new()
            .set_buckets_for_metric(
                Matcher::Full(String::from("http_request_duration_seconds")),
                &DURATION_BUCKETS,
            )
            .expect("the duration buckets are non-empty")
            .build_recorder();
        let handle: PrometheusHandle = recorder.handle();
        let _ = metrics::set_global_recorder(recorder);
        let release: String = RELEASE.get().cloned().unwrap_or_default();
        metrics::gauge!(
            "build_info",
            "release" => release,
            "webserver_base" => env!("CARGO_PKG_VERSION")
        )
        .set(1.0);
        handle
    })
}

fn is_valid_request_id(value: &HeaderValue) -> bool {
    let bytes: &[u8] = value.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_REQUEST_ID_LEN
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'-' || *byte == b'_')
}

/// Puts a request id on every request and response.
///
/// Runs outside the trace layer, so the request span (and so every log line of
/// the request) carries the same id as nginx's access log line.
pub(crate) async fn request_id(mut request: Request, next: Next) -> Response {
    let id: HeaderValue = match request.headers().get(&REQUEST_ID_HEADER) {
        Some(value) if is_valid_request_id(value) => value.clone(),
        _ => MakeRequestUuid
            .make_request_id(&request)
            .map(RequestId::into_header_value)
            .expect("a UUID always makes a request id"),
    };
    request.headers_mut().insert(REQUEST_ID_HEADER, id.clone());
    let mut response: Response = next.run(request).await;
    response.headers_mut().insert(REQUEST_ID_HEADER, id);
    response
}

/// Methods outside the standard set share one label, so a client cannot grow
/// the series count.
fn method_label(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::HEAD => "HEAD",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::PATCH => "PATCH",
        Method::DELETE => "DELETE",
        Method::OPTIONS => "OPTIONS",
        _ => "other",
    }
}

/// `http_requests_total`, `http_request_duration_seconds` and
/// `http_requests_in_flight`, labelled by route pattern (`/blog/{slug}`, never
/// the raw path) so the series count stays bounded. Unmatched paths share
/// `unmatched`.
pub(crate) async fn http_metrics(request: Request, next: Next) -> Response {
    let method: &'static str = method_label(request.method());
    let route: String = request.extensions().get::<MatchedPath>().map_or_else(
        || String::from("unmatched"),
        |path| path.as_str().to_owned(),
    );
    let started: Instant = Instant::now();

    metrics::gauge!("http_requests_in_flight").increment(1.0);
    let response: Response = next.run(request).await;
    metrics::gauge!("http_requests_in_flight").decrement(1.0);

    let status: String = response.status().as_u16().to_string();
    metrics::counter!(
        "http_requests_total",
        "method" => method,
        "route" => route.clone(),
        "status" => status
    )
    .increment(1);
    metrics::histogram!("http_request_duration_seconds", "method" => method, "route" => route)
        .record(started.elapsed().as_secs_f64());
    response
}

/// Serves `/metrics` on [`METRICS_PORT`] until shutdown.
///
/// Never fails the site: a port that cannot be bound degrades metrics only, so
/// it is reported to Sentry and the site keeps serving.
pub(crate) async fn serve_metrics(shutdown: Shutdown) {
    let address: SocketAddr = SocketAddr::from(([0, 0, 0, 0], METRICS_PORT));
    let listener: TcpListener = match TcpListener::bind(address).await {
        Ok(listener) => listener,
        Err(source) => {
            error!(%address, error = %source, "metrics listener not started; the site still serves");
            return;
        }
    };
    let handle: &'static PrometheusHandle = handle();
    let router: Router =
        Router::new().route("/metrics", get(move || async move { handle.render() }));

    let upkeep_shutdown: Shutdown = shutdown.clone();
    tokio::spawn(async move {
        let mut interval: tokio::time::Interval = tokio::time::interval(UPKEEP_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => handle.run_upkeep(),
                () = upkeep_shutdown.clone().recv() => break,
            }
        }
    });

    info!("metrics on http://{address}/metrics");
    if let Err(source) = axum::serve(listener, router)
        .with_graceful_shutdown(shutdown.recv())
        .await
    {
        error!(error = %source, "metrics listener stopped");
    }
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::body::Body;
    use axum::http::{HeaderValue, Request, StatusCode};
    use axum::response::Response;
    use tower::ServiceExt;

    use super::{REQUEST_ID_HEADER, handle, is_valid_request_id};
    use crate::environment::Environment;
    use crate::webserver::{Shutdown, WebServer};

    fn sidecar() -> Router {
        WebServer::new("127.0.0.1", 0, Environment::Local)
            .into_router(Shutdown::manual())
            .expect("a sidecar always boots")
    }

    async fn get(path: &str, request_id: Option<&'static str>) -> Response {
        let mut request: Request<Body> = Request::get(path).body(Body::empty()).expect("valid");
        if let Some(id) = request_id {
            request
                .headers_mut()
                .insert(REQUEST_ID_HEADER, HeaderValue::from_static(id));
        }
        sidecar().oneshot(request).await.expect("infallible")
    }

    fn response_id(response: &Response) -> String {
        response.headers()[&REQUEST_ID_HEADER]
            .to_str()
            .expect("ascii")
            .to_owned()
    }

    #[test]
    fn an_nginx_request_id_is_kept() {
        assert!(is_valid_request_id(&HeaderValue::from_static(
            "c99301cd50023a854cbb00c56e7f3537"
        )));
    }

    #[test]
    fn a_long_or_odd_request_id_is_replaced_rather_than_logged() {
        for bad in ["", "has space", "semi;colon", &"a".repeat(65)] {
            let value: HeaderValue = HeaderValue::from_str(bad).expect("ascii");
            assert!(!is_valid_request_id(&value), "{bad:?}");
        }
    }

    #[tokio::test]
    async fn the_id_nginx_sent_comes_back_on_the_response() {
        let response: Response =
            get("/api/v1/health", Some("c99301cd50023a854cbb00c56e7f3537")).await;

        let expected: &str = "c99301cd50023a854cbb00c56e7f3537";
        assert_eq!(expected, response_id(&response));
    }

    #[tokio::test]
    async fn a_request_without_a_usable_id_gets_a_fresh_one() {
        for sent in [None, Some("has;semicolon")] {
            let response: Response = get("/api/v1/health", sent).await;
            let id: String = response_id(&response);

            assert_eq!(36, id.len(), "a UUID, not {id:?}");
            assert!(is_valid_request_id(
                &HeaderValue::from_str(&id).expect("ascii")
            ));
        }
    }

    #[tokio::test]
    async fn requests_are_counted_by_route_pattern_and_unknown_paths_share_one_label() {
        let _ = handle();
        let health: Response = get("/api/v1/health", None).await;
        let missing: Response = get("/no/such/page/12345", None).await;
        assert_eq!(StatusCode::OK, health.status());
        assert_eq!(StatusCode::NOT_FOUND, missing.status());

        let rendered: String = handle().render();
        assert!(
            rendered.contains(
                r#"http_requests_total{method="GET",route="/api/v1/health",status="200"}"#
            ),
            "{rendered}"
        );
        assert!(
            rendered
                .contains(r#"http_requests_total{method="GET",route="unmatched",status="404"}"#),
            "{rendered}"
        );
        assert!(!rendered.contains("12345"), "raw paths never become labels");
        assert!(
            rendered.contains("http_request_duration_seconds_bucket"),
            "a histogram, not a summary"
        );
        assert!(rendered.contains("build_info{"), "{rendered}");
    }
}
