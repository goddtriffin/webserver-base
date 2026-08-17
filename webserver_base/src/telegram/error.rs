use std::fmt::{Debug, Formatter};
use std::time::Duration;
use std::{error, fmt};

/// Every way a Telegram send can fail.
///
/// Note that [`TelegramError::Http`] always holds a [`reqwest::Error`] which has
/// had its URL stripped via [`reqwest::Error::without_url`]. The bot token is a
/// path segment of every request URL, and `reqwest`'s own `Debug` includes the
/// URL, so an un-stripped error would leak the credential into any log line.
#[derive(Debug)]
pub enum TelegramError {
    /// The bot token was empty or malformed.
    InvalidToken(String),

    /// The message was too large to even attempt to send.
    ///
    /// This is a guard against an upstream bug producing a multi-megabyte
    /// string; it is checked before chunking, so it is never the result of
    /// merely exceeding Telegram's per-message limit.
    MessageTooLarge {
        /// Size of the offending message, in bytes.
        bytes: usize,
        /// The maximum permitted size, in bytes.
        max: usize,
    },

    /// The HTTP request itself failed (connection refused, TLS failure, timeout).
    Http(reqwest::Error),

    /// Telegram accepted the request but rejected its contents.
    Api {
        /// Telegram's `error_code` (an HTTP status code).
        error_code: i32,
        /// Telegram's human-readable `description`.
        description: String,
    },

    /// Telegram returned `429` along with how long to wait.
    ///
    /// This is distinct from a plain [`TelegramError::Api`] `429` because it
    /// carries an actionable delay: Telegram is telling us precisely when the
    /// request may be repeated.
    RateLimited {
        /// How long Telegram asked us to wait.
        retry_after: Duration,
    },

    /// The response body could not be deserialized.
    Serialization(serde_json::Error),

    /// An invariant inside this library was violated.
    Internal(String),
}

impl TelegramError {
    /// Whether this error is permanent, meaning a retry can never succeed.
    ///
    /// A permanent error almost always means the notifier is misconfigured
    /// rather than that this particular message was bad: a wrong token, a chat
    /// the bot was removed from, a chat id that does not exist. Every
    /// subsequent send will fail the same way.
    #[must_use]
    pub const fn is_permanent(&self) -> bool {
        match self {
            Self::InvalidToken(_) | Self::MessageTooLarge { .. } | Self::Internal(_) => true,
            Self::Api { error_code, .. } => {
                // 429 is handled separately (it carries `retry_after`), and 5xx
                // is transient. Everything else in the 4xx range is permanent.
                *error_code >= 400 && *error_code < 500 && *error_code != 429
            }
            Self::Http(_) | Self::Serialization(_) | Self::RateLimited { .. } => false,
        }
    }

    /// Whether this error indicates the notifier as a whole is misconfigured.
    ///
    /// These deserve a louder log than an ordinary send failure, because they
    /// mean every future send is also doomed:
    /// - `401` — the bot token is wrong.
    /// - `403` — the bot was blocked, or removed from the chat.
    /// - `400` with a chat-related description — the chat id is wrong.
    #[must_use]
    pub fn is_misconfiguration(&self) -> bool {
        match self {
            Self::InvalidToken(_) => true,
            Self::Api {
                error_code,
                description,
            } => {
                *error_code == 401
                    || *error_code == 403
                    || (*error_code == 400 && description.to_lowercase().contains("chat not found"))
            }
            _ => false,
        }
    }
}

impl error::Error for TelegramError {}

impl fmt::Display for TelegramError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidToken(reason) => write!(f, "invalid telegram bot token: {reason}"),
            Self::MessageTooLarge { bytes, max } => {
                write!(
                    f,
                    "message is {bytes} bytes, which exceeds the {max} byte maximum"
                )
            }
            Self::Http(error) => write!(f, "telegram http request failed: {error}"),
            Self::Api {
                error_code,
                description,
            } => write!(
                f,
                "telegram rejected the request ({error_code}): {description}"
            ),
            Self::RateLimited { retry_after } => {
                write!(f, "telegram rate limited us; retry after {retry_after:?}")
            }
            Self::Serialization(error) => {
                write!(f, "could not deserialize telegram's response: {error}")
            }
            Self::Internal(reason) => write!(f, "internal telegram client error: {reason}"),
        }
    }
}

impl From<reqwest::Error> for TelegramError {
    fn from(error: reqwest::Error) -> Self {
        // Strip the URL unconditionally: it contains the bot token.
        Self::Http(error.without_url())
    }
}

impl From<serde_json::Error> for TelegramError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}

#[cfg(test)]
mod tests {
    use super::TelegramError;

    #[test]
    fn client_errors_are_permanent() {
        let error: TelegramError = TelegramError::Api {
            error_code: 400,
            description: String::from("Bad Request: message text is empty"),
        };

        assert!(error.is_permanent());
    }

    #[test]
    fn rate_limit_errors_are_not_permanent() {
        let error: TelegramError = TelegramError::Api {
            error_code: 429,
            description: String::from("Too Many Requests: retry after 5"),
        };

        assert!(!error.is_permanent());
    }

    #[test]
    fn server_errors_are_not_permanent() {
        let error: TelegramError = TelegramError::Api {
            error_code: 502,
            description: String::from("Bad Gateway"),
        };

        assert!(!error.is_permanent());
    }

    #[test]
    fn unauthorized_is_a_misconfiguration() {
        let error: TelegramError = TelegramError::Api {
            error_code: 401,
            description: String::from("Unauthorized"),
        };

        assert!(error.is_misconfiguration());
    }

    #[test]
    fn forbidden_is_a_misconfiguration() {
        let error: TelegramError = TelegramError::Api {
            error_code: 403,
            description: String::from("Forbidden: bot was blocked by the user"),
        };

        assert!(error.is_misconfiguration());
    }

    #[test]
    fn chat_not_found_is_a_misconfiguration() {
        let error: TelegramError = TelegramError::Api {
            error_code: 400,
            description: String::from("Bad Request: chat not found"),
        };

        assert!(error.is_misconfiguration());
    }

    #[test]
    fn an_ordinary_bad_request_is_not_a_misconfiguration() {
        let error: TelegramError = TelegramError::Api {
            error_code: 400,
            description: String::from("Bad Request: message is too long"),
        };

        assert!(!error.is_misconfiguration());
    }

    #[test]
    fn display_includes_the_error_code_and_description() {
        let error: TelegramError = TelegramError::Api {
            error_code: 400,
            description: String::from("Bad Request: chat not found"),
        };

        let expected: String =
            String::from("telegram rejected the request (400): Bad Request: chat not found");
        let actual: String = format!("{error}");
        assert_eq!(expected, actual);
    }
}
