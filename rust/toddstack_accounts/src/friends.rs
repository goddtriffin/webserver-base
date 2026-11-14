//! The friends, events and inbox API, called with a person's access token
//! (scopes `friends`, and `friends:events` to post events).
//!
//! Your app only ever sees friends who also use your app.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::tokens::Accounts;
use crate::{Error, Result};

/// A friend, request or blocked person as your app sees them.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Person {
    pub sub: String,
    pub username: String,
    pub name: Option<String>,
    pub picture: Option<String>,
    /// When the friendship, request or block started (RFC 3339).
    pub since: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Requests {
    pub incoming: Vec<Person>,
    pub outgoing: Vec<Person>,
}

/// What sending a request did.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Sent {
    Requested,
    /// They had already asked: you're friends now.
    Accepted,
    AlreadyFriends,
}

/// An event for friends' inboxes ("Alex beat your high score").
#[derive(Clone, Debug, Default, Serialize)]
pub struct Event {
    /// 1-40 of `a-z`, `0-9`, `_`. People can mute a kind.
    pub kind: String,
    /// 1-80 characters.
    pub title: String,
    /// Up to 280 characters.
    pub body: String,
    /// An `https` link into your app.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// A newer event with the same key replaces an unread older one.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub collapse_key: Option<String>,
    /// Only these friends (`sub`s); all friends who use your app if None.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipients: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct InboxItem {
    pub id: String,
    pub from: Option<String>,
    pub from_username: Option<String>,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub url: Option<String>,
    pub created_at: String,
    pub read_at: Option<String>,
}

/// One authenticated JSON call; errors carry the API's message.
pub(crate) async fn call(
    accounts: &Accounts,
    token: &str,
    method: reqwest::Method,
    url: &str,
    body: Option<Value>,
) -> Result<Value> {
    let mut request = accounts.http().request(method, url).bearer_auth(token);
    if let Some(body) = body {
        request = request.json(&body);
    }
    let response = request.send().await?;
    let status = response.status();
    let text: String = response.text().await?;
    if !status.is_success() {
        let message: String = serde_json::from_str::<Value>(&text)
            .ok()
            .and_then(|v| v.get("error").and_then(Value::as_str).map(str::to_owned))
            .unwrap_or(text);
        return Err(Error::Api {
            status: status.as_u16(),
            message,
        });
    }
    if text.is_empty() {
        return Ok(Value::Null);
    }
    serde_json::from_str(&text).map_err(|e| Error::Api {
        status: status.as_u16(),
        message: e.to_string(),
    })
}

fn field<T: for<'de> Deserialize<'de>>(mut value: Value, name: &str) -> Result<T> {
    serde_json::from_value(value.get_mut(name).map_or(Value::Null, Value::take)).map_err(|e| {
        Error::Api {
            status: 200,
            message: e.to_string(),
        }
    })
}

/// The API for one person (their access token).
/// Mutes every event kind from your app.
pub const ALL_KINDS: &str = "*";

pub struct Friends<'a> {
    accounts: &'a Accounts,
    token: &'a str,
    base: String,
}

impl<'a> Friends<'a> {
    #[must_use]
    pub fn new(accounts: &'a Accounts, access_token: &'a str) -> Self {
        let base: String = format!(
            "{}/api/v1",
            accounts.config().issuer.trim_end_matches("/oidc")
        );
        Self {
            accounts,
            token: access_token,
            base,
        }
    }

    async fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        body: Option<Value>,
    ) -> Result<Value> {
        call(
            self.accounts,
            self.token,
            method,
            &format!("{}{path}", self.base),
            body,
        )
        .await
    }

    /// Friends who also use your app.
    ///
    /// # Errors
    /// The API refused or the request failed.
    pub async fn list(&self) -> Result<Vec<Person>> {
        field(
            self.call(reqwest::Method::GET, "/friends", None).await?,
            "friends",
        )
    }

    /// # Errors
    /// The API refused or the request failed.
    pub async fn requests(&self) -> Result<Requests> {
        let value: Value = self
            .call(reqwest::Method::GET, "/friends/requests", None)
            .await?;
        serde_json::from_value(value).map_err(|e| Error::Api {
            status: 200,
            message: e.to_string(),
        })
    }

    /// Sends a request by exact username.
    ///
    /// # Errors
    /// Not found (also for blocks and privacy settings), limits, or failure.
    pub async fn request_by_username(&self, username: &str) -> Result<Sent> {
        field(
            self.call(
                reqwest::Method::POST,
                "/friends/requests",
                Some(json!({ "username": username })),
            )
            .await?,
            "result",
        )
    }

    /// Sends a request by friend code (works even when the person only takes
    /// requests from people with their code).
    ///
    /// # Errors
    /// Not found, limits, or failure.
    pub async fn request_by_code(&self, friend_code: &str) -> Result<Sent> {
        field(
            self.call(
                reqwest::Method::POST,
                "/friends/requests",
                Some(json!({ "friend_code": friend_code })),
            )
            .await?,
            "result",
        )
    }

    /// # Errors
    /// No such request, or failure.
    pub async fn accept(&self, sub: &str) -> Result<()> {
        self.call(
            reqwest::Method::POST,
            &format!("/friends/requests/{sub}/accept"),
            None,
        )
        .await
        .map(|_| ())
    }

    /// # Errors
    /// The request failed.
    pub async fn decline(&self, sub: &str) -> Result<()> {
        self.call(
            reqwest::Method::POST,
            &format!("/friends/requests/{sub}/decline"),
            None,
        )
        .await
        .map(|_| ())
    }

    /// Cancels a request you sent.
    ///
    /// # Errors
    /// The request failed.
    pub async fn cancel(&self, sub: &str) -> Result<()> {
        self.call(
            reqwest::Method::DELETE,
            &format!("/friends/requests/{sub}"),
            None,
        )
        .await
        .map(|_| ())
    }

    /// # Errors
    /// The request failed.
    pub async fn remove(&self, sub: &str) -> Result<()> {
        self.call(reqwest::Method::DELETE, &format!("/friends/{sub}"), None)
            .await
            .map(|_| ())
    }

    /// Blocks: ends the friendship and requests, and stops events both ways.
    ///
    /// # Errors
    /// The request failed.
    pub async fn block(&self, sub: &str) -> Result<()> {
        self.call(
            reqwest::Method::POST,
            "/blocks",
            Some(json!({ "sub": sub })),
        )
        .await
        .map(|_| ())
    }

    /// # Errors
    /// The request failed.
    pub async fn unblock(&self, sub: &str) -> Result<()> {
        self.call(reqwest::Method::DELETE, &format!("/blocks/{sub}"), None)
            .await
            .map(|_| ())
    }

    /// Posts an event; returns how many friends' inboxes it reached.
    ///
    /// # Errors
    /// Invalid event, rate limit (60 per person per hour per app), or failure.
    pub async fn post_event(&self, event: &Event) -> Result<u64> {
        let value: Value = serde_json::to_value(event).map_err(|e| Error::Api {
            status: 0,
            message: e.to_string(),
        })?;
        field(
            self.call(reqwest::Method::POST, "/events", Some(value))
                .await?,
            "delivered",
        )
    }

    /// The person's inbox items from your app, newest first.
    ///
    /// # Errors
    /// The request failed.
    pub async fn inbox(&self, limit: u32) -> Result<Vec<InboxItem>> {
        field(
            self.call(reqwest::Method::GET, &format!("/inbox?limit={limit}"), None)
                .await?,
            "items",
        )
    }

    /// # Errors
    /// The request failed.
    pub async fn mark_read(&self, id: &str) -> Result<()> {
        self.call(reqwest::Method::POST, &format!("/inbox/{id}/read"), None)
            .await
            .map(|_| ())
    }

    /// The event kinds from your app the person has muted ([`ALL_KINDS`] means
    /// all of them).
    ///
    /// # Errors
    /// The request failed.
    pub async fn mutes(&self) -> Result<Vec<String>> {
        field(
            self.call(reqwest::Method::GET, "/mutes", None).await?,
            "mutes",
        )
    }

    /// Stops friends' events of `kind` from your app reaching the person
    /// ([`ALL_KINDS`] for every kind). The person can also do this in Account
    /// Center.
    ///
    /// # Errors
    /// Malformed kind, or the request failed.
    pub async fn mute(&self, kind: &str) -> Result<()> {
        self.call(reqwest::Method::PUT, &format!("/mutes/{kind}"), None)
            .await
            .map(|_| ())
    }

    /// Undoes [`Friends::mute`].
    ///
    /// # Errors
    /// The request failed.
    pub async fn unmute(&self, kind: &str) -> Result<()> {
        self.call(reqwest::Method::DELETE, &format!("/mutes/{kind}"), None)
            .await
            .map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_serialize_without_empty_fields() {
        let event: Event = Event {
            kind: String::from("daily_completed"),
            title: String::from("Alex finished the daily"),
            ..Event::default()
        };
        let json: Value = serde_json::to_value(&event).unwrap();
        assert_eq!(
            json,
            json!({ "kind": "daily_completed", "title": "Alex finished the daily", "body": "" })
        );
    }

    #[test]
    fn sent_reads_the_service_values() {
        assert_eq!(
            serde_json::from_value::<Sent>(json!("already_friends")).unwrap(),
            Sent::AlreadyFriends
        );
    }
}
