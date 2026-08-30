//! The web server: builder, shared state, bootstrap, graceful shutdown.
//!
//! The application owns `main`. [`bootstrap`] sets up the things that are
//! process-global and easy to order wrongly — error monitoring, the runtime,
//! signal handling — and hands back a [`Shutdown`] handle. Everything after
//! that is ordinary code, so one binary can run several [`WebServer`]s on
//! different ports and drain them together.

mod bootstrap;
mod error;
#[cfg(feature = "pages")]
mod frontend;
#[cfg(feature = "pages")]
mod pages;
mod server;
mod shutdown;
mod state;

pub use bootstrap::bootstrap_with_release;
pub use error::WebServerError;
#[cfg(feature = "pages")]
pub use frontend::{Frontend, FrontendParams, WellKnown};
pub use server::{API_PREFIX, DEFAULT_BODY_LIMIT, DEFAULT_PORT, ENV_HOST, ENV_PORT, WebServer};
pub use shutdown::{DEFAULT_DRAIN_TIMEOUT, Shutdown};
pub use state::WebServerState;

#[cfg(feature = "pages")]
pub use pages::Pages;
