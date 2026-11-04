//! [`WebServer::into_router`] against a real built site, from a root that is
//! not the working directory.
//!
//! That second half is the point. Every file path the server reads has to be
//! joined onto the root, and a missed join still passes wherever the root is the
//! working directory — which is every other test and every deploy. Here it never
//! is: `cargo test` runs from the crate, and the site is built in a scratch
//! directory from the source tree beside this file.

use std::fs;
use std::path::{Path, PathBuf};

use axum::Router;
use axum::body::Body;
use axum::extract::{Path as UrlPath, State};
use axum::http::{Request, StatusCode, Uri, header};
use axum::response::Response;
use handlebars::handlebars_helper;
use serde_json::json;
use tower::ServiceExt;

use crate::analytics::AnalyticsConfig;
use crate::assets::generate::generate_static_assets_in;
use crate::assets::{CacheBuster, CacheBusterError, Phase};
use crate::environment::Environment;
use crate::templates::{
    BaseTemplateData, BaseTemplateDataParams, Fallback, PageTemplateData, SiteEntity, ThemeColor,
};

use crate::templates::TemplateError;

use super::{FrontendParams, NotFound, Pages, Shutdown, WebServer, WebServerError, WebServerState};

// A project's own helper, which the fixture's home page calls.
handlebars_helper!(shout: |word: str| word.to_uppercase());

/// A matched route whose resource does not exist.
async fn thing(State(state): State<WebServerState>, UrlPath(slug): UrlPath<String>) -> Response {
    state.not_found(json!({ "hint": format!("no thing {slug}") }))
}

const STYLESHEET: &str = "static/stylesheet/main.css";

/// The un-built source tree. Never built in place: hashing renames files.
fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src/webserver/into_router_tests/site")
}

/// A fresh copy of the fixture, not yet built.
fn scratch_site(name: &str) -> PathBuf {
    let root: PathBuf = std::env::temp_dir().join(format!("wsb-into-router-{name}"));
    let _ = fs::remove_dir_all(&root);
    copy_tree(&fixture(), &root);
    root
}

/// A copy of the fixture, run through the real asset pipeline.
fn built_site(name: &str) -> PathBuf {
    let root: PathBuf = scratch_site(name);
    generate_static_assets_in(&root, Phase::NonScripts).expect("the fixture builds");
    root
}

fn copy_tree(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("scratch directory");
    for entry in fs::read_dir(from).expect("fixture directory") {
        let path: PathBuf = entry.expect("fixture entry").path();
        let target: PathBuf = to.join(path.file_name().expect("named entry"));
        if path.is_dir() {
            copy_tree(&path, &target);
        } else {
            fs::copy(&path, &target).expect("fixture file");
        }
    }
}

fn frontend() -> FrontendParams {
    FrontendParams::new(
        BaseTemplateData::new(BaseTemplateDataParams {
            project: String::from("Fixture"),
            description: String::from("A site that exists to be booted."),
            author: String::from("Fixture Author"),
            base_url: String::from("https://fixture.example"),
            twitter_username: String::from("fixture"),
            social_image: String::from("static/image/social/card.webp"),
            social_image_alt: String::from("Fixture card"),
            theme_color: ThemeColor::light_dark("#ffffff", "#000000"),
            theme_fallback: Fallback::Dark,
            site_entity: SiteEntity::person("Fixture Author"),
            language_code: String::from("en"),
            country_code: String::from("US"),
            see_also: Vec::new(),
            copyright_start: String::from("2026"),
            style_sheets: vec![String::from(STYLESHEET)],
            scripts: Vec::new(),
        })
        .with_site(json!({
            "phone": "(555) 010-0000",
            "nav": [
                { "href": "/", "label": "Home" },
                { "href": "/things", "label": "Things" },
            ],
        })),
        Pages::new()
            .static_page(PageTemplateData::new("home", "Home", "/"), ())
            .dynamic_page("/things/{slug}", axum::routing::get(thing))
            .unlisted(),
        AnalyticsConfig::new("fixture"),
        "https://abc123@o4504844394627072.ingest.sentry.io/4504862450515968",
    )
    .with_helper("shout", shout)
}

fn site_router(root: &Path) -> Result<Router, WebServerError> {
    site_router_with(root, frontend())
}

fn site_router_with(root: &Path, params: FrontendParams) -> Result<Router, WebServerError> {
    WebServer::new("127.0.0.1", 0, Environment::Local)
        .root_dir(root)
        .frontend(params)
        .into_router(Shutdown::manual())
}

async fn get(router: &Router, uri: &str) -> Response {
    router
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("a valid request"),
        )
        .await
        .expect("a router never fails")
}

async fn text(response: Response) -> String {
    let bytes: axum::body::Bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("a readable body");
    String::from_utf8(bytes.to_vec()).expect("a UTF-8 body")
}

#[tokio::test]
async fn a_page_renders_and_links_its_stylesheet_by_its_hashed_url() {
    let root: PathBuf = built_site("page");
    let router: Router = site_router(&root).expect("the built site boots");
    let hashed: String = CacheBuster::load_in(&root)
        .expect("the manifest the build wrote")
        .get_file(STYLESHEET);

    let response: Response = get(&router, "/").await;
    assert_eq!(StatusCode::OK, response.status());

    let body: String = text(response).await;
    assert!(body.contains("home of Fixture"), "{body}");
    assert!(body.contains(&format!("/{hashed}")), "{body}");
}

#[tokio::test]
async fn a_hashed_asset_is_served_from_the_root_and_cached_forever() {
    let root: PathBuf = built_site("static");
    let router: Router = site_router(&root).expect("the built site boots");
    let hashed: String = CacheBuster::load_in(&root)
        .expect("the manifest the build wrote")
        .get_file(STYLESHEET);

    let response: Response = get(&router, &format!("/{hashed}")).await;
    assert_eq!(StatusCode::OK, response.status());

    let cache_control: &str = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(cache_control.contains("immutable"), "{cache_control}");

    let expected: String =
        fs::read_to_string(fixture().join(STYLESHEET)).expect("the fixture stylesheet");
    let actual: String = text(response).await;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn a_static_miss_is_a_404_that_is_never_cached() {
    // A hashed URL can be asked for before its file is deployed; caching that
    // 404 immutable would outlive the deploy by a year.
    let root: PathBuf = built_site("static-miss");
    let router: Router = site_router(&root).expect("the built site boots");

    let response: Response = get(&router, "/static/stylesheet/main.0000000000.css").await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());

    let cache_control: &str = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(cache_control.contains("no-store"), "{cache_control}");
    assert!(!cache_control.contains("immutable"), "{cache_control}");
}

#[tokio::test]
async fn a_generated_icon_is_served_at_its_well_known_path() {
    let root: PathBuf = built_site("icon");
    let router: Router = site_router(&root).expect("the built site boots");

    let expected: StatusCode = StatusCode::OK;
    let actual: StatusCode = get(&router, "/favicon.ico").await.status();
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn an_unknown_path_renders_the_sites_own_404_page_with_its_site_data() {
    let root: PathBuf = built_site("not-found");
    let router: Router = site_router(&root).expect("the built site boots");

    let response: Response = get(&router, "/no-such-page").await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());

    // The fallback sits outside the never-cache layer, so it must stamp its
    // own headers.
    let cache_control: &str = response
        .headers()
        .get(header::CACHE_CONTROL)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    assert!(cache_control.contains("no-store"), "{cache_control}");

    let body: String = text(response).await;
    assert!(body.contains("fixture 404"), "{body}");
    // The header partial reads `{{site}}`; null data here was the original bug.
    assert!(body.contains("call (555) 010-0000"), "{body}");
    assert!(body.contains("noindex, follow"), "{body}");
    assert!(!body.contains(" hint "), "{body}");
}

#[tokio::test]
async fn the_fallback_404_renders_what_the_projects_hook_returns_for_the_requested_uri() {
    let root: PathBuf = built_site("not-found-hook");
    let params: FrontendParams = frontend().with_not_found(NotFound::with(
        |_state: WebServerState, uri: Uri| async move { json!({ "hint": uri.path() }) },
    ));
    let router: Router = site_router_with(&root, params).expect("the built site boots");

    let response: Response = get(&router, "/blgo/rust").await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());

    let body: String = text(response).await;
    assert!(body.contains("fixture 404 hint /blgo/rust"), "{body}");
    assert!(body.contains("call (555) 010-0000"), "{body}");
}

#[tokio::test]
async fn a_handler_whose_resource_is_missing_returns_the_same_404_with_its_own_data() {
    let root: PathBuf = built_site("handler-not-found");
    let router: Router = site_router(&root).expect("the built site boots");

    let response: Response = get(&router, "/things/nope").await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());

    let body: String = text(response).await;
    assert!(body.contains("fixture 404 hint no thing nope"), "{body}");
    assert!(body.contains("call (555) 010-0000"), "{body}");
    assert!(body.contains("noindex, follow"), "{body}");
}

#[tokio::test]
async fn every_page_gets_the_site_data_and_its_own_nav_item_highlighted() {
    let root: PathBuf = built_site("site-data");
    let router: Router = site_router(&root).expect("the built site boots");

    let body: String = text(get(&router, "/").await).await;
    assert!(body.contains("call (555) 010-0000"), "{body}");
    assert!(
        body.contains(r#"<a href="/" class="active">Home</a>"#),
        "{body}"
    );
    assert!(body.contains(r#"<a href="/things">Things</a>"#), "{body}");
}

#[tokio::test]
async fn a_projects_own_helper_renders() {
    let root: PathBuf = built_site("helper");
    let router: Router = site_router(&root).expect("the built site boots");

    let body: String = text(get(&router, "/").await).await;
    assert!(body.contains("home of Fixture, LOUD"), "{body}");
}

#[tokio::test]
async fn a_project_helper_named_like_a_built_in_is_refused_at_boot() {
    let root: PathBuf = built_site("helper-collision");
    let params: FrontendParams = frontend().with_helper("join", shout);

    let error: WebServerError =
        site_router_with(&root, params).expect_err("a built-in cannot be overridden");
    assert!(
        matches!(
            error,
            WebServerError::Template(TemplateError::ReservedHelper { ref name }) if name == "join"
        ),
        "{error}"
    );
}

#[tokio::test]
async fn a_handler_404_on_a_server_with_no_frontend_is_still_a_404() {
    let router: Router = WebServer::new("127.0.0.1", 0, Environment::Local)
        .root_dir(std::env::temp_dir().join("wsb-into-router-no-such-root"))
        .merge(Router::new().route(
            "/things/{slug}",
            axum::routing::get(
                |State(state): State<WebServerState>| async move { state.not_found(()) },
            ),
        ))
        .into_router(Shutdown::manual())
        .expect("a sidecar always boots");

    let response: Response = get(&router, "/things/nope").await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());

    let expected: String = String::from("404");
    let actual: String = text(response).await;
    assert_eq!(expected, actual);
}

#[tokio::test]
async fn a_site_whose_build_never_ran_fails_to_boot_instead_of_serving_unhashed_links() {
    let root: PathBuf = scratch_site("unbuilt");

    let error: WebServerError = site_router(&root).expect_err("no manifest, no boot");
    assert!(
        matches!(
            error,
            WebServerError::CacheBuster(CacheBusterError::MissingManifest { .. })
        ),
        "{error}"
    );
}

#[tokio::test]
async fn a_server_with_no_frontend_boots_from_a_root_holding_nothing() {
    // The sidecar guarantee, through the new path: no templates, no icons, no
    // `static/`, and still a working health check.
    let root: PathBuf = std::env::temp_dir().join("wsb-into-router-no-such-root");
    let router: Router = WebServer::new("127.0.0.1", 0, Environment::Local)
        .root_dir(root)
        .into_router(Shutdown::manual())
        .expect("a sidecar always boots");

    let expected: StatusCode = StatusCode::OK;
    let actual: StatusCode = get(&router, "/api/v1/health").await.status();
    assert_eq!(expected, actual);
}
