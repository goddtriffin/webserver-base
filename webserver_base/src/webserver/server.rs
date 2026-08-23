//! The web server builder.

use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Router, serve};
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::LatencyUnit;
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tracing::{Level, info, instrument, warn};

use crate::env::{self, EnvError};
use crate::environment::Environment;

use super::error::WebServerError;
use super::shutdown::{DEFAULT_DRAIN_TIMEOUT, Shutdown};
use super::state::{StateParts, WebServerState};

/// The environment variable holding the bind host.
pub const ENV_HOST: &str = "WSB_HOST";

/// The environment variable holding the bind port.
pub const ENV_PORT: &str = "WSB_PORT";

/// The port used when [`ENV_PORT`] is unset.
pub const DEFAULT_PORT: u16 = 8080;

/// The request body ceiling used when none is set.
pub const DEFAULT_BODY_LIMIT: usize = 1024;

/// The prefix every built-in endpoint is nested under.
pub const DEFAULT_API_PREFIX: &str = "/api/v1";

/// An HTTP server.
///
/// Requires only an address, a port and an [`Environment`]; everything else is
/// opt-in, from a sidecar serving one health check to a full site. A single
/// binary can run several of these and drain them from one [`Shutdown`].
pub struct WebServer<S = ()> {
    host: String,
    port: u16,
    environment: Environment,
    app: S,

    body_limit: usize,
    drain_timeout: Duration,
    api_prefix: String,
    health: bool,

    router: Router<WebServerState<S>>,

    #[cfg(feature = "templates")]
    base: Option<crate::templates::BaseTemplateData>,
    #[cfg(feature = "templates")]
    templates: Option<crate::templates::TemplateRegistry<'static>>,
    #[cfg(feature = "assets")]
    asset_directory: Option<String>,
    #[cfg(feature = "assets")]
    cache_manifest_directory: Option<String>,
    #[cfg(feature = "analytics")]
    analytics: Option<crate::analytics::AnalyticsConfig>,
    #[cfg(feature = "pages")]
    pages: Option<super::pages::Pages<S>>,
}

impl WebServer<()> {
    /// A server with no application state.
    #[must_use]
    pub fn new(host: impl Into<String>, port: u16, environment: Environment) -> Self {
        Self::with_state(host, port, environment, ())
    }

    /// A server with no application state, reading [`ENV_HOST`] and
    /// [`ENV_PORT`].
    ///
    /// `WSB_HOST` defaults to `127.0.0.1` locally and `0.0.0.0` in production;
    /// `WSB_PORT` defaults to [`DEFAULT_PORT`].
    ///
    /// # Errors
    ///
    /// [`WebServerError::Env`] if either variable is set but malformed.
    pub fn from_env(environment: Environment) -> Result<Self, WebServerError> {
        Self::from_env_with_state(environment, ())
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
            drain_timeout: DEFAULT_DRAIN_TIMEOUT,
            api_prefix: String::from(DEFAULT_API_PREFIX),
            health: false,
            router: Router::new(),
            #[cfg(feature = "templates")]
            base: None,
            #[cfg(feature = "templates")]
            templates: None,
            #[cfg(feature = "assets")]
            asset_directory: None,
            #[cfg(feature = "assets")]
            cache_manifest_directory: None,
            #[cfg(feature = "analytics")]
            analytics: None,
            #[cfg(feature = "pages")]
            pages: None,
        }
    }

    /// A server carrying application state, reading [`ENV_HOST`] and
    /// [`ENV_PORT`].
    ///
    /// # Errors
    ///
    /// [`WebServerError::Env`] if either variable is set but malformed.
    pub fn from_env_with_state(environment: Environment, app: S) -> Result<Self, WebServerError> {
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

    /// Serves `GET {api_prefix}/health`, answering `200`.
    #[must_use]
    pub const fn health(mut self) -> Self {
        self.health = true;
        self
    }

    /// Caps request bodies. Defaults to [`DEFAULT_BODY_LIMIT`].
    #[must_use]
    pub const fn body_limit(mut self, body_limit: usize) -> Self {
        self.body_limit = body_limit;
        self
    }

    /// How long in-flight work gets once shutdown starts, defaulting to
    /// [`DEFAULT_DRAIN_TIMEOUT`]. Raise it for long uploads.
    #[must_use]
    pub const fn drain_timeout(mut self, drain_timeout: Duration) -> Self {
        self.drain_timeout = drain_timeout;
        self
    }

    /// Changes the prefix the built-in endpoints are nested under.
    #[must_use]
    pub fn api_prefix(mut self, api_prefix: impl Into<String>) -> Self {
        self.api_prefix = api_prefix.into();
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

    /// Nests a bare `tower` service under `path` — a `ServeDir`, or a
    /// self-contained `Router<()>`.
    #[must_use]
    pub fn nest_service<T>(mut self, path: &str, service: T) -> Self
    where
        T: tower::Service<axum::extract::Request, Error = std::convert::Infallible>
            + Clone
            + Send
            + Sync
            + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static,
    {
        self.router = self.router.nest_service(path, service);
        self
    }

    /// Supplies the template registry and the site's base data.
    #[cfg(feature = "templates")]
    #[must_use]
    pub fn templates(
        mut self,
        registry: crate::templates::TemplateRegistry<'static>,
        base: crate::templates::BaseTemplateData,
    ) -> Self {
        self.templates = Some(registry);
        self.base = Some(base);
        self
    }

    /// Serves content-hashed static assets out of `asset_directory`.
    ///
    /// Hashed in place at start-up, after the sitemap is written so that it is
    /// hashed too. Adds `/static`, `/favicon.ico`, `/robots.txt`,
    /// `/humans.txt` and `/sitemap.xml`.
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn assets(mut self, asset_directory: impl Into<String>) -> Self {
        self.asset_directory = Some(asset_directory.into());
        self
    }

    /// Also writes `cache-buster.json` into `directory`, for tooling outside
    /// this process.
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn write_cache_manifest(mut self, directory: impl Into<String>) -> Self {
        self.cache_manifest_directory = Some(directory.into());
        self
    }

    /// Serves `POST {api_prefix}/scitylana`, forwarding pageviews to Plausible.
    #[cfg(feature = "analytics")]
    #[must_use]
    pub fn analytics(mut self, config: crate::analytics::AnalyticsConfig) -> Self {
        self.analytics = Some(config);
        self
    }

    /// Declares the site's pages, producing both routes and `sitemap.xml`.
    #[cfg(feature = "pages")]
    #[must_use]
    pub fn pages(mut self, pages: super::pages::Pages<S>) -> Self {
        self.pages = Some(pages);
        self
    }

    /// Binds, serves, and drains on `shutdown`, returning once the drain
    /// completes or [`WebServer::drain_timeout`] elapses.
    ///
    /// # Errors
    ///
    /// [`WebServerError`] if the sitemap or asset cache cannot be built, the
    /// listener cannot bind, or the server stops with an error.
    #[instrument(skip_all)]
    #[expect(
        clippy::too_many_lines,
        reason = "one linear assembly, read top to bottom"
    )]
    pub async fn run(self, shutdown: Shutdown) -> Result<(), WebServerError> {
        let Self {
            host,
            port,
            environment,
            app,
            body_limit,
            drain_timeout,
            api_prefix,
            health,
            router,
            #[cfg(feature = "templates")]
            base,
            #[cfg(feature = "templates")]
            templates,
            #[cfg(feature = "assets")]
            asset_directory,
            #[cfg(feature = "assets")]
            cache_manifest_directory,
            #[cfg(feature = "analytics")]
            analytics,
            #[cfg(feature = "pages")]
            pages,
        } = self;

        // ── 1. sitemap, before anything hashes the directory it lives in ────
        #[cfg(feature = "pages")]
        let not_found = {
            let mut not_found = None;
            if let Some(pages) = pages.as_ref() {
                let Some(base) = base.as_ref() else {
                    return Err(WebServerError::PagesRequireTemplates);
                };
                let urls = pages.sitemap_urls()?;
                crate::sitemap::write_sitemap(
                    base.base_url(),
                    &urls,
                    crate::sitemap::SITEMAP_OUTPUT_PATH,
                )?;
                info!("wrote {} urls to the sitemap", urls.len());
                not_found = pages.not_found_page();
            }
            not_found
        };

        // ── 2. content-hash the asset directory ─────────────────────────────
        #[cfg(feature = "assets")]
        let cache_buster: Option<crate::assets::CacheBuster> = match asset_directory.as_ref() {
            None => None,
            Some(directory) => {
                let cache_buster = crate::assets::CacheBuster::build(directory)?;
                if let Some(manifest_directory) = cache_manifest_directory.as_ref() {
                    cache_buster.write_manifest(manifest_directory)?;
                }
                info!("hashed {} assets", cache_buster.cache().len());
                Some(cache_buster)
            }
        };

        // ── 3. assemble the state every handler sees ────────────────────────
        #[cfg(feature = "analytics")]
        let analytics_handler = analytics.map(|config| {
            std::sync::Arc::new(crate::analytics::AnalyticsHandler::new(config, environment))
        });

        let state: WebServerState<S> = WebServerState::new(StateParts {
            host: host.clone(),
            port,
            environment,
            shutdown: shutdown.clone(),
            #[cfg(feature = "templates")]
            base,
            #[cfg(feature = "templates")]
            templates,
            #[cfg(feature = "assets")]
            cache_buster: cache_buster.clone(),
            #[cfg(feature = "analytics")]
            analytics: analytics_handler,
            app,
        });

        // ── 4. routes ───────────────────────────────────────────────────────
        let mut no_cache: Router<WebServerState<S>> = router;

        #[cfg(feature = "pages")]
        if let Some(pages) = pages {
            no_cache = no_cache.merge(pages.into_router());
        }

        let mut built_in: Router<WebServerState<S>> = Router::new();
        if health {
            built_in = built_in.route("/health", get(health_check));
        }
        #[cfg(feature = "analytics")]
        {
            built_in = built_in.route("/scitylana", axum::routing::post(analytics_endpoint::<S>));
        }
        no_cache = no_cache.nest(&api_prefix, built_in);

        #[cfg(feature = "assets")]
        if let Some(cache_buster) = cache_buster.as_ref() {
            no_cache = attach_singleton_files(no_cache, cache_buster);
        }

        // Nothing outside `/static` carries a content hash, so nothing outside
        // `/static` may be cached.
        #[cfg(feature = "assets")]
        {
            no_cache = no_cache.layer(axum::middleware::from_fn(
                crate::assets::CacheBuster::never_cache_middleware,
            ));
        }

        let mut app_router: Router<WebServerState<S>> = no_cache;

        #[cfg(feature = "assets")]
        if let Some(directory) = asset_directory.as_ref() {
            let forever: Router<WebServerState<S>> = Router::new()
                .nest_service("/static", tower_http::services::ServeDir::new(directory))
                .layer(axum::middleware::from_fn(
                    crate::assets::CacheBuster::forever_cache_middleware,
                ));
            app_router = app_router.merge(forever);
        }

        // ── 5. the 404, served in place with a real status ──────────────────
        #[cfg(feature = "pages")]
        let app_router = match not_found {
            Some((page, data)) => app_router.fallback(move |axum::extract::State(state)| {
                let page = std::sync::Arc::clone(&page);
                let data = std::sync::Arc::clone(&data);
                async move {
                    let body: Response = super::pages::render_or_500(&state, &page, &data);
                    (StatusCode::NOT_FOUND, body).into_response()
                }
            }),
            None => app_router.fallback(plain_not_found),
        };
        #[cfg(not(feature = "pages"))]
        let app_router = app_router.fallback(plain_not_found);

        // ── 6. outer layers ─────────────────────────────────────────────────
        let service_builder = ServiceBuilder::new()
            .layer(DefaultBodyLimit::max(body_limit))
            .layer(
                TraceLayer::new_for_http()
                    .make_span_with(
                        DefaultMakeSpan::default()
                            .level(Level::INFO)
                            .include_headers(false),
                    )
                    .on_response(
                        DefaultOnResponse::new()
                            .level(Level::INFO)
                            .latency_unit(LatencyUnit::Micros),
                    ),
            );

        #[cfg(feature = "observability")]
        let app_router = app_router.with_state(state).layer(
            service_builder.layer(sentry::integrations::tower::NewSentryLayer::new_from_top()),
        );
        #[cfg(not(feature = "observability"))]
        let app_router = app_router.with_state(state).layer(service_builder);

        // ── 7. bind and serve ───────────────────────────────────────────────
        let ip: IpAddr = IpAddr::from_str(&host).map_err(|_| {
            WebServerError::Env(EnvError::Invalid {
                key: String::from(ENV_HOST),
                expected: "IP address",
                value: host.clone(),
            })
        })?;
        let addr: SocketAddr = SocketAddr::new(ip, port);

        let listener: TcpListener = TcpListener::bind(&addr)
            .await
            .map_err(|source| WebServerError::Bind { addr, source })?;
        info!("listening on http://{addr}");

        let server = serve(
            listener,
            app_router.into_make_service_with_connect_info::<SocketAddr>(),
        )
        .with_graceful_shutdown(shutdown.clone().recv());

        tokio::select! {
            result = server => {
                result.map_err(WebServerError::Serve)?;
                info!("{addr} drained cleanly");
            }
            () = drain_deadline(shutdown, drain_timeout) => {
                warn!(
                    "{addr} did not drain within {drain_timeout:?}; \
                     stopping with connections still open"
                );
            }
        }

        Ok(())
    }
}

/// Resolves once shutdown has started and the drain window has elapsed.
async fn drain_deadline(shutdown: Shutdown, drain_timeout: Duration) {
    shutdown.recv().await;
    tokio::time::sleep(drain_timeout).await;
}

/// `GET {api_prefix}/health`.
async fn health_check() -> StatusCode {
    StatusCode::OK
}

/// The fallback when no 404 page was declared.
async fn plain_not_found() -> Response {
    (StatusCode::NOT_FOUND, "not found").into_response()
}

/// `POST {api_prefix}/scitylana`.
#[cfg(feature = "analytics")]
async fn analytics_endpoint<S>(
    axum::extract::State(state): axum::extract::State<WebServerState<S>>,
    axum::extract::ConnectInfo(addr): axum::extract::ConnectInfo<SocketAddr>,
    headers: axum::http::HeaderMap,
    axum::Form(request): axum::Form<crate::analytics::AnalyticsRequest>,
) -> StatusCode
where
    S: Clone + Send + Sync + 'static,
{
    match state.analytics() {
        Some(handler) => handler.handle(&headers, addr, request).await,
        None => StatusCode::NOT_FOUND,
    }
}

/// Serves the well-known single files from their hashed locations.
#[cfg(feature = "assets")]
fn attach_singleton_files<S>(
    router: Router<WebServerState<S>>,
    cache_buster: &crate::assets::CacheBuster,
) -> Router<WebServerState<S>>
where
    S: Clone + Send + Sync + 'static,
{
    use tower_http::services::ServeFile;

    let directory: &str = cache_buster.asset_directory();
    let files: [(&str, String); 4] = [
        (
            "/favicon.ico",
            format!("{directory}/image/favicon/favicon.ico"),
        ),
        ("/robots.txt", format!("{directory}/file/robots.txt")),
        ("/humans.txt", format!("{directory}/file/humans.txt")),
        ("/sitemap.xml", format!("{directory}/file/sitemap.xml")),
    ];

    let mut router: Router<WebServerState<S>> = router;
    for (route, original) in files {
        let hashed: String = cache_buster.get_file(&original);
        if !std::path::Path::new(&hashed).exists() {
            warn!("skipping `{route}`: `{original}` does not exist");
            continue;
        }
        router = router.nest_service(route, ServeFile::new(hashed));
    }
    router
}
