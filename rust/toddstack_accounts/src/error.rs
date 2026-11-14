/// Everything that can go wrong talking to the accounts service.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The callback's `state` doesn't match the pending login (a forged or
    /// stale callback: start over).
    #[error("the sign-in callback doesn't match this browser's pending sign-in")]
    StateMismatch,
    /// The accounts service sent the person back with an OAuth error
    /// (`access_denied` when they cancelled, `login_required` for `prompt=none`).
    #[error("sign-in failed: {error}")]
    Authorization {
        error: String,
        description: Option<String>,
    },
    /// The callback had no `code`.
    #[error("the callback has no code")]
    MissingCode,
    /// The token endpoint refused (`invalid_grant`: expired, reused or revoked).
    #[error("token request refused: {error}")]
    TokenRefused {
        error: String,
        description: Option<String>,
    },
    /// A token failed verification (signature, issuer, audience, expiry, nonce).
    #[error("token rejected: {0}")]
    InvalidToken(String),
    /// A webhook failed verification.
    #[error("webhook rejected: {0}")]
    InvalidWebhook(&'static str),
    /// The API refused a call (status and its `error` message).
    #[error("accounts API returned {status}: {message}")]
    Api { status: u16, message: String },
    #[error("http: {0}")]
    Http(#[from] reqwest::Error),
    #[error("configuration: {0}")]
    Config(String),
}
