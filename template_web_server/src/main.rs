use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::handler::HandlerWithoutStateExt;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{serve, Form, Router};
use axum_extra::routing::RouterExt;
use chrono::{DateTime, Utc};
use reqwest::Client;
use sentry::integrations::tower::NewSentryLayer;
use sentry::ClientInitGuard;
use sitemap_rs::image::Image;
use sitemap_rs::url::{ChangeFrequency, Url, DEFAULT_PRIORITY};
use sitemap_rs::url_builder::UrlBuilder;
use sitemap_rs::url_set::UrlSet;
use std::fs::File;
use std::io::BufWriter;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use template_web_server::template_data::TemplateData;
use template_web_server::webserver_error::WebserverResult;
use tokio::net::TcpListener;
use tower::ServiceBuilder;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::trace::{DefaultMakeSpan, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::{info, instrument, Level};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::EnvFilter;
use webserver_base::{
    AxumPlausibleAnalyticsHandler, BaseSettings, CacheBuster, Page, RequestPayload,
    TemplateRegistry,
};

#[derive(Clone)]
struct AppState {
    settings: BaseSettings,
    cache_buster: CacheBuster,
    template_registry: TemplateRegistry<'static>,
    template_data: TemplateData,
    plausible_client: Arc<AxumPlausibleAnalyticsHandler>,
}

impl AppState {
    #[instrument(skip_all)]
    pub fn new(settings: &BaseSettings) -> WebserverResult<Self> {
        // sitemaps
        generate_sitemaps(settings)?;

        // generate CacheBuster (must occur after sitemap generation)
        let mut cache_buster: CacheBuster = CacheBuster::new("static");
        cache_buster.gen_cache();
        cache_buster.update_source_map_references();
        info!("{}", cache_buster);
        cache_buster.print_to_file("..");

        Ok(Self {
            settings: settings.clone(),
            cache_buster: cache_buster.clone(),
            template_registry: TemplateRegistry::new()?,
            template_data: TemplateData::new(settings.clone(), &cache_buster),
            plausible_client: Arc::new(AxumPlausibleAnalyticsHandler::new_with_client(
                Client::new(),
            )),
        })
    }
}

#[instrument(skip_all)]
fn main() {
    // env vars
    let settings: BaseSettings = BaseSettings::default();

    // sentry
    let _guard: ClientInitGuard = sentry::init((
        settings.sentry_dsn.clone(),
        sentry::ClientOptions {
            release: sentry::release_name!(),
            attach_stacktrace: true,
            ..Default::default()
        },
    ));

    // Must manually create a multithreaded tokio runtime so that the Sentry hub will be applied to all threads
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
        .block_on(async_main(settings))
        .unwrap();
}

#[instrument(skip_all)]
async fn async_main(settings: BaseSettings) -> WebserverResult<()> {
    // initialize tracing
    tracing_subscriber::Registry::default()
        .with(
            EnvFilter::try_from_default_env()
                .or_else(|_| EnvFilter::try_new("info"))
                .unwrap(),
        )
        .with(tracing_subscriber::fmt::Layer::default())
        .with(sentry::integrations::tracing::layer())
        .init();

    // app state
    let app_state: AppState = AppState::new(&settings)?;

    // API routes
    let v1_api_routes: Router<Arc<AppState>> = Router::new()
        .route_with_tsr("/health", get(health_check))
        .route_with_tsr("/scitylana", post(analytics))
        .fallback(fallback);

    // build our application with a route
    let app: Router = Router::new()
        .route("/", get(home))
        .route_with_tsr("/404", get(four_oh_four))
        .nest("/api/v1", v1_api_routes)
        .nest_service(
            "/static",
            ServeDir::new("static").fallback(fallback.into_service()),
        )
        .nest_service(
            "/favicon.ico",
            ServeFile::new(
                app_state
                    .cache_buster
                    .get_file("static/image/favicon/favicon.ico"),
            ),
        )
        .nest_service(
            "/robots.txt",
            ServeFile::new(app_state.cache_buster.get_file("static/file/robots.txt")),
        )
        .nest_service(
            "/sitemap.xml",
            ServeFile::new(app_state.cache_buster.get_file("static/file/sitemap.xml")),
        )
        .nest_service(
            "/humans.txt",
            ServeFile::new(app_state.cache_buster.get_file("static/file/humans.txt")),
        )
        .fallback(fallback)
        .with_state(Arc::new(app_state))
        .layer(
            ServiceBuilder::new()
                .layer(DefaultBodyLimit::max(1024))
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
                )
                // TODO - figure out how to make SentryHttpLayer work
                .layer(NewSentryLayer::new_from_top()),
        );

    // run it
    let addr: SocketAddr = SocketAddr::new(
        IpAddr::from_str(settings.host.as_str()).expect("failed to parse host"),
        settings.port,
    );
    info!("listening on {}", addr);
    let listener: TcpListener = TcpListener::bind(&addr).await.unwrap();
    serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();

    Ok(())
}

#[instrument(skip_all)]
async fn home(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(
        state
            .template_registry
            .render(
                "home",
                &state.template_data.clone().render(Page::new(
                    String::from("Home"),
                    String::from("/"),
                    vec![String::from("static/stylesheet/main.css")],
                    vec![String::from("static/script/scitylana.js")],
                )),
            )
            .unwrap(),
    )
}

#[instrument(skip_all)]
async fn four_oh_four(State(state): State<Arc<AppState>>) -> Html<String> {
    Html(
        state
            .template_registry
            .render(
                "404",
                &state.template_data.clone().render(Page::new(
                    String::from("404"),
                    String::from("/404"),
                    vec![String::from("static/stylesheet/main.css")],
                    vec![String::from("static/script/scitylana.js")],
                )),
            )
            .unwrap(),
    )
}

#[instrument(skip_all)]
async fn fallback() -> Response {
    Redirect::to("/404").into_response()
}

async fn health_check() -> StatusCode {
    StatusCode::OK
}

#[instrument(skip_all)]
async fn analytics(
    headers: HeaderMap,
    State(state): State<Arc<AppState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Form(incoming_payload): Form<RequestPayload>,
) -> StatusCode {
    let plausible_client: Arc<AxumPlausibleAnalyticsHandler> = Arc::clone(&state.plausible_client);
    plausible_client
        .handle(headers, state.settings.clone(), addr, incoming_payload)
        .await
}

#[instrument(skip_all)]
fn generate_sitemaps(settings: &BaseSettings) -> WebserverResult<()> {
    // track all the base routes (e.g. "/blog", "/projects", etc.)
    let base_routes: Vec<&str> = vec!["/"];

    // generate <url> for all routes
    let mut urls: Vec<Url> = Vec::new();

    // base routes
    for base_route in base_routes {
        // create URL
        let mut url_builder: UrlBuilder =
            Url::builder(format!("{}{base_route}", settings.home_url));
        url_builder
            .last_modified(DateTime::from(Utc::now()))
            .change_frequency(ChangeFrequency::Weekly)
            .priority(DEFAULT_PRIORITY);

        // only add image for home page
        if base_route.is_empty() {
            url_builder.images(vec![Image::new(format!(
                "{}/static/image/social/profile-picture.webp",
                settings.home_url
            ))]);
        }

        // store URL
        urls.push(url_builder.build()?);
    }

    // write sitemap.xml to static files directory
    let url_set: UrlSet = UrlSet::new(urls)?;
    url_set.write(BufWriter::new(File::create("./static/file/sitemap.xml")?))?;
    Ok(())
}
