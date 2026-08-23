//! Coordinated graceful shutdown.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;
use tracing::{info, instrument, warn};

/// How long in-flight work gets once shutdown begins.
///
/// The bound is what makes graceful shutdown work. `with_graceful_shutdown`
/// waits for every connection, and a WebSocket never closes on its own — so
/// without a deadline a socket-holding server hangs until it is killed.
pub const DEFAULT_DRAIN_TIMEOUT: Duration = Duration::from_secs(10);

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
    use tokio::signal::unix::{SignalKind, signal};

    let mut terminate = signal(SignalKind::terminate()).expect("failed to install SIGTERM handler");
    let mut interrupt = signal(SignalKind::interrupt()).expect("failed to install SIGINT handler");
    let mut quit = signal(SignalKind::quit()).expect("failed to install SIGQUIT handler");

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
