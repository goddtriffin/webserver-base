//! The template site, as a library: `main` and the tests build the one
//! [`WebServer`] this returns, so what the tests drive is what production
//! serves.

use webserver_base::analytics::{AnalyticsConfig, ENV_SENTRY_BROWSER_DSN};
use webserver_base::env;
use webserver_base::templates::{
    BaseTemplateData, Fallback, GoddtriffinParams, PageTemplateData, ThemeColor,
};
use webserver_base::webserver::{FrontendParams, Pages, WebServer, WebServerError};

/// Everything this site reads from the environment.
///
/// Plain data, so a test builds it directly rather than mutating process-wide
/// environment variables it would share with every other test.
pub struct Settings {
    /// The site's name.
    pub project: String,
    /// The default description.
    pub description: String,
    /// The site's origin, without a trailing slash.
    pub base_url: String,
    /// Which Plausible site this reports to.
    pub analytics: AnalyticsConfig,
    /// The browser Sentry DSN.
    pub sentry_browser_dsn: String,
}

impl Settings {
    /// Reads `TWS_PROJECT`, `TWS_DESCRIPTION`, `TWS_BASE_URL` and the two
    /// frontend variables the library names.
    ///
    /// # Errors
    ///
    /// [`WebServerError::Env`] if any of them is unset or blank.
    pub fn from_env() -> Result<Self, WebServerError> {
        Ok(Self {
            project: env::required("TWS_PROJECT")?,
            description: env::required("TWS_DESCRIPTION")?,
            base_url: env::required("TWS_BASE_URL")?,
            analytics: AnalyticsConfig::from_env()?,
            sentry_browser_dsn: env::required(ENV_SENTRY_BROWSER_DSN)?,
        })
    }
}

/// This site, on `server`.
///
/// Takes the server rather than building one, so `main` can pass
/// [`WebServer::from_env`] and a test can pass one with its own
/// [`root_dir`](WebServer::root_dir).
#[must_use]
pub fn web_server(server: WebServer, settings: Settings) -> WebServer {
    server.frontend(FrontendParams {
        base: BaseTemplateData::goddtriffin(GoddtriffinParams {
            project: settings.project,
            description: settings.description,
            base_url: settings.base_url,
            social_image: String::from("static/image/social/todo.webp"),
            theme_color: ThemeColor::light_dark("#fafafa", "#121212"),
            theme_fallback: Fallback::Dark,
            copyright_start: String::from("1998"),
            style_sheets: vec![String::from("static/stylesheet/main.css")],
            scripts: vec![],
        }),
        pages: Pages::new()
            .static_page(PageTemplateData::new("home", "Home", "/"), ())
            .extend_images(["static/image/social/todo.webp"]),
        analytics: settings.analytics,
        sentry_browser_dsn: settings.sentry_browser_dsn,
    })
}
