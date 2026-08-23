//! Shared logic for Todd Everett Griffin's web servers, as a set of
//! independently feature-gated **kits**.
//!
//! Every kit works on its own — a bot that wants only Telegram notifications
//! enables `telegram` and compiles neither axum nor Handlebars. Together they
//! compose into one opinionated web server. Nothing is enabled by default.
//!
//! | feature | what it is |
//! |---|---|
//! | `assets` | content-hashed static files, cache-control middleware |
//! | `templates` | Handlebars registry and the three template-data types |
//! | `analytics` | Plausible event forwarding with true-client-IP resolution |
//! | `sitemap` | `sitemap.xml` generation |
//! | `observability` | Sentry + tracing, initialised in the one order that works |
//! | `telegram` | outbound Telegram Bot API notifier |
//! | `webserver` | the server builder, shared state, bootstrap, graceful shutdown |
//! | `pages` | page declarations that produce routes *and* sitemap entries |
//! | `preset` | Todd Everett Griffin's personal defaults |
//! | `full` | all of the above |
//!
//! # Configuration
//!
//! Every kit is constructed manually first; `from_env` is a convenience layered
//! on top. All environment variables this crate reads are prefixed `WSB_` so
//! they cannot collide with a project's own.

pub mod env;
mod environment;

pub use environment::{ENV_ENVIRONMENT, Environment, EnvironmentParseError};

#[cfg(feature = "analytics")]
pub mod analytics;
#[cfg(feature = "assets")]
pub mod assets;
#[cfg(feature = "observability")]
pub mod observability;
#[cfg(feature = "sitemap")]
pub mod sitemap;
#[cfg(feature = "telegram")]
pub mod telegram;
#[cfg(feature = "templates")]
pub mod templates;
#[cfg(feature = "webserver")]
pub mod webserver;

#[cfg(feature = "webserver")]
pub use webserver::{Shutdown, WebServer, WebServerError, WebServerState, bootstrap};
