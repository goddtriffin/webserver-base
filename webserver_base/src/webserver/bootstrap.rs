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
/// It also owns the build-tool subcommands, so no project has to declare a
/// second binary or write a line of glue to reach them:
///
/// ```text
/// $ my-server gen-static-assets    # icons, then hash everything but scripts
/// $ my-server gen-static-scripts   # hash the built JavaScript
/// $ my-server                      # serve
/// ```
///
/// Those two exit before any runtime or error monitoring starts. They still
/// read `WSB_ENVIRONMENT`, because `main` resolves it before calling in — set
/// it to `local` in the build stage.
///
/// ```no_run
/// use webserver_base::{WebServer, WebServerError, bootstrap};
///
/// fn main() -> Result<(), WebServerError> {
///     bootstrap!(|shutdown| async move { WebServer::from_env()?.run(shutdown).await })
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
pub fn bootstrap_with_release<F, Fut, T, E>(release: &str, body: F) -> Result<T, E>
where
    F: FnOnce(Shutdown) -> Fut,
    Fut: Future<Output = Result<T, E>>,
    E: From<super::error::WebServerError>,
{
    // Build-tool modes run and exit before the runtime, before error
    // monitoring, and before anything binds a port. Handling them here is what
    // lets every project reach the static-asset pipeline through its own server
    // binary, with no shim binary and no duplicated glue.
    if let Some(phase) = std::env::args()
        .nth(1)
        .as_deref()
        .and_then(crate::assets::Phase::from_subcommand)
    {
        match crate::assets::generate_static_assets(phase) {
            Ok(()) => std::process::exit(0),
            Err(error) => {
                eprintln!("{error}");
                std::process::exit(1);
            }
        }
    }

    // Resolved here rather than in every `main`: it is the same three lines in
    // every project, and one of them is easy to get in the wrong order.
    #[cfg(feature = "observability")]
    let _guard = {
        let environment: crate::Environment = crate::Environment::from_env()
            .map_err(|error| E::from(super::error::WebServerError::from(error)))?;
        crate::observability::Observability::from_env(environment)
            .map_err(|error| E::from(super::error::WebServerError::from(error)))?
            .with_release(release)
            .init()
    };

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

/// Starts a server, naming the running build from the *calling* crate.
///
/// A macro rather than a function because the release string has to come from
/// the application's own `CARGO_PKG_NAME` and `CARGO_PKG_VERSION`, and those are
/// resolved where the code is written. Called from inside this library — as
/// `sentry::release_name!()` does — every project would report the same
/// release, and Sentry could not tell one site's deploys from another's.
///
/// ```no_run
/// use webserver_base::{WebServer, WebServerError, bootstrap};
///
/// fn main() -> Result<(), WebServerError> {
///     bootstrap!(|shutdown| async move { WebServer::from_env()?.run(shutdown).await })
/// }
/// ```
#[macro_export]
macro_rules! bootstrap {
    ($body:expr) => {
        $crate::webserver::bootstrap_with_release(
            concat!(env!("CARGO_PKG_NAME"), "@", env!("CARGO_PKG_VERSION")),
            $body,
        )
    };
}
