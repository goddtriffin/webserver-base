//! Coordinated graceful shutdown.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tracing::{info, instrument, warn};

/// The longest in-flight work may take once shutdown begins.
///
/// A ceiling, never a wait: a server holding no connection exits the instant it
/// is signalled. The ceiling is what makes graceful shutdown terminate at all —
/// `with_graceful_shutdown` waits for every connection, and a WebSocket never
/// closes on its own, so without a deadline a socket-holding server hangs until
/// it is killed.
pub const DEFAULT_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

/// Application cleanup that must finish before the process exits.
///
/// [`WebServer::run`](super::WebServer::run) requires it of every state it
/// carries, so a project cannot add app state and quietly forget that it needs
/// draining. A server with no state gets the no-op implementation below and
/// writes nothing.
///
/// The library owns the sequencing. This runs the moment the shutdown signal
/// arrives, *concurrently* with the connection drain and under the same
/// [`DEFAULT_DRAIN_TIMEOUT`], so a slow drain never eats the cleanup's budget
/// and the two together cannot exceed one window. Overrunning is reported at
/// `error!` by the caller — do not try to bound this yourself for that reason.
///
/// Being cut off at the ceiling is ordinary cancellation: the future is
/// dropped at its last await point. Anything that must not be interrupted
/// mid-way needs to be atomic on its own, because no shutdown design can
/// cancel a synchronously blocking call.
///
/// # Idempotency
///
/// **This runs once per server holding the state, so it must be idempotent.** A
/// binary running several servers off one `Arc<AppState>` calls it once per
/// server. The library cannot deduplicate that — each [`WebServer`](super::WebServer)
/// builds its own [`WebServerState`](super::WebServerState), and only the
/// application knows what those share. Guard anything that would misbehave
/// twice behind a `OnceCell` in your own state.
///
/// ```no_run
/// use std::future::Future;
/// use webserver_base::webserver::AppShutdown;
///
/// struct AppState {
///     telegram: SomeNotifier,
/// }
/// # struct SomeNotifier;
/// # impl SomeNotifier {
/// #     async fn flush(&self, _: std::time::Duration) -> bool { true }
/// # }
///
/// impl AppShutdown for AppState {
///     async fn on_shutdown(&self) {
///         self.telegram.flush(std::time::Duration::from_secs(5)).await;
///     }
/// }
/// ```
pub trait AppShutdown {
    /// Drains whatever would otherwise be lost when the process exits.
    ///
    /// Returns `impl Future` rather than being an `async fn` because the
    /// server's own future must stay `Send`, and an `async fn` in a trait
    /// cannot promise that to its callers.
    fn on_shutdown(&self) -> impl Future<Output = ()> + Send;
}

/// A server carrying no application state has nothing to drain.
///
/// Deliberately the only blanket implementation: a project that adds state has
/// to say what draining means for it, and that is the whole point of the bound.
impl AppShutdown for () {
    async fn on_shutdown(&self) {}
}

/// A cloneable handle that resolves when the process should stop.
///
/// One listener, many holders — which is what lets a binary run several servers
/// and drain them together.
#[derive(Clone, Debug)]
pub struct Shutdown {
    sender: Arc<watch::Sender<bool>>,
    receiver: watch::Receiver<bool>,
}

impl Shutdown {
    /// A handle fired by hand, for tests and custom signal handling.
    #[must_use]
    pub fn manual() -> Self {
        let (sender, receiver) = watch::channel(false);
        Self {
            sender: Arc::new(sender),
            receiver,
        }
    }

    /// A handle wired to this platform's termination signals: `SIGTERM`,
    /// `SIGINT` and `SIGQUIT` on unix, ctrl-c elsewhere.
    ///
    /// The first signal starts the drain; a second exits immediately. Must be
    /// called from inside a Tokio runtime.
    #[must_use]
    #[instrument(skip_all)]
    pub fn listen() -> Self {
        let shutdown: Self = Self::manual();
        let trigger: Self = shutdown.clone();

        tokio::spawn(async move {
            wait_for_signal().await;
            info!("shutdown signal received; draining");
            trigger.trigger();

            wait_for_signal().await;
            warn!("second shutdown signal received; exiting immediately");
            std::process::exit(130);
        });

        shutdown
    }

    /// Starts the drain.
    pub fn trigger(&self) {
        // A send failure means every receiver is already gone.
        let _ = self.sender.send(true);
    }

    /// Whether the drain has started.
    #[must_use]
    pub fn is_shutting_down(&self) -> bool {
        *self.receiver.borrow()
    }

    /// Resolves when the drain starts, immediately if it already has.
    ///
    /// Consumes the handle so the future is `'static`; clone first if the
    /// handle is still needed.
    pub async fn recv(mut self) {
        if *self.receiver.borrow_and_update() {
            return;
        }
        let _ = self.receiver.changed().await;
    }
}

/// Waits for whichever termination signal arrives first.
#[cfg(unix)]
async fn wait_for_signal() {
    use tokio::signal::unix::{Signal, SignalKind, signal};

    let mut terminate: Signal =
        signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut interrupt: Signal =
        signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut quit: Signal = signal(SignalKind::quit()).expect("failed to install SIGQUIT handler");

    tokio::select! {
        _ = terminate.recv() => {}
        _ = interrupt.recv() => {}
        _ = quit.recv() => {}
    }
}

/// Waits for whichever termination signal arrives first.
#[cfg(not(unix))]
async fn wait_for_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use tokio::time::timeout;

    use super::Shutdown;

    #[tokio::test]
    async fn a_fresh_handle_is_not_shutting_down_and_does_not_resolve() {
        let shutdown: Shutdown = Shutdown::manual();

        let expected: bool = false;
        let actual: bool = shutdown.is_shutting_down();
        assert_eq!(expected, actual);

        let resolved: bool = timeout(Duration::from_millis(20), shutdown.clone().recv())
            .await
            .is_ok();
        assert!(!resolved, "recv resolved before anything triggered it");
    }

    #[tokio::test]
    async fn every_clone_hears_one_trigger() {
        let shutdown: Shutdown = Shutdown::manual();
        let first: Shutdown = shutdown.clone();
        let second: Shutdown = shutdown.clone();

        shutdown.trigger();

        timeout(Duration::from_millis(200), first.recv())
            .await
            .expect("the first clone resolved");
        timeout(Duration::from_millis(200), second.recv())
            .await
            .expect("the second clone resolved");
    }

    #[tokio::test]
    async fn recv_after_the_fact_resolves_immediately() {
        let shutdown: Shutdown = Shutdown::manual();
        shutdown.trigger();

        let expected: bool = true;
        let actual: bool = shutdown.is_shutting_down();
        assert_eq!(expected, actual);

        timeout(Duration::from_millis(50), shutdown.clone().recv())
            .await
            .expect("a handle created before the trigger still resolves after it");
    }

    #[tokio::test]
    async fn a_handle_cloned_after_the_trigger_still_resolves() {
        let shutdown: Shutdown = Shutdown::manual();
        shutdown.trigger();

        let late: Shutdown = shutdown.clone();
        timeout(Duration::from_millis(50), late.recv())
            .await
            .expect("a late clone sees the state, not just the transition");
    }
}
