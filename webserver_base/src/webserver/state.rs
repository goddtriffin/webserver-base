//! The one context every handler receives.

use std::sync::Arc;

use crate::environment::Environment;

use super::shutdown::Shutdown;

#[cfg(feature = "templates")]
use {
    super::error::WebServerError,
    crate::templates::{BaseTemplateData, PageTemplateData, TemplateData, TemplateRegistry},
    axum::response::Html,
    serde::Serialize,
    std::collections::BTreeMap,
    std::sync::LazyLock,
};

/// Stands in for the asset map when the `assets` kit is off.
#[cfg(feature = "templates")]
static NO_CACHE_BUSTER: LazyLock<BTreeMap<String, String>> = LazyLock::new(BTreeMap::new);

/// Everything a handler can reach: the server's configuration and kits, plus
/// the application's state as `S`.
///
/// One generic, not two — page data is passed to
/// [`render`](WebServerState::render) per call. Cloning is an `Arc` bump.
///
/// ```ignore
/// type Ctx = State<WebServerState<Arc<AppState>>>;
/// ```
#[derive(Debug)]
pub struct WebServerState<S = ()>(Arc<Inner<S>>);

impl<S> Clone for WebServerState<S> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

#[derive(Debug)]
struct Inner<S> {
    host: String,
    port: u16,
    environment: Environment,
    shutdown: Shutdown,

    #[cfg(feature = "templates")]
    base: Option<BaseTemplateData>,
    #[cfg(feature = "templates")]
    templates: Option<TemplateRegistry<'static>>,
    #[cfg(feature = "assets")]
    cache_buster: Option<crate::assets::CacheBuster>,
    #[cfg(feature = "analytics")]
    analytics: Option<Arc<crate::analytics::AnalyticsHandler>>,

    app: S,
}

/// The pieces [`WebServerState::new`] assembles.
pub(super) struct StateParts<S> {
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) environment: Environment,
    pub(super) shutdown: Shutdown,
    #[cfg(feature = "templates")]
    pub(super) base: Option<BaseTemplateData>,
    #[cfg(feature = "templates")]
    pub(super) templates: Option<TemplateRegistry<'static>>,
    #[cfg(feature = "assets")]
    pub(super) cache_buster: Option<crate::assets::CacheBuster>,
    #[cfg(feature = "analytics")]
    pub(super) analytics: Option<Arc<crate::analytics::AnalyticsHandler>>,
    pub(super) app: S,
}

impl<S> WebServerState<S> {
    pub(super) fn new(parts: StateParts<S>) -> Self {
        Self(Arc::new(Inner {
            host: parts.host,
            port: parts.port,
            environment: parts.environment,
            shutdown: parts.shutdown,
            #[cfg(feature = "templates")]
            base: parts.base,
            #[cfg(feature = "templates")]
            templates: parts.templates,
            #[cfg(feature = "assets")]
            cache_buster: parts.cache_buster,
            #[cfg(feature = "analytics")]
            analytics: parts.analytics,
            app: parts.app,
        }))
    }

    /// The host this server bound to.
    #[must_use]
    pub fn host(&self) -> &str {
        &self.0.host
    }

    /// The port this server bound to.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.0.port
    }

    /// Which deployment this is.
    #[must_use]
    pub fn environment(&self) -> Environment {
        self.0.environment
    }

    /// The shutdown handle. Clone it into a socket loop to close cleanly:
    ///
    /// ```ignore
    /// tokio::select! {
    ///     message = socket.recv() => { /* … */ }
    ///     () = state.shutdown().clone().recv() => {
    ///         socket.send(Message::Close(None)).await.ok();
    ///     }
    /// }
    /// ```
    #[must_use]
    pub fn shutdown(&self) -> &Shutdown {
        &self.0.shutdown
    }

    /// The application's own state.
    #[must_use]
    pub fn app(&self) -> &S {
        &self.0.app
    }

    /// The per-server template data, if `.templates(..)` was called.
    #[cfg(feature = "templates")]
    #[must_use]
    pub fn base_template_data(&self) -> Option<&BaseTemplateData> {
        self.0.base.as_ref()
    }

    /// The template registry, if `.templates(..)` was called.
    #[cfg(feature = "templates")]
    #[must_use]
    pub fn templates(&self) -> Option<&TemplateRegistry<'static>> {
        self.0.templates.as_ref()
    }

    /// The asset cache, if `.assets(..)` was called.
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn cache_buster(&self) -> Option<&crate::assets::CacheBuster> {
        self.0.cache_buster.as_ref()
    }

    /// The content-hashed path for an asset, or the original path when the
    /// `assets` kit is not in use.
    #[cfg(feature = "assets")]
    #[must_use]
    pub fn asset(&self, original_asset_file_path: &str) -> String {
        self.0.cache_buster.as_ref().map_or_else(
            || original_asset_file_path.to_string(),
            |cache_buster| cache_buster.get_file(original_asset_file_path),
        )
    }

    /// The analytics handler, if `.analytics(..)` was called.
    #[cfg(feature = "analytics")]
    #[must_use]
    pub fn analytics(&self) -> Option<&crate::analytics::AnalyticsHandler> {
        self.0.analytics.as_deref()
    }

    /// Renders `page` with `data` as its `{{app}}`. Pass `()` for none.
    ///
    /// # Errors
    ///
    /// [`WebServerError::TemplatesNotConfigured`] without `.templates(..)`, or
    /// [`WebServerError::Template`] if the render fails.
    #[cfg(feature = "templates")]
    pub fn render<A>(
        &self,
        page: &PageTemplateData,
        data: A,
    ) -> Result<Html<String>, WebServerError>
    where
        A: Serialize,
    {
        let (Some(base), Some(registry)) = (self.0.base.as_ref(), self.0.templates.as_ref()) else {
            return Err(WebServerError::TemplatesNotConfigured);
        };

        let template_data: TemplateData<'_, A> = TemplateData::assemble(
            base,
            page,
            self.0.environment,
            self.cache_buster_map(),
            data,
        )?;

        Ok(Html(registry.render(page.template(), &template_data)?))
    }

    /// The asset map, empty when the `assets` kit is off.
    #[cfg(feature = "templates")]
    fn cache_buster_map(&self) -> &BTreeMap<String, String> {
        #[cfg(feature = "assets")]
        {
            self.0
                .cache_buster
                .as_ref()
                .map_or(&NO_CACHE_BUSTER, crate::assets::CacheBuster::cache)
        }
        #[cfg(not(feature = "assets"))]
        {
            &NO_CACHE_BUSTER
        }
    }
}
