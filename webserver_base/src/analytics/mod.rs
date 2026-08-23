//! Plausible Analytics event forwarding.
//!
//! The browser posts to this server, which forwards server-side, so the client
//! never talks to an analytics domain directly. Events are dropped in
//! [`Environment::Local`](crate::Environment::Local).

mod handler;
mod ip;

pub use handler::{AnalyticsConfig, AnalyticsHandler, AnalyticsRequest, ENV_ANALYTICS_DOMAIN};
pub use ip::{CLIENT_IP_HEADERS, resolve_true_client_ip_address};
