//! The data a frontend's router-fallback 404 renders with.
//!
//! The 404 itself — template, status, URL, `noindex` — is the library's. Only
//! its `{{app}}` is the project's, and only for the fallback: a handler that
//! matched its route passes its own to
//! [`WebServerState::not_found`](super::WebServerState::not_found).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::http::Uri;
use serde::Serialize;
use serde_json::Value;
use tracing::error;

use super::state::WebServerState;

type Hook<S> =
    dyn Fn(WebServerState<S>, Uri) -> Pin<Box<dyn Future<Output = Value> + Send>> + Send + Sync;

/// What the router-fallback 404 shows as `{{app}}`.
///
/// `{{site}}` is already on it, so most sites need [`NotFound::none`]. Reach
/// for [`NotFound::with`] when the 404 should depend on what was asked for —
/// a "did you mean" link, the latest posts.
pub struct NotFound<S = ()> {
    hook: Option<Arc<Hook<S>>>,
}

impl<S> NotFound<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// `{{app}}` is `null`.
    #[must_use]
    pub const fn none() -> Self {
        Self { hook: None }
    }

    /// `{{app}}` is whatever `hook` returns for the requested URI.
    ///
    /// Runs on every unmatched request, bots' probes included, so keep it
    /// cheap. Data that fails to serialize is reported and rendered as `null`
    /// rather than failing the request.
    #[must_use]
    pub fn with<F, Fut, A>(hook: F) -> Self
    where
        F: Fn(WebServerState<S>, Uri) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = A> + Send + 'static,
        A: Serialize,
    {
        Self {
            hook: Some(Arc::new(move |state: WebServerState<S>, uri: Uri| {
                let data: Fut = hook(state, uri);
                Box::pin(async move { to_value_or_null(&data.await) })
            })),
        }
    }

    /// The `{{app}}` for a request to `uri`.
    pub(super) async fn data(&self, state: WebServerState<S>, uri: Uri) -> Value {
        match &self.hook {
            None => Value::Null,
            Some(hook) => hook(state, uri).await,
        }
    }
}

impl<S> Clone for NotFound<S> {
    fn clone(&self) -> Self {
        Self {
            hook: self.hook.clone(),
        }
    }
}

impl<S> Default for NotFound<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::none()
    }
}

fn to_value_or_null<A>(data: &A) -> Value
where
    A: Serialize,
{
    serde_json::to_value(data).unwrap_or_else(|serialize_error| {
        error!("the 404 hook's data is unserializable, rendering with null: {serialize_error}");
        Value::Null
    })
}
