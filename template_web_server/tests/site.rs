//! The built site, driven through the router `main` serves, without a socket.
//!
//! Serves `bin/`, so the assets must be built first: `make test` does that via
//! `make assets`. A bare `cargo test` without them fails at boot, by design —
//! the same boot validation a deploy with a missing build would hit.

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use template_web_server::{Settings, web_server};
use tower::ServiceExt;
use webserver_base::Environment;
use webserver_base::analytics::AnalyticsConfig;
use webserver_base::webserver::{Shutdown, WebServer};

const BIN: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../bin");

fn router() -> Router {
    let settings: Settings = Settings {
        project: String::from("Template Web Server"),
        description: String::from("Here is a description of the project."),
        base_url: String::from("https://www.template-web-server.com"),
        analytics: AnalyticsConfig::new("template-web-server"),
        sentry_browser_dsn: String::from(
            "https://abc123@o4504844394627072.ingest.sentry.io/4504862450515968",
        ),
    };
    let server: WebServer = WebServer::new("127.0.0.1", 0, Environment::Local).root_dir(BIN);

    web_server(server, settings)
        .into_router(Shutdown::manual())
        .unwrap_or_else(|error| panic!("{error}\n\nrun `make assets` first: this serves bin/"))
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

/// The first quoted URL in `html` that starts with `prefix`, made
/// root-absolute.
fn linked(html: &str, prefix: &str) -> String {
    let start: usize = html
        .find(prefix)
        .unwrap_or_else(|| panic!("`{prefix}` is linked:\n{html}"));
    let end: usize = start + html[start..].find('"').expect("a quoted attribute");
    format!("/{}", &html[start..end])
}

#[tokio::test]
async fn the_home_page_renders_with_a_stylesheet_that_resolves() {
    let router: Router = router();

    let response: Response = get(&router, "/").await;
    assert_eq!(StatusCode::OK, response.status());
    let html: String = text(response).await;
    assert!(html.contains("Project: Template Web Server"), "{html}");

    let stylesheet: String = linked(&html, "static/stylesheet/main.");
    let expected: StatusCode = StatusCode::OK;
    let actual: StatusCode = get(&router, &stylesheet).await.status();
    assert_eq!(expected, actual, "{stylesheet}");
}

#[tokio::test]
async fn an_unknown_path_renders_the_404_page_with_an_image_that_resolves() {
    // The smoke test's load-bearing check, without Docker: the 404 page's
    // content-hashed image proves the whole cache-buster pipeline survived the
    // build.
    let router: Router = router();

    let response: Response = get(&router, "/no-such-page").await;
    assert_eq!(StatusCode::NOT_FOUND, response.status());
    let html: String = text(response).await;

    let image: String = linked(&html, "static/image/page/404/stay-on-marked-trail.");
    let expected: StatusCode = StatusCode::OK;
    let actual: StatusCode = get(&router, &image).await.status();
    assert_eq!(expected, actual, "{image}");
}

#[tokio::test]
async fn every_route_the_layout_depends_on_is_served() {
    let router: Router = router();

    for path in [
        "/api/v1/health",
        "/robots.txt",
        "/humans.txt",
        "/site.webmanifest",
        "/sitemap.xml",
        "/favicon.ico",
    ] {
        let expected: StatusCode = StatusCode::OK;
        let actual: StatusCode = get(&router, path).await.status();
        assert_eq!(expected, actual, "{path}");
    }
}
