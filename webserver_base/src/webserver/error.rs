//! The web server's error type.

use std::net::SocketAddr;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use tracing::error;

use crate::env::EnvError;

/// Every way the web server can fail.
///
/// Composed from the per-kit errors rather than replacing them: a crate on only
/// `telegram` sees [`TelegramError`](crate::telegram::TelegramError) and never
/// this type. Deliberately not `#[non_exhaustive]`.
#[derive(Debug, thiserror::Error)]
pub enum WebServerError {
    /// A required environment variable was missing or malformed.
    #[error(transparent)]
    Env(#[from] EnvError),

    /// The listener could not bind.
    #[error("failed to bind to {addr}")]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },

    /// The server stopped with an error rather than a shutdown.
    #[error("the server stopped unexpectedly")]
    Serve(#[source] std::io::Error),

    /// A template could not be loaded or rendered.
    #[cfg(feature = "templates")]
    #[error(transparent)]
    Template(#[from] crate::templates::TemplateError),

    /// A render was attempted without `.templates(..)` having been called.
    #[cfg(feature = "templates")]
    #[error("cannot render: the server was built without `.templates(..)`")]
    TemplatesNotConfigured,

    /// The asset cache could not be built.
    #[cfg(feature = "assets")]
    #[error(transparent)]
    CacheBuster(#[from] crate::assets::CacheBusterError),

    /// The sitemap could not be written.
    #[cfg(feature = "sitemap")]
    #[error(transparent)]
    Sitemap(#[from] crate::sitemap::SitemapError),

    /// Observability could not be configured.
    #[cfg(feature = "observability")]
    #[error(transparent)]
    Observability(#[from] crate::observability::ObservabilityError),

    /// Pages were declared without the template data needed to render or list
    /// them.
    #[cfg(feature = "pages")]
    #[error("`.pages(..)` requires `.templates(..)`: the sitemap needs the site's base url")]
    PagesRequireTemplates,

    /// A parameterised path was declared as a single page. `/blog/{slug}`
    /// matches many URLs, so it cannot be one sitemap entry.
    #[cfg(feature = "pages")]
    #[error(
        "page path `{path}` contains a route parameter; \
         use `dynamic_page_group` with concrete urls, or mark it unlisted"
    )]
    DynamicPagePathHasParameters { path: String },
}

impl IntoResponse for WebServerError {
    /// The body says nothing beyond "internal error"; the detail goes to the log.
    fn into_response(self) -> Response {
        error!("request failed: {self}");
        (StatusCode::INTERNAL_SERVER_ERROR, "internal server error").into_response()
    }
}
