//! The web server builder.

use std::future::{Future, IntoFuture};
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Router, serve};
use tokio::net::TcpListener;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{Level, error, info, instrument, warn};

use crate::env;
use crate::environment::Environment;

use super::error::WebServerError;
use super::shutdown::{AppShutdown, DEFAULT_DRAIN_TIMEOUT, Shutdown};
use super::state::{StateParts, WebServerState};

/// The environment variable holding the bind host.
pub const ENV_HOST: &str = "WSB_HOST";

/// The environment variable holding the bind port.
pub const ENV_PORT: &str = "WSB_PORT";

/// The port used when [`ENV_PORT`] is unset.
pub const DEFAULT_PORT: u16 = 8080;

/// The request body ceiling used when none is set.
///
/// 256 KiB comfortably covers a form post or a JSON payload while still
/// bounding abuse. A project that accepts uploads raises it explicitly.
pub const DEFAULT_BODY_LIMIT: usize = 256 * 1024;

/// The prefix every built-in endpoint is nested under.
///
/// Fixed rather than configurable: every route in every project is then
/// predictable from the outside, and the derived analytics endpoint can be
/// stated in the documentation without qualification.
pub const API_PREFIX: &str = "/api/v1";

/// An HTTP server.
///
/// Requires only an address, a port and an [`Environment`]. A sidecar that
/// serves nothing but its health check is a valid server; a full site adds
/// [`frontend`](WebServer::frontend). A single binary can run several of these
/// and drain them from one [`Shutdown`].
pub struct WebServer<S = ()> {
    host: String,
    port: u16,
    environment: Environment,
    app: S,

    body_limit: usize,
    router: Router<WebServerState<S>>,

    #[cfg(feature = "pages")]
    frontend: Option<super::frontend::FrontendParams<S>>,

    /// Gated on `pages` rather than `feed`: a feed without a frontend is a
    /// boot failure, so the two always travel together.
    #[cfg(feature = "pages")]
    feed: Option<crate::feed::Feed>,
}

impl WebServer<()> {
    /// A server with no application state.
    #[must_use]
    pub fn new(host: impl Into<String>, port: u16, environment: Environment) -> Self {
        Self::with_state(host, port, environment, ())
    }

    /// A server with no application state, read entirely from the environment.
    ///
    /// `WSB_ENVIRONMENT` is required. `WSB_HOST` defaults to `127.0.0.1`
    /// locally and `0.0.0.0` in production; `WSB_PORT` defaults to
    /// [`DEFAULT_PORT`].
    ///
    /// # Errors
    ///
    /// [`WebServerError::Env`] if a variable is missing or malformed.
    pub fn from_env() -> Result<Self, WebServerError> {
        Self::from_env_with_state(())
    }
}

impl<S> WebServer<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// A server carrying application state.
    #[must_use]
    pub fn with_state(
        host: impl Into<String>,
        port: u16,
        environment: Environment,
        app: S,
    ) -> Self {
        Self {
            host: host.into(),
            port,
            environment,
            app,
            body_limit: DEFAULT_BODY_LIMIT,
            router: Router::new(),
            #[cfg(feature = "pages")]
            frontend: None,
            #[cfg(feature = "pages")]
            feed: None,
        }
    }

    /// A server carrying application state, read entirely from the
    /// environment.
    ///
    /// # Errors
    ///
    /// [`WebServerError::Env`] if a variable is missing or malformed.
    pub fn from_env_with_state(app: S) -> Result<Self, WebServerError> {
        let environment: Environment = Environment::from_env()?;
        let host: String = env::optional(ENV_HOST).unwrap_or_else(|| {
            if environment.is_production() {
                String::from("0.0.0.0")
            } else {
                String::from("127.0.0.1")
            }
        });
        let port: u16 =
            env::parse_or(ENV_PORT, "port number", DEFAULT_PORT).map_err(WebServerError::Env)?;

        Ok(Self::with_state(host, port, environment, app))
    }

    /// Caps request bodies. Defaults to [`DEFAULT_BODY_LIMIT`].
    ///
    /// The one size that genuinely varies: an image-upload endpoint and a
    /// landing page have nothing in common here.
    #[must_use]
    pub const fn body_limit(mut self, body_limit: usize) -> Self {
        self.body_limit = body_limit;
        self
    }

    /// Nests a router under `path`.
    #[must_use]
    pub fn nest(mut self, path: &str, router: Router<WebServerState<S>>) -> Self {
        self.router = self.router.nest(path, router);
        self
    }

    /// Merges a router at the root.
    #[must_use]
    pub fn merge(mut self, router: Router<WebServerState<S>>) -> Self {
        self.router = self.router.merge(router);
        self
    }

    /// Nests a service under `path`.
    #[must_use]
    pub fn nest_service<T>(mut self, path: &str, service: T) -> Self
    where
        T: tower::Service<axum::extract::Request, Error = std::convert::Infallible>
            + Clone
            + Send
            + Sync
            + 'static,
        T::Response: axum::response::IntoResponse,
        T::Future: Send + 'static,
    {
        self.router = self.router.nest_service(path, service);
        self
    }

    /// Declares this server a frontend: it serves HTML to humans.
    ///
    /// This is the line between a site and a service, and everything downstream
    /// hangs off it — the embedded layout, the icon set, the web manifest,
    /// `robots.txt`, the sitemaps, analytics and browser error monitoring. A
    /// server that never calls it needs none of them and boots clean.
    #[cfg(feature = "pages")]
    #[must_use]
    pub fn frontend(mut self, params: super::frontend::FrontendParams<S>) -> Self {
        self.frontend = Some(params);
        self
    }

    /// Gives this site a feed, served as RSS 2.0, Atom 1.0 and JSON Feed.
    ///
    /// Omittable, because most sites publish no stream: a site that never calls
    /// this serves no feed documents and emits no autodiscovery links. Requires
    /// [`frontend`](WebServer::frontend) — calling this without it fails the
    /// boot rather than silently serving nothing.
    ///
    /// Entries are supplied rather than derived: a blog is a
    /// [`dynamic_page_group`](Pages::dynamic_page_group), and a dynamic page
    /// builds its template data per request, so there is nothing to read at
    /// boot. They may be in any order and may exceed
    /// [`MAX_FEED_ENTRIES`](crate::feed::MAX_FEED_ENTRIES); the newest survive.
    #[cfg(feature = "pages")]
    #[must_use]
    pub fn feed(mut self, feed: crate::feed::Feed) -> Self {
        self.feed = Some(feed);
        self
    }

    /// Binds, serves, and drains.
    ///
    /// # Errors
    ///
    /// [`WebServerError`] if the host is unparseable, the port cannot be bound,
    /// the frontend cannot be assembled, or serving fails.
    #[instrument(skip_all)]
    pub async fn run(self, shutdown: Shutdown) -> Result<(), WebServerError>
    where
        S: AppShutdown,
    {
        let Self {
            host,
            port,
            environment,
            app,
            body_limit,
            router,
            #[cfg(feature = "pages")]
            frontend,
            #[cfg(feature = "pages")]
            feed,
        } = self;

        // A feed on a server that is not a frontend is never served, and
        // nothing at runtime would say so.
        #[cfg(feature = "pages")]
        if feed.is_some() && frontend.is_none() {
            return Err(WebServerError::FeedWithoutFrontend);
        }

        // Hashing is a build step, so this only ever reads what the build
        // produced. A project with no `static/` gets an empty map.
        let cache_buster: crate::assets::CacheBuster = crate::assets::CacheBuster::load()?;

        let mut no_cache: Router<WebServerState<S>> = router;
        let mut built_in: Router<WebServerState<S>> = Router::new().route("/health", get(health));

        #[cfg(feature = "pages")]
        let mut frontend_runtime: Option<crate::templates::FrontendRuntime> = None;
        #[cfg(feature = "pages")]
        let mut base: Option<crate::templates::BaseTemplateData> = None;
        #[cfg(feature = "pages")]
        let mut templates: Option<crate::templates::TemplateRegistry<'static>> = None;
        #[cfg(feature = "pages")]
        let mut not_found: Option<(
            std::sync::Arc<crate::templates::PageTemplateData>,
            std::sync::Arc<serde_json::Value>,
        )> = None;
        #[cfg(feature = "pages")]
        let mut proxy_scripts: Option<Router<WebServerState<S>>> = None;
        #[cfg(feature = "pages")]
        let mut feed_router: Option<Router<WebServerState<S>>> = None;

        log_immutable_assets(&cache_buster);

        #[cfg(feature = "pages")]
        if let Some(params) = frontend {
            let assembled: Assembled<S> =
                assemble_frontend(params, feed, &cache_buster, environment)?;

            no_cache = no_cache.merge(assembled.routes);
            built_in = built_in.merge(assembled.api);
            proxy_scripts = Some(assembled.proxy_scripts);
            feed_router = assembled.feeds;
            not_found = Some(assembled.not_found);
            frontend_runtime = Some(assembled.runtime);
            base = Some(assembled.base);
            templates = Some(assembled.templates);
        }

        let no_cache: Router<WebServerState<S>> = no_cache.nest(API_PREFIX, built_in);

        let mut app_router: Router<WebServerState<S>> = apply_cache_policy(no_cache, &cache_buster);

        // Merged after the never-cache layer so the upstream's own headers
        // survive: the vendor scripts carry their own policy, and stamping
        // `no-store` over it would re-download them on every page view.
        #[cfg(feature = "pages")]
        if let Some(scripts) = proxy_scripts {
            app_router = app_router.merge(scripts);
        }

        #[cfg(feature = "pages")]
        if let Some(feeds) = feed_router {
            app_router = app_router.merge(feeds);
        }

        #[cfg(feature = "pages")]
        let app_router: Router<WebServerState<S>> = attach_not_found(app_router, not_found);
        #[cfg(not(feature = "pages"))]
        let app_router: Router<WebServerState<S>> = app_router.fallback(plain_not_found);

        let state: WebServerState<S> = WebServerState::new(StateParts {
            host: host.clone(),
            port,
            environment,
            shutdown: shutdown.clone(),
            #[cfg(feature = "templates")]
            base,
            #[cfg(feature = "templates")]
            templates,
            cache_buster: Some(cache_buster),
            #[cfg(feature = "templates")]
            frontend: frontend_runtime,
            app,
        });

        // The router takes the state by value; cleanup needs it after serving
        // ends, and `WebServerState` is an `Arc` so this costs a refcount.
        let cleanup: WebServerState<S> = state.clone();

        // Applied outermost-last, so the body cap runs before tracing sees a
        // request it may never finish reading.
        let app_router: Router = app_router
            .with_state(state)
            .layer(
                TraceLayer::new_for_http()
                    // The span carries the method and URI, and the response
                    // event is logged inside it. Left at its default DEBUG
                    // level the span is never created under an INFO filter, so
                    // every request logs a status and a latency with no way to
                    // tell which route it was.
                    .make_span_with(
                        DefaultMakeSpan::new()
                            .level(Level::INFO)
                            .include_headers(false),
                    )
                    .on_response(
                        DefaultOnResponse::new()
                            .level(Level::INFO)
                            .latency_unit(LatencyUnit::Millis),
                    ),
            )
            .layer(DefaultBodyLimit::max(body_limit));

        serve_on(app_router, &host, port, shutdown, async move {
            cleanup.app().on_shutdown().await;
        })
        .await
    }
}

/// Attaches the 404 fallback.
///
/// A frontend renders its own page at the requested URL with a real 404 status;
/// a service with no pages answers with a bare body. Split out of `run` because
/// it is the one branch there that is about a single route rather than about
/// assembling the router.
#[cfg(feature = "pages")]
fn attach_not_found<S>(
    router: Router<WebServerState<S>>,
    not_found: Option<(
        std::sync::Arc<crate::templates::PageTemplateData>,
        std::sync::Arc<serde_json::Value>,
    )>,
) -> Router<WebServerState<S>>
where
    S: Clone + Send + Sync + 'static,
{
    match not_found {
        Some((page, data)) => router.fallback(move |axum::extract::State(state)| {
            let page: std::sync::Arc<crate::templates::PageTemplateData> =
                std::sync::Arc::clone(&page);
            let data: std::sync::Arc<serde_json::Value> = std::sync::Arc::clone(&data);
            async move {
                let body: Response = super::pages::render_or_500(&state, &page, &data);
                (StatusCode::NOT_FOUND, body).into_response()
            }
        }),
        None => router.fallback(plain_not_found),
    }
}

/// Binds and serves until the last connection closes, or the drain window
/// shuts, whichever comes first.
///
/// `on_shutdown` is the application's own cleanup. It is only ever awaited if a
/// shutdown signal actually arrives — a server that dies on a bind error drops
/// it unrun, rather than hanging on cleanup nobody asked for.
async fn serve_on<F>(
    router: Router,
    host: &str,
    port: u16,
    shutdown: Shutdown,
    on_shutdown: F,
) -> Result<(), WebServerError>
where
    F: Future<Output = ()> + Send,
{
    let ip: IpAddr = IpAddr::from_str(host).map_err(|source| WebServerError::Bind {
        addr: SocketAddr::from(([0, 0, 0, 0], port)),
        source: std::io::Error::new(std::io::ErrorKind::InvalidInput, source),
    })?;
    let address: SocketAddr = SocketAddr::new(ip, port);
    let listener: TcpListener =
        TcpListener::bind(address)
            .await
            .map_err(|source| WebServerError::Bind {
                addr: address,
                source,
            })?;

    info!("listening on http://{address}");

    // The one binding in the crate with no written type: `WithGracefulShutdown`
    // is generic over the listener, the make-service, the service and the
    // shutdown future, and rustc itself elides it when printing.
    let serving = serve(
        listener,
        router.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown.clone().recv())
    .into_future();
    tokio::pin!(serving);

    // The drain window is a ceiling on the wait, not the wait itself, so the
    // clock cannot start until the drain does — and `serving` resolves the
    // moment the last connection closes, which for an idle server is at once.
    tokio::select! {
        result = &mut serving => return result.map_err(WebServerError::Serve),
        () = shutdown.recv() => {}
    }

    // Two ceilings started at one instant, not one after the other: the drain
    // and the cleanup are independent, so the process leaves in the longer of
    // the two rather than their sum — which is what keeps the whole shutdown
    // inside a single orchestrator kill window.
    let (served, cleaned) = tokio::join!(
        tokio::time::timeout(DEFAULT_DRAIN_TIMEOUT, &mut serving),
        tokio::time::timeout(DEFAULT_DRAIN_TIMEOUT, on_shutdown),
    );

    // The hook's own reporting never runs when it is cancelled from out here,
    // so the overrun has to be reported from out here too, or the work it
    // failed to finish is lost silently.
    if cleaned.is_err() {
        error!(
            "app shutdown hook exceeded {DEFAULT_DRAIN_TIMEOUT:?}; cleanup was cancelled part-way"
        );
    }

    match served {
        Ok(result) => result.map_err(WebServerError::Serve),
        // Expected of anything holding a socket open, not a misconfiguration:
        // the ceiling exists precisely because such a connection never ends.
        Err(_elapsed) => {
            warn!("drain window elapsed after {DEFAULT_DRAIN_TIMEOUT:?}; closing what remained");
            Ok(())
        }
    }
}

/// The two cache policies: nothing outside `/static` may be cached, and
/// everything inside it is immutable because its URL carries a content hash.
fn apply_cache_policy<S>(
    router: Router<WebServerState<S>>,
    cache_buster: &crate::assets::CacheBuster,
) -> Router<WebServerState<S>>
where
    S: Clone + Send + Sync + 'static,
{
    let router: Router<WebServerState<S>> = router.layer(axum::middleware::from_fn(
        crate::assets::CacheBuster::never_cache_middleware,
    ));

    if cache_buster.is_empty() {
        return router;
    }

    router.merge(
        Router::new()
            .nest_service(
                "/static",
                tower_http::services::ServeDir::new(crate::assets::STATIC_DIRECTORY),
            )
            .layer(axum::middleware::from_fn(
                crate::assets::CacheBuster::forever_cache_middleware,
            )),
    )
}

/// `GET /api/v1/health`. Always routed: every deployment target expects one,
/// and a bool to switch it off was surface for nothing.
async fn health() -> StatusCode {
    StatusCode::OK
}

/// The fallback for a server with no 404 page of its own.
async fn plain_not_found() -> Response {
    (StatusCode::NOT_FOUND, "404").into_response()
}

/// Serves the generated and embedded documents from memory.
///
/// None of these is a file. Sitemaps need the route table, so they are built at
/// boot; `robots.txt` and the manifest are derived from site data; humans.txt is
/// compiled in. All are served at fixed never-cached routes where a content hash
/// would mean nothing — which is also why the server needs no writable disk.
#[cfg(feature = "pages")]
fn well_known_routes<S>(well_known: &super::frontend::WellKnown) -> Router<WebServerState<S>>
where
    S: Clone + Send + Sync + 'static,
{
    use axum::http::header;

    fn text<S>(
        router: Router<WebServerState<S>>,
        path: &str,
        content_type: &'static str,
        body: String,
    ) -> Router<WebServerState<S>>
    where
        S: Clone + Send + Sync + 'static,
    {
        router.route(
            path,
            get(move || {
                let body: String = body.clone();
                async move { ([(header::CONTENT_TYPE, content_type)], body) }
            }),
        )
    }

    let mut router: Router<WebServerState<S>> = Router::new();
    router = text(
        router,
        "/robots.txt",
        "text/plain; charset=utf-8",
        well_known.robots_txt.clone(),
    );
    router = text(
        router,
        "/humans.txt",
        "text/plain; charset=utf-8",
        well_known.humans_txt.clone(),
    );
    router = text(
        router,
        "/site.webmanifest",
        "application/manifest+json",
        well_known.webmanifest.clone(),
    );
    router = text(
        router,
        crate::sitemap::SITEMAP_INDEX_PATH,
        "application/xml",
        well_known.sitemaps.index().to_string(),
    );
    for (index, chunk) in well_known.sitemaps.chunks().iter().enumerate() {
        router = text(
            router,
            &format!("/sitemap-{}.xml", index + 1),
            "application/xml",
            chunk.clone(),
        );
    }
    router
}

/// Serves the icon set at the well-known root paths browsers actually request.
///
/// Each one resolves through the manifest to its hashed file under `/static`,
/// so the same bytes are reachable both ways: immutable at the hashed URL, and
/// never-cached here where the URL cannot change.
#[cfg(feature = "pages")]
fn icon_routes<S>(
    cache_buster: &crate::assets::CacheBuster,
    has_svg_icon: bool,
) -> Router<WebServerState<S>>
where
    S: Clone + Send + Sync + 'static,
{
    use tower_http::services::ServeFile;

    let mut icons: Vec<(&str, String)> = vec![
        (
            "/favicon.ico",
            String::from("static/image/favicon/favicon.ico"),
        ),
        (
            "/apple-touch-icon.png",
            String::from("static/image/favicon/apple-touch-icon.png"),
        ),
        (
            "/icon-192.png",
            String::from("static/image/favicon/icon-192.png"),
        ),
        (
            "/icon-512.png",
            String::from("static/image/favicon/icon-512.png"),
        ),
    ];
    // Only a project whose art is vector has one to serve.
    if has_svg_icon {
        icons.push((
            "/favicon.svg",
            String::from("static/image/favicon/favicon.svg"),
        ));
    }

    let mut router: Router<WebServerState<S>> = Router::new();
    for (route, original) in icons {
        // Existence was proved at boot, so this is a resolution, not a check.
        let hashed: String = cache_buster.get_file(&original);
        router = router.nest_service(route, ServeFile::new(hashed));
    }
    router
}

/// The feed routes, with the conditional-GET handling the rest of the site has
/// no use for.
///
/// A feed is polled relentlessly — a few hundred subscribers across Feedly,
/// Inoreader and `NetNewsWire` is tens of thousands of requests a day for a
/// document that changes twice a year. A strong validator computed at boot
/// turns all but the first of those into a bodiless `304`.
///
/// These routes never see [`CacheBuster::never_cache_middleware`], which
/// removes `ETag` and `If-None-Match` outright.
#[cfg(feature = "pages")]
fn feed_routes<S>(feeds: &crate::feed::FeedSet) -> Router<WebServerState<S>>
where
    S: Clone + Send + Sync + 'static,
{
    use axum::http::{HeaderMap, HeaderValue, StatusCode, header};

    // Parsed once, at boot: a header value that cannot be built is a
    // programming error, and discovering it per request would mean either a
    // panic in a handler or a feed served without its validators.
    let last_modified: HeaderValue = HeaderValue::from_str(
        &feeds
            .last_modified()
            .format("%a, %d %b %Y %H:%M:%S GMT")
            .to_string(),
    )
    .unwrap_or_else(|_| HeaderValue::from_static(""));
    let cache_control: HeaderValue = HeaderValue::from_str(&format!(
        "public, max-age={}",
        crate::feed::FEED_MAX_AGE_SECONDS
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("public"));

    let mut router: Router<WebServerState<S>> = Router::new();
    for document in feeds.documents() {
        let body: String = String::from(document.body());
        let etag_text: String = String::from(document.etag());
        let etag: HeaderValue =
            HeaderValue::from_str(&etag_text).unwrap_or_else(|_| HeaderValue::from_static(""));
        let content_type: HeaderValue = HeaderValue::from_static(document.content_type());
        let last_modified: HeaderValue = last_modified.clone();
        let cache_control: HeaderValue = cache_control.clone();

        router = router.route(
            document.path(),
            get(move |headers: HeaderMap| {
                let body: String = body.clone();
                let etag_text: String = etag_text.clone();
                let etag: HeaderValue = etag.clone();
                let content_type: HeaderValue = content_type.clone();
                let last_modified: HeaderValue = last_modified.clone();
                let cache_control: HeaderValue = cache_control.clone();
                async move {
                    let fresh: bool = if_none_match(&headers, &etag_text);

                    // Built rather than tupled so the headers are *inserted*:
                    // appending leaves axum's own `text/plain` for the string
                    // body in place ahead of ours, and a reader that reads the
                    // first `Content-Type` then treats the feed as plain text.
                    let mut response: Response = if fresh {
                        StatusCode::NOT_MODIFIED.into_response()
                    } else {
                        (StatusCode::OK, body).into_response()
                    };

                    let headers: &mut HeaderMap = response.headers_mut();
                    headers.insert(header::CACHE_CONTROL, cache_control);
                    headers.insert(header::ETAG, etag);
                    headers.insert(header::LAST_MODIFIED, last_modified);
                    if !fresh {
                        headers.insert(header::CONTENT_TYPE, content_type);
                    }

                    response
                }
            }),
        );
    }

    router
}

/// Whether the request already holds this exact document.
///
/// `If-None-Match` is a comma-separated list, entries may be weak (`W/"…"`),
/// and `*` matches anything that exists — so a bare string comparison against
/// the header would 200 on requests that should 304.
#[cfg(feature = "pages")]
fn if_none_match(headers: &axum::http::HeaderMap, etag: &str) -> bool {
    let Some(header) = headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
    else {
        return false;
    };

    header.split(',').any(|candidate| {
        let candidate: &str = candidate.trim();
        candidate == "*" || candidate.trim_start_matches("W/") == etag
    })
}

/// Names every immutable asset and the URL it is actually served at, once per
/// boot.
///
/// Per-request logging cannot answer the question this exists for: a request
/// for an immutable asset reaching the origin is indistinguishable from a first
/// visit, a bot, a hard refresh or an evicted entry, and only one of those is a
/// fault. What *is* checkable is whether the tier was wired up at all, and that
/// is decided once, here. The hashed path is printed because it is the URL to
/// `curl` when verifying the edge.
fn log_immutable_assets(cache_buster: &crate::assets::CacheBuster) {
    for (original, hashed) in cache_buster.cache() {
        info!("immutable 1y  /{original} -> /{hashed}");
    }
}

/// Names the generated documents and the tier each is served under.
#[cfg(feature = "pages")]
fn log_served_documents(well_known: &super::frontend::WellKnown) {
    for path in ["/robots.txt", "/humans.txt", "/site.webmanifest"] {
        info!("uncached      {path}");
    }
    for path in well_known.sitemaps.paths() {
        info!("uncached      {path}");
    }

    if let Some(feeds) = well_known.feeds.as_ref() {
        for document in feeds.documents() {
            info!(
                "cached {}m    {} (etag {})",
                crate::feed::FEED_MAX_AGE_SECONDS / 60,
                document.path(),
                document.etag()
            );
        }
    }
}

/// Everything a frontend contributes to the router, assembled.
///
/// A struct rather than a tuple of eight: `run` merges these into differently
/// cached layers, and two routers transposed would put the analytics script
/// behind `no-store`.
#[cfg(feature = "pages")]
struct Assembled<S> {
    /// Pages, icons and the never-cached well-known documents.
    routes: Router<WebServerState<S>>,
    /// Endpoints to nest under the API prefix.
    api: Router<WebServerState<S>>,
    /// Vendor scripts, which carry their own cache policy.
    proxy_scripts: Router<WebServerState<S>>,
    /// Feed documents, which carry theirs.
    feeds: Option<Router<WebServerState<S>>>,
    runtime: crate::templates::FrontendRuntime,
    base: crate::templates::BaseTemplateData,
    templates: crate::templates::TemplateRegistry<'static>,
    not_found: (
        std::sync::Arc<crate::templates::PageTemplateData>,
        std::sync::Arc<serde_json::Value>,
    ),
}

/// Builds the frontend and sorts its routes by the cache policy each needs.
#[cfg(feature = "pages")]
fn assemble_frontend<S>(
    params: super::frontend::FrontendParams<S>,
    feed: Option<crate::feed::Feed>,
    cache_buster: &crate::assets::CacheBuster,
    environment: Environment,
) -> Result<Assembled<S>, WebServerError>
where
    S: Clone + Send + Sync + 'static,
{
    let templates: crate::templates::TemplateRegistry<'static> =
        crate::templates::TemplateRegistry::from_dir(crate::templates::TEMPLATE_ROOT)?;

    let built: super::frontend::Frontend<S> =
        super::frontend::Frontend::build(params, feed, cache_buster, environment)?;

    log_served_documents(&built.well_known);

    // Feeds are deliberately kept out of the never-cache layer: it strips
    // conditional headers and stamps `no-store`, which on the most-polled
    // document a site serves means re-sending every byte on every poll.
    let feeds: Option<Router<WebServerState<S>>> = built.well_known.feeds.as_ref().map(feed_routes);

    let mut routes: Router<WebServerState<S>> = well_known_routes(&built.well_known);
    routes = routes.merge(icon_routes(cache_buster, built.has_svg_icon));

    // The vendor scripts are likewise kept out: they carry the vendor's own
    // cache policy, and stamping `no-store` over it would re-download the
    // analytics script on every page view.
    let (proxy_scripts, api) = proxy_routes(&built);

    let not_found = built.not_found.clone();
    routes = routes.merge(built.pages.into_router());

    Ok(Assembled {
        routes,
        api,
        proxy_scripts,
        feeds,
        runtime: built.runtime,
        base: built.base,
        templates,
        not_found,
    })
}

/// The first-party proxy routes.
///
/// Split in two because the scripts sit at the root — where they read as
/// ordinary bundler output — while the endpoints they post to belong under the
/// API prefix.
#[cfg(feature = "pages")]
fn proxy_routes<S>(
    frontend: &super::frontend::Frontend<S>,
) -> (Router<WebServerState<S>>, Router<WebServerState<S>>)
where
    S: Clone + Send + Sync + 'static,
{
    use crate::analytics::{AnalyticsConfig, relay_envelope, relay_event, relay_script};
    use axum::body::Bytes;
    use axum::extract::ConnectInfo;
    use axum::http::HeaderMap;
    use axum::routing::post;

    let client: reqwest::Client = reqwest::Client::new();
    let paths: &crate::templates::FrontendRuntime = &frontend.runtime;

    let analytics_script_upstream: String = frontend.analytics.upstream_script_url();
    let analytics_event_upstream: String = AnalyticsConfig::upstream_event_url();
    let sentry_script_upstream: String = frontend.sentry_dsn.upstream_script_url();
    let sentry_envelope_upstream: String = frontend.sentry_dsn.upstream_envelope_url();

    let scripts: Router<WebServerState<S>> = Router::new()
        .route(
            &paths.analytics.script_path,
            get({
                let client: reqwest::Client = client.clone();
                let upstream: std::sync::Arc<str> =
                    std::sync::Arc::from(analytics_script_upstream.as_str());
                move || {
                    let client: reqwest::Client = client.clone();
                    let upstream: std::sync::Arc<str> = std::sync::Arc::clone(&upstream);
                    async move { relay_script(&client, &upstream).await }
                }
            }),
        )
        .route(
            &paths.sentry_browser.script_path,
            get({
                let client: reqwest::Client = client.clone();
                let upstream: std::sync::Arc<str> =
                    std::sync::Arc::from(sentry_script_upstream.as_str());
                move || {
                    let client: reqwest::Client = client.clone();
                    let upstream: std::sync::Arc<str> = std::sync::Arc::clone(&upstream);
                    async move { relay_script(&client, &upstream).await }
                }
            }),
        );

    // Nested under the API prefix, so the paths registered here are relative.
    let event_path: String = strip_api_prefix(&paths.analytics.event_path);
    let tunnel_path: String = strip_api_prefix(&paths.sentry_browser.tunnel_path);

    let endpoints: Router<WebServerState<S>> = Router::new()
        .route(
            &event_path,
            post({
                let client: reqwest::Client = client.clone();
                let upstream: std::sync::Arc<str> =
                    std::sync::Arc::from(analytics_event_upstream.as_str());
                move |ConnectInfo(peer): ConnectInfo<SocketAddr>,
                      headers: HeaderMap,
                      body: Bytes| {
                    let client: reqwest::Client = client.clone();
                    let upstream: std::sync::Arc<str> = std::sync::Arc::clone(&upstream);
                    async move { relay_event(&client, &upstream, &headers, peer, body).await }
                }
            }),
        )
        .route(
            &tunnel_path,
            post({
                let upstream: std::sync::Arc<str> =
                    std::sync::Arc::from(sentry_envelope_upstream.as_str());
                move |body: Bytes| {
                    let client: reqwest::Client = client.clone();
                    let upstream: std::sync::Arc<str> = std::sync::Arc::clone(&upstream);
                    async move { relay_envelope(&client, &upstream, body).await }
                }
            }),
        );

    (scripts, endpoints)
}

/// `/api/v1/thing` → `/thing`, for a router that will be nested under the
/// prefix.
#[cfg(feature = "pages")]
fn strip_api_prefix(path: &str) -> String {
    path.strip_prefix(API_PREFIX)
        .map_or_else(|| path.to_string(), String::from)
}

#[cfg(test)]
mod tests {

    #[cfg(feature = "pages")]
    mod conditional_get {
        use axum::http::{HeaderMap, HeaderValue, header};

        use crate::webserver::server::if_none_match;

        const ETAG: &str = "\"abc123\"";

        fn headers(value: &str) -> HeaderMap {
            let mut headers: HeaderMap = HeaderMap::new();
            headers.insert(
                header::IF_NONE_MATCH,
                HeaderValue::from_str(value).expect("a valid header"),
            );
            headers
        }

        #[test]
        fn a_request_without_the_header_always_gets_the_body() {
            let expected: bool = false;
            let actual: bool = if_none_match(&HeaderMap::new(), ETAG);
            assert_eq!(expected, actual);
        }

        #[test]
        fn the_same_validator_means_the_reader_already_has_this_feed() {
            let expected: bool = true;
            let actual: bool = if_none_match(&headers(ETAG), ETAG);
            assert_eq!(expected, actual);
        }

        #[test]
        fn a_weak_validator_still_matches_because_feeds_need_no_byte_equality() {
            let expected: bool = true;
            let actual: bool = if_none_match(&headers("W/\"abc123\""), ETAG);
            assert_eq!(expected, actual);
        }

        #[test]
        fn a_star_matches_anything_that_exists() {
            let expected: bool = true;
            let actual: bool = if_none_match(&headers("*"), ETAG);
            assert_eq!(expected, actual);
        }

        #[test]
        fn one_match_anywhere_in_the_list_is_enough() {
            // The header is a comma-separated list, so a naive string compare
            // against the whole value would re-send the body to a reader that
            // already holds it.
            let expected: bool = true;
            let actual: bool = if_none_match(&headers("\"other\", \"abc123\""), ETAG);
            assert_eq!(expected, actual);
        }

        #[test]
        fn a_stale_validator_gets_the_new_document() {
            let expected: bool = false;
            let actual: bool = if_none_match(&headers("\"stale\""), ETAG);
            assert_eq!(expected, actual);
        }
    }
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use axum::Router;
    use axum::routing::get;
    use tokio::time::timeout;

    use super::super::shutdown::Shutdown;
    use super::{
        API_PREFIX, AppShutdown, DEFAULT_BODY_LIMIT, DEFAULT_DRAIN_TIMEOUT, WebServerError, health,
        serve_on,
    };

    /// Records whether cleanup ran, and can be made slow enough to overrun the
    /// drain window.
    #[derive(Clone)]
    struct RecordingState {
        ran: Arc<AtomicBool>,
        linger: Option<Duration>,
    }

    impl RecordingState {
        fn instant() -> Self {
            Self {
                ran: Arc::new(AtomicBool::new(false)),
                linger: None,
            }
        }
    }

    impl AppShutdown for RecordingState {
        async fn on_shutdown(&self) {
            if let Some(linger) = self.linger {
                tokio::time::sleep(linger).await;
            }
            self.ran.store(true, Ordering::SeqCst);
        }
    }

    /// Serves `state` on an ephemeral port and returns once serving has ended.
    async fn serve_until_shutdown(
        state: RecordingState,
        shutdown: Shutdown,
    ) -> Result<(), WebServerError> {
        let router: Router = Router::new().route("/health", get(health));
        let cleanup: RecordingState = state.clone();
        // Port 0 asks the OS for a free one; nothing here connects to it.
        serve_on(router, "127.0.0.1", 0, shutdown, async move {
            cleanup.on_shutdown().await;
        })
        .await
    }

    #[tokio::test]
    async fn app_cleanup_runs_when_the_shutdown_signal_arrives() {
        let shutdown: Shutdown = Shutdown::manual();
        let state: RecordingState = RecordingState::instant();
        let ran: Arc<AtomicBool> = Arc::clone(&state.ran);

        shutdown.trigger();
        timeout(
            Duration::from_secs(1),
            serve_until_shutdown(state, shutdown.clone()),
        )
        .await
        .expect("serving ended promptly")
        .expect("serving ended cleanly");

        let expected: bool = true;
        let actual: bool = ran.load(Ordering::SeqCst);
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn app_cleanup_never_runs_when_the_server_dies_before_any_signal() {
        // An unparseable host fails before the listener binds, so no signal is
        // ever sent — the hook must be dropped unrun rather than awaited, or a
        // failed boot would hang until the drain window closed.
        let shutdown: Shutdown = Shutdown::manual();
        let state: RecordingState = RecordingState::instant();
        let ran: Arc<AtomicBool> = Arc::clone(&state.ran);
        let cleanup: RecordingState = state.clone();

        let router: Router = Router::new().route("/health", get(health));
        let result: Result<(), WebServerError> = timeout(
            Duration::from_secs(1),
            serve_on(router, "not-an-ip", 0, shutdown, async move {
                cleanup.on_shutdown().await;
            }),
        )
        .await
        .expect("a bind failure returns immediately, it does not wait for cleanup");

        assert!(result.is_err(), "an unparseable host is a bind error");

        let expected: bool = false;
        let actual: bool = ran.load(Ordering::SeqCst);
        assert_eq!(expected, actual);
    }

    #[tokio::test(start_paused = true)]
    async fn a_cleanup_that_overruns_the_window_is_cancelled_rather_than_awaited() {
        // Longer than the ceiling, so the hook cannot finish. With a paused
        // clock this costs no real time; the assertion is that serving still
        // returns, which it cannot do if the hook is awaited to completion.
        let shutdown: Shutdown = Shutdown::manual();
        let state: RecordingState = RecordingState {
            ran: Arc::new(AtomicBool::new(false)),
            linger: Some(DEFAULT_DRAIN_TIMEOUT * 2),
        };
        let ran: Arc<AtomicBool> = Arc::clone(&state.ran);

        shutdown.trigger();
        serve_until_shutdown(state, shutdown.clone())
            .await
            .expect("serving still ends cleanly when cleanup is cancelled");

        let expected: bool = false;
        let actual: bool = ran.load(Ordering::SeqCst);
        assert_eq!(
            expected, actual,
            "the hook was cancelled at its await, so it never reached its final store"
        );
    }

    #[tokio::test]
    async fn a_server_with_nothing_in_flight_stops_at_once_instead_of_waiting_out_the_window() {
        let shutdown: Shutdown = Shutdown::manual();
        let router: Router = Router::new().route("/health", get(health));

        // Port 0 asks the OS for a free one; nothing here connects to it.
        let serving: tokio::task::JoinHandle<Result<(), WebServerError>> = tokio::spawn({
            let shutdown: Shutdown = shutdown.clone();
            async move { serve_on(router, "127.0.0.1", 0, shutdown, async {}).await }
        });

        shutdown.trigger();

        // Far below the drain window: an unconditional wait fails here, which
        // is the bug — the window is a ceiling, not a delay.
        timeout(Duration::from_secs(1), serving)
            .await
            .expect("the server returned as soon as the drain began")
            .expect("the serving task did not panic")
            .expect("serving ended cleanly");
    }

    #[test]
    fn the_body_limit_accommodates_an_ordinary_form_post() {
        // 1 KiB — the previous default — rejected almost any real submission,
        // so every project had to override it.
        let expected: usize = 256 * 1024;
        let actual: usize = DEFAULT_BODY_LIMIT;
        assert_eq!(expected, actual);
    }

    #[cfg(feature = "pages")]
    #[test]
    fn stripping_the_prefix_leaves_a_nestable_path() {
        let expected: String = String::from("/boggledygook-a3f2c1d8");
        let actual: String = super::strip_api_prefix("/api/v1/boggledygook-a3f2c1d8");
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_api_prefix_is_the_one_every_project_shares() {
        assert_eq!("/api/v1", API_PREFIX);
    }
}
