//! Client for Todd Griffin's Projects accounts (`accounts.toddgriffin.me`).
//!
//! What a project's backend needs to use the shared accounts, and nothing
//! else:
//!
//! - [`login`]: the `OpenID` Connect sign-in round trip (authorization code +
//!   PKCE), as two calls around a redirect.
//! - [`Accounts`]: token exchange, refresh, revocation, and checking ID and
//!   access tokens against the published signing keys.
//! - [`webhook`]: verifying the signed webhooks the accounts service sends.
//! - [`friends`]: the friends, events and inbox API.
//! - [`return_to`]: keeping post-sign-in redirects on your own site.
//!
//! It holds no secrets of its own and never logs tokens. Your app's client
//! secret and webhook secret come from your environment.
//!
//! ```no_run
//! # async fn example() -> Result<(), toddstack_accounts::Error> {
//! use toddstack_accounts::{Accounts, Config, login};
//!
//! let accounts = Accounts::new(Config::toddgriffin(
//!     "boggledygook",
//!     Some(String::from("client secret from 1Password")),
//!     "https://www.boggledygook.com/auth/callback",
//! ))?;
//!
//! // GET /auth/login: send the person to sign in; keep `pending` in your session.
//! let (url, pending) = login::start(accounts.config(), &["openid", "profile", "offline_access"], Some("/play"));
//!
//! // GET /auth/callback?code=..&state=..: finish, then start your own session.
//! # let query = "";
//! let signed_in = accounts.finish_login(&pending, query).await?;
//! println!("{} signed in", signed_in.claims.sub);
//! # Ok(()) }
//! ```

mod error;
pub mod friends;
mod jwks;
pub mod login;
pub mod return_to;
mod tokens;
pub mod webhook;

pub use error::Error;
pub use login::PendingLogin;
pub use tokens::{AccessClaims, Accounts, Config, IdClaims, SignedIn, TokenSet};

/// The production issuer. Permanent: every app and passkey depends on it.
pub const ISSUER: &str = "https://accounts.toddgriffin.me/oidc";

pub type Result<T, E = Error> = std::result::Result<T, E>;
