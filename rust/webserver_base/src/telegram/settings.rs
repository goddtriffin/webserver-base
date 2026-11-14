use std::time::Duration;

use super::error::TelegramError;
use super::token::BotToken;

/// The official Telegram Bot API host.
pub const TELEGRAM_API_BASE_URL: &str = "https://api.telegram.org";

/// Telegram's maximum message text length, in UTF-16 code units.
pub const MAX_TEXT_LENGTH: usize = 4096;

/// Telegram's maximum media caption length, in UTF-16 code units.
///
/// Note that this is a quarter of [`MAX_TEXT_LENGTH`]: a caption which fits in
/// a text message may still overflow when attached to a photo.
pub const MAX_CAPTION_LENGTH: usize = 1024;

/// Default ceiling on how many messages one oversized message may become.
pub const DEFAULT_MAX_CHUNKS: usize = 5;

/// Default ceiling on the size of a single message, in bytes.
///
/// This guards against an upstream bug producing an enormous string; such a
/// message is rejected outright rather than chunked.
pub const DEFAULT_MAX_INPUT_BYTES: usize = 1024 * 1024;

/// Default number of messages held per chat before new sends are dropped.
pub const DEFAULT_QUEUE_CAPACITY: usize = 1024;

/// Default minimum spacing between two messages to the same chat.
///
/// Telegram's Bot FAQ: "In a single chat, avoid sending more than one message
/// per second."
pub const DEFAULT_PER_CHAT_INTERVAL: Duration = Duration::from_secs(1);

/// Default ceiling on messages per second across all chats.
///
/// Telegram's Bot FAQ: "bots are not able to broadcast more than about 30
/// messages per second".
pub const DEFAULT_GLOBAL_PER_SECOND: u32 = 30;

/// Default number of retries for transient failures.
pub const DEFAULT_MAX_RETRIES: u32 = 3;

/// Default ceiling on an honored `retry_after`.
///
/// Telegram can ask for a very long wait. Because the queue is ordered per
/// chat, obeying an hour-long `retry_after` would stall every later message to
/// that chat, so beyond this ceiling the message is dropped instead.
pub const DEFAULT_MAX_RETRY_AFTER: Duration = Duration::from_mins(1);

/// Default timeout for establishing a connection.
pub const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Default timeout for a complete request.
pub const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

/// Configuration for a [`ReqwestTelegram`](super::ReqwestTelegram).
///
/// This type deliberately reads nothing from the environment. Where the token
/// comes from — an env var, a secrets manager, a config file — is the caller's
/// decision, and every project already handles it differently.
#[derive(Debug, Clone)]
pub struct TelegramSettings {
    pub(crate) token: BotToken,
    pub(crate) base_url: String,
    pub(crate) connect_timeout: Duration,
    pub(crate) request_timeout: Duration,
    pub(crate) queue_capacity: usize,
    pub(crate) max_chunks: usize,
    pub(crate) max_input_bytes: usize,
    pub(crate) max_retries: u32,
    pub(crate) max_retry_after: Duration,
    pub(crate) per_chat_interval: Duration,
    pub(crate) global_per_second: u32,
}

impl TelegramSettings {
    /// Starts building settings for the given bot token.
    ///
    /// The token is the only required value; everything else defaults to the
    /// constants in this module.
    #[must_use]
    pub fn builder(token: impl Into<String>) -> TelegramSettingsBuilder {
        TelegramSettingsBuilder::new(token)
    }

    /// The configured API base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
}

/// Builds a [`TelegramSettings`].
#[derive(Debug, Clone)]
pub struct TelegramSettingsBuilder {
    token: String,
    base_url: String,
    connect_timeout: Duration,
    request_timeout: Duration,
    queue_capacity: usize,
    max_chunks: usize,
    max_input_bytes: usize,
    max_retries: u32,
    max_retry_after: Duration,
    per_chat_interval: Duration,
    global_per_second: u32,
}

impl TelegramSettingsBuilder {
    /// Creates a builder for the given bot token.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self {
            token: token.into(),
            base_url: String::from(TELEGRAM_API_BASE_URL),
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            max_chunks: DEFAULT_MAX_CHUNKS,
            max_input_bytes: DEFAULT_MAX_INPUT_BYTES,
            max_retries: DEFAULT_MAX_RETRIES,
            max_retry_after: DEFAULT_MAX_RETRY_AFTER,
            per_chat_interval: DEFAULT_PER_CHAT_INTERVAL,
            global_per_second: DEFAULT_GLOBAL_PER_SECOND,
        }
    }

    /// Overrides the API base URL.
    ///
    /// Intended for pointing tests at a local mock server, or for a self-hosted
    /// Bot API server. Production should leave this at its default.
    #[must_use]
    pub fn base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    /// Overrides the connection timeout.
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// Overrides the total request timeout.
    #[must_use]
    pub const fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    /// Overrides how many messages are held per chat before sends are dropped.
    #[must_use]
    pub const fn queue_capacity(mut self, capacity: usize) -> Self {
        self.queue_capacity = capacity;
        self
    }

    /// Overrides how many messages one oversized message may become.
    #[must_use]
    pub const fn max_chunks(mut self, max_chunks: usize) -> Self {
        self.max_chunks = max_chunks;
        self
    }

    /// Overrides the largest message accepted, in bytes.
    #[must_use]
    pub const fn max_input_bytes(mut self, max_bytes: usize) -> Self {
        self.max_input_bytes = max_bytes;
        self
    }

    /// Overrides how many times a transient failure is retried.
    #[must_use]
    pub const fn max_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
    }

    /// Overrides the longest `retry_after` which will be honored.
    #[must_use]
    pub const fn max_retry_after(mut self, max_retry_after: Duration) -> Self {
        self.max_retry_after = max_retry_after;
        self
    }

    /// Overrides the minimum spacing between messages to the same chat.
    #[must_use]
    pub const fn per_chat_interval(mut self, interval: Duration) -> Self {
        self.per_chat_interval = interval;
        self
    }

    /// Overrides the ceiling on messages per second across all chats.
    #[must_use]
    pub const fn global_per_second(mut self, per_second: u32) -> Self {
        self.global_per_second = per_second;
        self
    }

    /// Validates and finalizes the settings.
    ///
    /// # Errors
    ///
    /// Returns [`TelegramError::InvalidToken`] if the token is blank or
    /// malformed. A blank-but-present token is treated as missing, because a
    /// server which boots with one looks healthy while notifying nobody.
    pub fn build(self) -> Result<TelegramSettings, TelegramError> {
        Ok(TelegramSettings {
            token: BotToken::new(self.token)?,
            base_url: self.base_url,
            connect_timeout: self.connect_timeout,
            request_timeout: self.request_timeout,
            queue_capacity: self.queue_capacity.max(1),
            max_chunks: self.max_chunks.max(1),
            max_input_bytes: self.max_input_bytes.max(1),
            max_retries: self.max_retries,
            max_retry_after: self.max_retry_after,
            per_chat_interval: self.per_chat_interval,
            global_per_second: self.global_per_second.max(1),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{
        DEFAULT_GLOBAL_PER_SECOND, DEFAULT_MAX_CHUNKS, DEFAULT_QUEUE_CAPACITY,
        TELEGRAM_API_BASE_URL, TelegramSettings,
    };
    use crate::telegram::error::TelegramError;

    const VALID: &str = "123456789:AAFNpHzr6wq4YimAMwIjqVrFU8TO5kcayEI";

    #[test]
    fn defaults_match_telegrams_documented_limits() {
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .build()
            .expect("valid token should build");

        let expected_base: String = String::from(TELEGRAM_API_BASE_URL);
        let actual_base: String = settings.base_url().to_string();
        assert_eq!(expected_base, actual_base);

        let expected_global: u32 = DEFAULT_GLOBAL_PER_SECOND;
        let actual_global: u32 = settings.global_per_second;
        assert_eq!(expected_global, actual_global);

        let expected_interval: Duration = Duration::from_secs(1);
        let actual_interval: Duration = settings.per_chat_interval;
        assert_eq!(expected_interval, actual_interval);

        let expected_chunks: usize = DEFAULT_MAX_CHUNKS;
        let actual_chunks: usize = settings.max_chunks;
        assert_eq!(expected_chunks, actual_chunks);

        let expected_capacity: usize = DEFAULT_QUEUE_CAPACITY;
        let actual_capacity: usize = settings.queue_capacity;
        assert_eq!(expected_capacity, actual_capacity);
    }

    #[test]
    fn blank_token_is_rejected_at_build_time() {
        let error: TelegramError = TelegramSettings::builder("   ")
            .build()
            .expect_err("blank token should be rejected");

        assert!(matches!(error, TelegramError::InvalidToken(_)));
    }

    #[test]
    fn an_explicit_setting_beats_the_environment_it_was_read_from() {
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url("http://127.0.0.1:8080/")
            .queue_capacity(16)
            .max_chunks(2)
            .global_per_second(5)
            .per_chat_interval(Duration::from_millis(10))
            .build()
            .expect("valid token should build");

        let expected_base: String = String::from("http://127.0.0.1:8080");
        let actual_base: String = settings.base_url().to_string();
        assert_eq!(expected_base, actual_base);

        let expected_capacity: usize = 16;
        let actual_capacity: usize = settings.queue_capacity;
        assert_eq!(expected_capacity, actual_capacity);

        let expected_chunks: usize = 2;
        let actual_chunks: usize = settings.max_chunks;
        assert_eq!(expected_chunks, actual_chunks);
    }

    #[test]
    fn trailing_slash_is_stripped_from_the_base_url() {
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url("https://example.com///")
            .build()
            .expect("valid token should build");

        let expected: String = String::from("https://example.com");
        let actual: String = settings.base_url().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn degenerate_values_are_clamped_to_something_workable() {
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .queue_capacity(0)
            .max_chunks(0)
            .global_per_second(0)
            .build()
            .expect("valid token should build");

        let expected_capacity: usize = 1;
        let actual_capacity: usize = settings.queue_capacity;
        assert_eq!(expected_capacity, actual_capacity);

        let expected_chunks: usize = 1;
        let actual_chunks: usize = settings.max_chunks;
        assert_eq!(expected_chunks, actual_chunks);

        let expected_global: u32 = 1;
        let actual_global: u32 = settings.global_per_second;
        assert_eq!(expected_global, actual_global);
    }

    #[test]
    fn debug_output_does_not_leak_the_token() {
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .build()
            .expect("valid token should build");

        let actual: String = format!("{settings:?}");
        assert!(!actual.contains("AAFNpHzr"));
        assert!(actual.contains("***"));
    }
}
