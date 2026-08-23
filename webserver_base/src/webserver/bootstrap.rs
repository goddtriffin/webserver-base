//! Process start-up, in the one order that works.

use std::future::Future;

use super::shutdown::Shutdown;

/// Runs `body` with observability, a runtime, and signal handling set up.
///
/// The ordering is the point: observability on the main thread first (so the
/// Sentry hub reaches the runtime's workers), then the runtime, then one signal
/// listener whose [`Shutdown`] handle is passed to `body`, then the guard drops
/// after `body` returns — flushing errors raised during the shutdown itself.
///
/// The application still owns `main`, so it can start as many servers as it
/// likes.
///
/// ```no_run
/// use webserver_base::{Environment, WebServer, bootstrap};
/// use webserver_base::observability::Observability;
///
/// fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let environment = Environment::from_env()?;
///     bootstrap(Observability::from_env(environment)?, |shutdown| async move {
///         WebServer::from_env(environment)?
///             .health()
///             .run(shutdown)
///             .await?;
///         Ok(())
///     })
/// }
/// ```
///
/// # Errors
///
/// Whatever `body` returns. `bootstrap` adds no failure of its own.
///
/// # Panics
///
/// If the Tokio runtime cannot be built. There is no useful way to continue,
/// and no server to report it to yet.
pub fn bootstrap<F, Fut, T, E>(
    #[cfg(feature = "observability")] observability: crate::observability::Observability,
    #[cfg(not(feature = "observability"))] observability: (),
    body: F,
) -> Result<T, E>
where
    F: FnOnce(Shutdown) -> Fut,
    Fut: Future<Output = Result<T, E>>,
{
    #[cfg(feature = "observability")]
    let _guard = observability.init();
    #[cfg(not(feature = "observability"))]
    let () = observability;

    let runtime: tokio::runtime::Runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("failed to build the tokio runtime");

    runtime.block_on(async move {
        let shutdown: Shutdown = Shutdown::listen();
        body(shutdown).await
    })
    // `_guard` drops here, after the drain.
}
