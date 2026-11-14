//! Verifying webhooks from the accounts service.
//!
//! Each delivery is a POST with JSON and `X-Accounts-Signature: t=<unix
//! seconds>,v1=<hex HMAC-SHA256 of "<t>.<raw body>">` under your app's
//! webhook secret. Verify against the raw body bytes, before parsing.
//! Deliveries can repeat (retries); use the event `id` to ignore duplicates.

use hmac::{Hmac, KeyInit, Mac};
use serde::Deserialize;
use serde_json::Value;
use sha2::Sha256;
use subtle::ConstantTimeEq;

use crate::{Error, Result};

/// The header name.
pub const HEADER: &str = "X-Accounts-Signature";

/// How old a signature may be.
pub const TOLERANCE_SECONDS: i64 = 300;

/// A verified event.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Event {
    /// Unique per event; repeated on retries.
    pub id: String,
    /// `user.updated`, `user.deleted`, `user.suspended`, `user.reactivated`,
    /// `user.signed_out`, or `grant.revoked`.
    #[serde(rename = "type")]
    pub kind: String,
    pub created_at: i64,
    /// Always has `sub`; `user.updated` adds the fields that changed.
    pub data: Value,
}

impl Event {
    /// The person the event is about.
    #[must_use]
    pub fn sub(&self) -> Option<&str> {
        self.data.get("sub").and_then(Value::as_str)
    }
}

/// Checks the signature and age, then parses the event. `now` is the current
/// Unix time (a parameter so tests can fix it).
///
/// # Errors
/// [`Error::InvalidWebhook`] for a missing or malformed header, a wrong
/// signature, an old or future timestamp, or a body that isn't an event.
pub fn verify(secret: &[u8], signature_header: &str, body: &[u8], now: i64) -> Result<Event> {
    let mut t: Option<i64> = None;
    let mut v1: Option<Vec<u8>> = None;
    for part in signature_header.split(',') {
        match part.trim().split_once('=') {
            Some(("t", value)) => t = value.parse().ok(),
            Some(("v1", value)) => v1 = hex::decode(value).ok(),
            _ => {}
        }
    }
    let (Some(t), Some(v1)) = (t, v1) else {
        return Err(Error::InvalidWebhook("malformed signature header"));
    };
    if (now - t).abs() > TOLERANCE_SECONDS {
        return Err(Error::InvalidWebhook(
            "signature too old (or from the future)",
        ));
    }
    let mut mac: Hmac<Sha256> = <Hmac<Sha256> as KeyInit>::new_from_slice(secret)
        .map_err(|_| Error::InvalidWebhook("bad secret"))?;
    mac.update(t.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    let expected = mac.finalize().into_bytes();
    if !bool::from(expected.as_slice().ct_eq(&v1)) {
        return Err(Error::InvalidWebhook("signature mismatch"));
    }
    serde_json::from_slice(body).map_err(|_| Error::InvalidWebhook("body is not an event"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sign(secret: &[u8], t: i64, body: &str) -> String {
        let mut mac: Hmac<Sha256> = <Hmac<Sha256> as KeyInit>::new_from_slice(secret).unwrap();
        mac.update(format!("{t}.{body}").as_bytes());
        format!("t={t},v1={}", hex::encode(mac.finalize().into_bytes()))
    }

    const BODY: &str =
        r#"{"id":"e1","type":"user.deleted","created_at":1700000000,"data":{"sub":"u1"}}"#;

    #[test]
    fn valid_webhooks_verify() {
        let event: Event = verify(
            b"secret",
            &sign(b"secret", 1_700_000_000, BODY),
            BODY.as_bytes(),
            1_700_000_010,
        )
        .unwrap();
        assert_eq!(event.kind, "user.deleted");
        assert_eq!(event.sub(), Some("u1"));
    }

    #[test]
    fn tampering_wrong_secret_and_age_are_refused() {
        let header: String = sign(b"secret", 1_700_000_000, BODY);
        let tampered: String = BODY.replace("u1", "u2");
        assert!(verify(b"secret", &header, tampered.as_bytes(), 1_700_000_000).is_err());
        assert!(verify(b"other", &header, BODY.as_bytes(), 1_700_000_000).is_err());
        assert!(verify(b"secret", &header, BODY.as_bytes(), 1_700_000_301).is_err());
        assert!(verify(b"secret", "nonsense", BODY.as_bytes(), 1_700_000_000).is_err());
    }
}
