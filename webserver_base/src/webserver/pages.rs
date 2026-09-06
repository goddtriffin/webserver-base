//! Page declarations that produce both the route table and the sitemap.
//!
//! A page is declared once, so the two cannot drift, and leaving one out of the
//! sitemap is something you type ([`Pages::unlisted`]) rather than forget.
//!
//! Only pages live here — API routes, WebSocket upgrades and static file
//! services stay ordinary axum.

use std::sync::Arc;

use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::routing::{MethodRouter, get};
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use tracing::error;

use crate::sitemap::SitemapUrl;
use crate::templates::PageTemplateData;

use super::error::WebServerError;
use super::state::WebServerState;

/// Whether a page appears in `sitemap.xml`.
#[derive(Debug, Clone)]
enum Listing {
    Listed(Vec<SitemapUrl>),
    Unlisted,
}

/// How a page produces its response.
enum PageKind<S> {
    /// Pure declaration — no handler function exists.
    Static {
        page: Arc<PageTemplateData>,
        data: Arc<Value>,
    },
    /// An ordinary axum handler.
    Dynamic(Box<MethodRouter<WebServerState<S>>>),
}

struct PageEntry<S> {
    path: String,
    kind: PageKind<S>,
    listing: Listing,
}

/// The pages a site serves.
#[derive(Default)]
pub struct Pages<S = ()> {
    entries: Vec<PageEntry<S>>,
}

impl<S> Pages<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// An empty declaration.
    #[must_use]
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// A page with no logic: no handler function exists.
    ///
    /// Its route comes from the page's own
    /// [`page_url`](PageTemplateData::page_url), and `data` is serialized once
    /// at boot. Anything that varies per request needs
    /// [`Pages::dynamic_page`].
    ///
    /// # Panics
    ///
    /// If `data` cannot be serialized.
    #[must_use]
    pub fn static_page<A>(mut self, page: PageTemplateData, data: A) -> Self
    where
        A: Serialize,
    {
        let path: String = page.page_url().to_string();
        let value: Value = serde_json::to_value(data).unwrap_or_else(|error| {
            panic!("static page `{path}` has unserializable data: {error}")
        });

        self.entries.push(PageEntry {
            listing: Listing::Listed(vec![SitemapUrl::new(&path)]),
            path,
            kind: PageKind::Static {
                page: Arc::new(page),
                data: Arc::new(value),
            },
        });
        self
    }

    /// A page backed by an ordinary axum handler. For a parameterised path use
    /// [`Pages::dynamic_page_group`].
    #[must_use]
    pub fn dynamic_page(
        mut self,
        path: impl Into<String>,
        handler: MethodRouter<WebServerState<S>>,
    ) -> Self {
        let path: String = path.into();
        self.entries.push(PageEntry {
            listing: Listing::Listed(vec![SitemapUrl::new(&path)]),
            path,
            kind: PageKind::Dynamic(Box::new(handler)),
        });
        self
    }

    /// One handler serving many URLs, with those URLs supplied for the sitemap.
    ///
    /// The list is built where the route is declared, so adding a blog post
    /// cannot leave the sitemap behind.
    #[must_use]
    pub fn dynamic_page_group<I>(
        mut self,
        path: impl Into<String>,
        handler: MethodRouter<WebServerState<S>>,
        urls: I,
    ) -> Self
    where
        I: IntoIterator<Item = SitemapUrl>,
    {
        self.entries.push(PageEntry {
            path: path.into(),
            kind: PageKind::Dynamic(Box::new(handler)),
            listing: Listing::Listed(urls.into_iter().collect()),
        });
        self
    }

    /// Keeps the most recently declared page out of the sitemap.
    #[must_use]
    pub fn unlisted(mut self) -> Self {
        if let Some(entry) = self.entries.last_mut() {
            entry.listing = Listing::Unlisted;
        }
        self
    }

    /// Sets `<lastmod>` on the most recently declared page.
    #[must_use]
    pub fn with_last_modified(mut self, last_modified: DateTime<Utc>) -> Self {
        self.map_last_urls(|url| url.with_last_modified(last_modified));
        self
    }

    /// Adds `<image:image>` entries to the most recently declared page.
    #[must_use]
    pub fn extend_images<I, T>(mut self, images: I) -> Self
    where
        I: IntoIterator<Item = T>,
        T: Into<String>,
    {
        let images: Vec<String> = images.into_iter().map(Into::into).collect();
        self.map_last_urls(|url| url.extend_images(images.iter().cloned()));
        self
    }

    fn map_last_urls<F>(&mut self, mut transform: F)
    where
        F: FnMut(SitemapUrl) -> SitemapUrl,
    {
        if let Some(entry) = self.entries.last_mut()
            && let Listing::Listed(urls) = &mut entry.listing
        {
            for url in urls.iter_mut() {
                *url = transform(url.clone());
            }
        }
    }

    /// How many pages were declared.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing was declared.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every URL that belongs in the sitemap.
    ///
    /// # Errors
    ///
    /// [`WebServerError::DynamicPagePathHasParameters`] if a listed page's path
    /// contains a route parameter, which cannot be resolved to one URL.
    /// Every asset path the declared pages reference.
    ///
    /// Static pages only: a dynamic page builds its `PageTemplateData` per
    /// request, so there is nothing to inspect at boot.
    #[must_use]
    pub fn declared_assets(&self) -> Vec<String> {
        self.entries
            .iter()
            .filter_map(|entry| match &entry.kind {
                PageKind::Static { page, .. } => Some(page.declared_assets()),
                PageKind::Dynamic(_) => None,
            })
            .flatten()
            .collect()
    }

    /// Every listed page as a sitemap entry.
    ///
    /// # Errors
    ///
    /// [`WebServerError::DynamicPagePathHasParameters`] if a listed page's path
    /// contains a route parameter, which cannot become a single sitemap URL.
    pub fn sitemap_urls(&self) -> Result<Vec<SitemapUrl>, WebServerError> {
        let mut urls: Vec<SitemapUrl> = Vec::new();

        for entry in &self.entries {
            let Listing::Listed(entry_urls) = &entry.listing else {
                continue;
            };
            for url in entry_urls {
                if has_route_parameter(url.path()) {
                    return Err(WebServerError::DynamicPagePathHasParameters {
                        path: entry.path.clone(),
                    });
                }
                urls.push(url.clone());
            }
        }

        Ok(urls)
    }

    /// The 404 declaration, if one was made.
    /// Turns every declaration into routes.
    pub(super) fn into_router(self) -> axum::Router<WebServerState<S>> {
        let mut router: axum::Router<WebServerState<S>> = axum::Router::new();

        for entry in self.entries {
            let method_router: MethodRouter<WebServerState<S>> = match entry.kind {
                PageKind::Static { page, data } => static_page_handler(page, data),
                PageKind::Dynamic(handler) => *handler,
            };
            router = router.route(&entry.path, method_router);
        }

        router
    }
}

/// Builds the handler for a page that has no handler function.
fn static_page_handler<S>(
    page: Arc<PageTemplateData>,
    data: Arc<Value>,
) -> MethodRouter<WebServerState<S>>
where
    S: Clone + Send + Sync + 'static,
{
    get(move |State(state): State<WebServerState<S>>| {
        let page: Arc<PageTemplateData> = Arc::clone(&page);
        let data: Arc<Value> = Arc::clone(&data);
        async move { render_or_500(&state, &page, &data) }
    })
}

/// Renders a declared page, turning a failure into a 500.
pub(super) fn render_or_500<S>(
    state: &WebServerState<S>,
    page: &PageTemplateData,
    data: &Value,
) -> Response {
    match state.render(page, data) {
        Ok(html) => html.into_response(),
        Err(error) => {
            error!("failed to render `{}`: {error}", page.page_url());
            error.into_response()
        }
    }
}

/// Whether an axum path pattern matches more than one URL.
fn has_route_parameter(path: &str) -> bool {
    path.contains(':') || path.contains('*') || path.contains('{')
}

#[cfg(test)]
mod tests {

    use chrono::TimeZone;

    use crate::sitemap::SitemapUrl;
    use crate::templates::PageTemplateData;
    use crate::webserver::error::WebServerError;

    use super::{Pages, has_route_parameter};

    #[test]
    fn a_static_page_takes_its_route_from_its_page_url() {
        let pages: Pages = Pages::new()
            .static_page(PageTemplateData::new("home", "Home", "/"), ())
            .static_page(PageTemplateData::new("music", "Music", "/music"), ());

        let expected_len: usize = 2;
        let actual_len: usize = pages.len();
        assert_eq!(expected_len, actual_len);

        let expected_paths: Vec<String> = vec![String::from("/"), String::from("/music")];
        let actual_paths: Vec<String> = pages
            .sitemap_urls()
            .expect("no parameters")
            .iter()
            .map(|url| url.path().to_string())
            .collect();
        assert_eq!(expected_paths, actual_paths);
    }

    #[test]
    fn unlisted_keeps_a_page_routed_but_out_of_the_sitemap() {
        let pages: Pages = Pages::new()
            .static_page(PageTemplateData::new("home", "Home", "/"), ())
            .static_page(PageTemplateData::new("secret", "Secret", "/secret"), ())
            .unlisted();

        let expected_routes: usize = 2;
        let actual_routes: usize = pages.len();
        assert_eq!(expected_routes, actual_routes);

        let expected_listed: Vec<String> = vec![String::from("/")];
        let actual_listed: Vec<String> = pages
            .sitemap_urls()
            .expect("no parameters")
            .iter()
            .map(|url| url.path().to_string())
            .collect();
        assert_eq!(expected_listed, actual_listed);
    }

    #[test]
    fn a_group_lists_the_urls_it_was_given_not_its_pattern() {
        let pages: Pages = Pages::new().dynamic_page_group(
            "/blog/{slug}",
            axum::routing::get(|| async { "post" }),
            [
                SitemapUrl::new("/blog/first"),
                SitemapUrl::new("/blog/second"),
            ],
        );

        let expected: Vec<String> = vec![String::from("/blog/first"), String::from("/blog/second")];
        let actual: Vec<String> = pages
            .sitemap_urls()
            .expect("concrete urls")
            .iter()
            .map(|url| url.path().to_string())
            .collect();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_parameterised_path_cannot_be_declared_as_a_single_page() {
        let pages: Pages =
            Pages::new().dynamic_page("/blog/{slug}", axum::routing::get(|| async { "post" }));

        let error: WebServerError = pages
            .sitemap_urls()
            .expect_err("one pattern is not one url");
        assert!(matches!(
            error,
            WebServerError::DynamicPagePathHasParameters { ref path } if path == "/blog/{slug}"
        ));
    }

    #[test]
    fn a_parameterised_path_is_fine_once_it_is_unlisted() {
        let pages: Pages = Pages::new()
            .dynamic_page(
                "/og/{version}/{file}",
                axum::routing::get(|| async { "card" }),
            )
            .unlisted();

        let expected: usize = 0;
        let actual: usize = pages.sitemap_urls().expect("unlisted").len();
        assert_eq!(expected, actual);
    }

    #[test]
    fn sitemap_modifiers_apply_to_the_page_just_declared() {
        let modified: chrono::DateTime<chrono::Utc> = chrono::Utc
            .with_ymd_and_hms(2026, 3, 4, 5, 6, 7)
            .single()
            .expect("a real instant");

        let pages: Pages = Pages::new()
            .static_page(PageTemplateData::new("home", "Home", "/"), ())
            .static_page(PageTemplateData::new("blog", "Blog", "/blog"), ())
            .with_last_modified(modified);

        let urls: Vec<SitemapUrl> = pages.sitemap_urls().expect("no parameters");

        assert_eq!(None, urls[0].last_modified());
        assert_eq!(Some(modified), urls[1].last_modified());

        let expected_path: String = String::from("/blog");
        let actual_path: String = urls[1].path().to_string();
        assert_eq!(expected_path, actual_path);
    }

    #[test]
    fn route_parameters_are_recognised_in_every_axum_spelling() {
        assert!(has_route_parameter("/blog/{slug}"));
        assert!(has_route_parameter("/blog/{slug}"));
        assert!(has_route_parameter("/files/{*path}"));
        assert!(!has_route_parameter("/blog"));
        assert!(!has_route_parameter("/"));
    }

    #[test]
    fn declarations_actually_build_a_router() {
        // Route syntax is validated when axum registers the path, not when this
        // module compiles — so the router has to actually be built here.
        let pages: Pages = Pages::new()
            .static_page(PageTemplateData::new("home", "Home", "/"), ())
            .dynamic_page("/blog", axum::routing::get(|| async { "index" }))
            .dynamic_page_group(
                "/blog/{slug}",
                axum::routing::get(|| async { "post" }),
                [SitemapUrl::new("/blog/first")],
            )
            .unlisted();

        let _router: axum::Router<crate::webserver::WebServerState> = pages.into_router();
    }

    #[test]
    fn a_declaration_starts_empty() {
        let pages: Pages = Pages::new();

        let expected: bool = true;
        let actual: bool = pages.is_empty();
        assert_eq!(expected, actual);
    }

    #[test]
    fn declared_assets_are_collected_from_static_pages_for_boot_validation() {
        let pages: Pages = Pages::new().static_page(
            PageTemplateData::new("blog", "Blog", "/blog")
                .extend_style_sheets(["static/stylesheet/blog.css"])
                .with_social_image("static/image/social/blog.webp"),
            (),
        );

        let declared: Vec<String> = pages.declared_assets();
        assert!(declared.contains(&String::from("static/stylesheet/blog.css")));
        assert!(declared.contains(&String::from("static/image/social/blog.webp")));
    }
}
