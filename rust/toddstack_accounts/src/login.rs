//! The sign-in round trip, as two steps around a redirect.
//!
//! 1. [`start`] builds the URL to send the person to and a [`PendingLogin`].
//!    Store the pending login server-side (or in a signed, `HttpOnly` cookie):
//!    it holds the PKCE verifier, which must never reach the browser's JS.
//! 2. When the person comes back to your redirect URI, pass the pending login
//!    and the callback query to [`crate::Accounts::finish_login`].

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::tokens::Config;

/// What to remember between [`start`] and the callback.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingLogin {
    pub state: String,
    pub nonce: String,
    pub verifier: String,
    /// Where to go after sign-in: a path on your site, already checked.
    pub return_to: Option<String>,
}

/// 32 random bytes, base64url.
pub(crate) fn random_token() -> String {
    let mut bytes: [u8; 32] = [0; 32];
    getrandom::fill(&mut bytes).expect("the OS randomness source failed");
    URL_SAFE_NO_PAD.encode(bytes)
}

/// The PKCE S256 challenge for `verifier`.
#[must_use]
pub fn challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// Options beyond the defaults.
#[derive(Clone, Debug, Default)]
pub struct Options<'a> {
    /// `login` forces a fresh sign-in; `none` checks silently (no page shown);
    /// `create` opens the sign-up page.
    pub prompt: Option<&'a str>,
    /// Prefills the email field.
    pub login_hint: Option<&'a str>,
    /// Require a sign-in within this many seconds.
    pub max_age: Option<u64>,
}

/// Starts a sign-in. `return_to` is checked with [`crate::return_to::safe`];
/// anything off-site is dropped.
///
/// # Panics
/// If the OS randomness source fails.
#[must_use]
pub fn start(config: &Config, scopes: &[&str], return_to: Option<&str>) -> (String, PendingLogin) {
    start_with(config, scopes, return_to, &Options::default())
}

/// [`start`] with [`Options`].
///
/// # Panics
/// If the OS randomness source fails.
#[must_use]
pub fn start_with(
    config: &Config,
    scopes: &[&str],
    return_to: Option<&str>,
    options: &Options<'_>,
) -> (String, PendingLogin) {
    let pending: PendingLogin = PendingLogin {
        state: random_token(),
        nonce: random_token(),
        verifier: random_token(),
        return_to: return_to.and_then(crate::return_to::safe),
    };
    let mut scopes: Vec<&str> = scopes.to_vec();
    if !scopes.contains(&"openid") {
        scopes.insert(0, "openid");
    }
    let mut url: url::Url =
        url::Url::parse(&format!("{}/auth", config.issuer)).expect("the issuer is a URL");
    {
        let mut q = url.query_pairs_mut();
        q.append_pair("client_id", &config.client_id);
        q.append_pair("redirect_uri", &config.redirect_uri);
        q.append_pair("response_type", "code");
        q.append_pair("scope", &scopes.join(" "));
        q.append_pair("state", &pending.state);
        q.append_pair("nonce", &pending.nonce);
        q.append_pair("code_challenge", &challenge(&pending.verifier));
        q.append_pair("code_challenge_method", "S256");
        if let Some(prompt) = options.prompt {
            q.append_pair("prompt", prompt);
        }
        if let Some(hint) = options.login_hint {
            q.append_pair("login_hint", hint);
        }
        if let Some(age) = options.max_age {
            q.append_pair("max_age", &age.to_string());
        }
    }
    (url.into(), pending)
}

/// What the callback query says.
pub(crate) struct Callback {
    pub code: Option<String>,
    pub state: Option<String>,
    pub iss: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

pub(crate) fn parse_callback(query: &str) -> Callback {
    let mut callback: Callback = Callback {
        code: None,
        state: None,
        iss: None,
        error: None,
        error_description: None,
    };
    for (key, value) in url::form_urlencoded::parse(query.trim_start_matches('?').as_bytes()) {
        let value: String = value.into_owned();
        match key.as_ref() {
            "code" => callback.code = Some(value),
            "state" => callback.state = Some(value),
            "iss" => callback.iss = Some(value),
            "error" => callback.error = Some(value),
            "error_description" => callback.error_description = Some(value),
            _ => {}
        }
    }
    callback
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> Config {
        Config::toddgriffin(
            "boggledygook",
            None,
            "https://www.boggledygook.com/auth/callback",
        )
    }

    #[test]
    fn start_builds_a_pkce_request() {
        let (url, pending) = start(&config(), &["profile"], Some("/play"));
        let parsed: url::Url = url::Url::parse(&url).unwrap();
        assert_eq!(
            parsed.as_str().split('?').next(),
            Some("https://accounts.toddgriffin.me/oidc/auth")
        );
        let q: std::collections::HashMap<String, String> =
            parsed.query_pairs().into_owned().collect();
        assert_eq!(q["scope"], "openid profile");
        assert_eq!(q["code_challenge"], challenge(&pending.verifier));
        assert_eq!(q["code_challenge_method"], "S256");
        assert_eq!(q["state"], pending.state);
        assert_eq!(q["nonce"], pending.nonce);
        assert_eq!(pending.return_to.as_deref(), Some("/play"));
        assert_ne!(pending.state, pending.nonce);
    }

    #[test]
    fn off_site_return_paths_are_dropped() {
        let (_, pending) = start(&config(), &[], Some("https://evil.example"));
        assert_eq!(pending.return_to, None);
    }

    #[test]
    fn challenge_is_base64url_sha256() {
        // base64url(sha256("abc")), computed independently.
        assert_eq!(
            challenge("abc"),
            "ungWv48Bz-pBQUDeXa4iI7ADYaOWF3qctBD_YfIAFa0"
        );
    }

    #[test]
    fn callbacks_parse() {
        let c: Callback = parse_callback("?code=c1&state=s1&iss=https%3A%2F%2Fa");
        assert_eq!(c.code.as_deref(), Some("c1"));
        assert_eq!(c.iss.as_deref(), Some("https://a"));
        let e: Callback = parse_callback("error=access_denied&state=s1");
        assert_eq!(e.error.as_deref(), Some("access_denied"));
    }
}
