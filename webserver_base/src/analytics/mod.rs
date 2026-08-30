//! First-party proxies for Plausible and the Sentry browser SDK.
//!
//! Both vendors' own origins are on blocklists, and a blocked request is a
//! visitor you never counted or an error you never saw. Serving their scripts
//! and receiving their payloads from this origin makes the traffic
//! same-origin, which is what Plausible themselves recommend.
//!
//! Nothing here interprets a payload — see [`proxy`] for why that matters.

mod config;
mod ip;
mod proxy;

pub use config::{
    AnalyticsConfig, ENV_ANALYTICS_ID, ENV_SENTRY_BROWSER_DSN, PLAUSIBLE_HOST, SENTRY_CDN_HOST,
    SentryDsn, SentryDsnParseError,
};
pub use ip::{CLIENT_IP_HEADERS, resolve_true_client_ip_address};
pub use proxy::{relay_envelope, relay_event, relay_script};
