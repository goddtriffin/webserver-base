use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use reqwest::Client;
use tracing::{error, instrument};

use super::chat_id::ChatId;
use super::chunk::{Chunk, SplitOutcome, prepare};
use super::error::TelegramError;
use super::message::{FileSource, Media, Message, SendOptions};
use super::queue::{Outgoing, Worker, spawn};
use super::settings::{MAX_CAPTION_LENGTH, MAX_TEXT_LENGTH, TelegramSettings};

/// Telegram's upload ceiling for a photo, in bytes.
const MAX_PHOTO_BYTES: usize = 10 * 1024 * 1024;

/// Telegram's upload ceiling for a document, in bytes.
const MAX_DOCUMENT_BYTES: usize = 50 * 1024 * 1024;

/// Substituted when a message would otherwise be empty, which Telegram rejects.
const EMPTY_PLACEHOLDER: &str = "<no content>";

/// Sends outbound Telegram notifications.
///
/// Sending is deliberately **synchronous and infallible**: a notification must
/// never block a request handler, and there is rarely anything upstream which
/// could act on a delivery failure anyway. Failures surface through `tracing`
/// instead, which reaches Sentry through the usual subscriber.
pub trait Telegram: Send + Sync {
    /// Queues a message for delivery to a chat.
    ///
    /// Returns immediately. The message is chunked, paced against Telegram's
    /// rate limits, retried on transient failure, and finally delivered on a
    /// background task.
    fn send(&self, chat_id: ChatId, message: Message);

    /// Queues an unformatted message.
    ///
    /// Convenience for the common case where no formatting is needed.
    fn send_text(&self, chat_id: ChatId, text: &str) {
        self.send(chat_id, Message::text(text));
    }
}

/// A [`Telegram`] which really talks to the Bot API over HTTP.
#[derive(Debug)]
pub struct ReqwestTelegram {
    worker: Arc<Worker>,
    max_input_bytes: usize,
    max_chunks: usize,
}

impl ReqwestTelegram {
    /// Creates a notifier and spawns its background delivery worker.
    ///
    /// If `client` is `None`, an HTTP client is built from the timeouts in
    /// `settings`. Passing an existing client lets the notifier share the
    /// application's connection pool.
    ///
    /// # Errors
    ///
    /// Returns [`TelegramError::Http`] if a client must be built and cannot be.
    ///
    /// # Panics
    ///
    /// Panics if called outside a Tokio runtime, because the delivery worker is
    /// spawned during construction.
    pub fn new(settings: TelegramSettings, client: Option<Client>) -> Result<Self, TelegramError> {
        let client: Client = match client {
            Some(client) => client,
            None => Client::builder()
                .connect_timeout(settings.connect_timeout)
                .timeout(settings.request_timeout)
                .build()?,
        };

        let max_input_bytes: usize = settings.max_input_bytes;
        let max_chunks: usize = settings.max_chunks;

        let worker: Arc<Worker> = Arc::new(Worker::new(settings, client));
        spawn(&worker);

        Ok(Self {
            worker,
            max_input_bytes,
            max_chunks,
        })
    }

    /// Waits for every queued message to be delivered, up to `timeout`.
    ///
    /// Call this during shutdown. Without it, whatever is still queued when the
    /// process exits is lost — including, typically, the notification about
    /// whatever caused the shutdown.
    ///
    /// Returns `true` if the queue drained in time.
    pub async fn flush(&self, timeout: Duration) -> bool {
        self.worker.flush(timeout).await
    }

    /// How many messages are currently waiting to be delivered.
    #[must_use]
    pub fn queued(&self) -> usize {
        self.worker.queued()
    }

    /// Removes control characters which Telegram rejects, keeping newlines and tabs.
    fn sanitize(text: &str) -> String {
        text.chars()
            .filter(|character: &char| {
                !character.is_control() || *character == '\n' || *character == '\t'
            })
            .collect()
    }

    /// Rejects an attachment which exceeds Telegram's upload ceiling.
    fn media_is_sendable(media: &Media) -> bool {
        let FileSource::Bytes { bytes, .. } = media.source() else {
            return true;
        };

        let (limit, label): (usize, &str) = match media {
            Media::Photo(_) => (MAX_PHOTO_BYTES, "photo"),
            Media::Document(_) => (MAX_DOCUMENT_BYTES, "document"),
        };

        if bytes.len() > limit {
            error!(
                "telegram {label} is {} bytes, which exceeds the {limit} byte upload limit; dropping message",
                bytes.len()
            );
            return false;
        }

        true
    }

    /// Strips the options which only make sense on the first chunk of a split.
    fn continuation_options(options: &SendOptions) -> SendOptions {
        SendOptions {
            reply_to_message_id: None,
            ..options.clone()
        }
    }
}

impl Telegram for ReqwestTelegram {
    #[instrument(skip_all)]
    fn send(&self, chat_id: ChatId, message: Message) {
        if message.text.len() > self.max_input_bytes {
            error!(
                "{}",
                TelegramError::MessageTooLarge {
                    bytes: message.text.len(),
                    max: self.max_input_bytes,
                }
            );
            return;
        }

        if let Some(media) = message.media.as_ref()
            && !Self::media_is_sendable(media)
        {
            return;
        }

        let has_media: bool = message.media.is_some();
        let mut text: String = Self::sanitize(&message.text);

        // Telegram rejects an empty `text`, though an empty caption is fine.
        if text.trim().is_empty() && !has_media {
            text = String::from(EMPTY_PLACEHOLDER);
        }

        let limit: usize = if has_media {
            MAX_CAPTION_LENGTH
        } else {
            MAX_TEXT_LENGTH
        };

        let outcome: SplitOutcome = prepare(&text, &message.entities, limit, self.max_chunks);

        if outcome.dropped_units > 0 {
            error!(
                "telegram message exceeded {} chunk(s); {} character(s) were truncated",
                self.max_chunks, outcome.dropped_units
            );
        }

        for (index, chunk) in outcome.chunks.into_iter().enumerate() {
            let chunk: Chunk = chunk;
            let is_first: bool = index == 0;

            let accepted: bool = self.worker.enqueue(Outgoing {
                chat_id: chat_id.clone(),
                text: chunk.text,
                entities: chunk.entities,
                // Only the first chunk carries the attachment; the rest are
                // follow-up text messages continuing an overlong caption.
                media: if is_first {
                    message.media.clone()
                } else {
                    None
                },
                options: if is_first {
                    message.options.clone()
                } else {
                    Self::continuation_options(&message.options)
                },
            });

            if !accepted {
                // Nothing more will fit either; stop rather than interleaving.
                break;
            }
        }
    }
}

/// One message captured by [`MockTelegram`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentMessage {
    /// The chat the message was addressed to.
    pub chat_id: ChatId,
    /// The message itself, exactly as the caller built it.
    pub message: Message,
}

/// A [`Telegram`] which records messages instead of sending them.
///
/// Used both as the test double and as the local-development implementation, so
/// that neither tests nor a developer's machine can reach the real bot. It
/// records every message and logs nothing.
#[derive(Debug, Default)]
pub struct MockTelegram {
    sent: Mutex<Vec<SentMessage>>,
}

impl MockTelegram {
    /// Creates an empty mock.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Locks the recorded messages, recovering from a poisoned lock.
    fn lock(&self) -> MutexGuard<'_, Vec<SentMessage>> {
        self.sent.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Every message recorded, in the order it was sent.
    #[must_use]
    pub fn sent(&self) -> Vec<SentMessage> {
        self.lock().clone()
    }

    /// Every message recorded for one chat, in order.
    #[must_use]
    pub fn sent_to(&self, chat_id: &ChatId) -> Vec<Message> {
        self.lock()
            .iter()
            .filter(|sent: &&SentMessage| &sent.chat_id == chat_id)
            .map(|sent: &SentMessage| sent.message.clone())
            .collect()
    }

    /// The text of every message recorded, in order.
    #[must_use]
    pub fn texts(&self) -> Vec<String> {
        self.lock()
            .iter()
            .map(|sent: &SentMessage| sent.message.as_text().to_string())
            .collect()
    }

    /// How many messages have been recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Whether no messages have been recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }

    /// Discards every recorded message.
    pub fn clear(&self) {
        self.lock().clear();
    }
}

impl Telegram for MockTelegram {
    fn send(&self, chat_id: ChatId, message: Message) {
        self.lock().push(SentMessage { chat_id, message });
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::time::Duration;

    use axum::Router;
    use axum::extract::State;
    use serde_json::Value;
    use tokio::net::TcpListener;
    use tokio::sync::Mutex as AsyncMutex;

    use super::{MockTelegram, ReqwestTelegram, SentMessage, Telegram};
    use crate::telegram::chat_id::ChatId;
    use crate::telegram::message::Message;
    use crate::telegram::settings::TelegramSettings;

    const VALID: &str = "123456789:AAFNpHzr6wq4YimAMwIjqVrFU8TO5kcayEI";

    type Captured = Arc<AsyncMutex<Vec<Value>>>;

    /// Serves Telegram's success envelope and records every request body.
    async fn capture(State(captured): State<Captured>, body: String) -> &'static str {
        if let Ok(value) = serde_json::from_str::<Value>(&body) {
            captured.lock().await.push(value);
        }

        r#"{"ok":true,"result":{}}"#
    }

    /// Starts a local stand-in for the Bot API, returning its base URL.
    async fn mock_api() -> (String, Captured) {
        let captured: Captured = Arc::new(AsyncMutex::new(Vec::new()));
        let app: Router = Router::new()
            .fallback(capture)
            .with_state(Arc::clone(&captured));

        let listener: TcpListener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("should bind an ephemeral port");
        let address: std::net::SocketAddr = listener
            .local_addr()
            .expect("listener should have an address");

        tokio::spawn(async move {
            axum::serve(listener, app).await.expect("server should run");
        });

        (format!("http://{address}"), captured)
    }

    #[tokio::test]
    async fn mock_records_what_it_was_given() {
        let mock: MockTelegram = MockTelegram::new();
        mock.send(ChatId::Id(1), Message::from("hello"));

        let expected: Vec<SentMessage> = vec![SentMessage {
            chat_id: ChatId::Id(1),
            message: Message::from("hello"),
        }];
        let actual: Vec<SentMessage> = mock.sent();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn a_fresh_mock_has_recorded_nothing_so_a_test_starts_from_a_known_state() {
        let mock: MockTelegram = MockTelegram::new();

        assert!(mock.is_empty());

        let expected: usize = 0;
        let actual: usize = mock.len();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn mock_filters_by_chat() {
        let mock: MockTelegram = MockTelegram::new();
        mock.send(ChatId::Id(1), Message::from("one"));
        mock.send(ChatId::Id(2), Message::from("two"));
        mock.send(ChatId::Id(1), Message::from("three"));

        let expected: Vec<Message> = vec![Message::from("one"), Message::from("three")];
        let actual: Vec<Message> = mock.sent_to(&ChatId::Id(1));
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn clearing_a_mock_discards_earlier_sends_so_one_mock_can_serve_several_phases() {
        let mock: MockTelegram = MockTelegram::new();
        mock.send(ChatId::Id(1), Message::from("hello"));
        mock.clear();

        assert!(mock.is_empty());
    }

    #[tokio::test]
    async fn mock_is_usable_behind_a_trait_object() {
        let mock: Arc<MockTelegram> = Arc::new(MockTelegram::new());
        let notifier: Arc<dyn Telegram> = Arc::clone(&mock) as Arc<dyn Telegram>;

        notifier.send_text(ChatId::Id(1), "via dyn");

        let expected: Vec<String> = vec![String::from("via dyn")];
        let actual: Vec<String> = mock.texts();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn a_plain_message_reaches_the_api() {
        let (base_url, captured): (String, Captured) = mock_api().await;
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url(base_url)
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        notifier.send(ChatId::Id(42), Message::from("hello"));

        let drained: bool = notifier.flush(Duration::from_secs(5)).await;
        assert!(drained);

        let bodies: Vec<Value> = captured.lock().await.clone();

        let expected: usize = 1;
        let actual: usize = bodies.len();
        assert_eq!(expected, actual);

        let expected_text: Value = Value::String(String::from("hello"));
        let actual_text: Value = bodies[0]
            .get("text")
            .cloned()
            .expect("body should carry text");
        assert_eq!(expected_text, actual_text);

        let expected_chat: Value = Value::String(String::from("42"));
        let actual_chat: Value = bodies[0]
            .get("chat_id")
            .cloned()
            .expect("body should carry chat_id");
        assert_eq!(expected_chat, actual_chat);
    }

    #[tokio::test]
    async fn entities_are_sent_instead_of_markup() {
        let (base_url, captured): (String, Captured) = mock_api().await;
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url(base_url)
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        notifier.send(
            ChatId::Id(1),
            Message::builder().text("hi ").bold("there").build(),
        );

        assert!(notifier.flush(Duration::from_secs(5)).await);

        let bodies: Vec<Value> = captured.lock().await.clone();

        // The text carries no markup at all.
        let expected_text: Value = Value::String(String::from("hi there"));
        let actual_text: Value = bodies[0].get("text").cloned().expect("text should exist");
        assert_eq!(expected_text, actual_text);

        // There is no parse_mode anywhere in the request.
        assert!(bodies[0].get("parse_mode").is_none());

        let entities: &Value = bodies[0].get("entities").expect("entities should exist");
        let expected_entities: Value = serde_json::json!([
            {"type": "bold", "offset": 3, "length": 5}
        ]);
        assert_eq!(&expected_entities, entities);
    }

    #[tokio::test]
    async fn an_oversized_message_is_split_into_several_requests() {
        let (base_url, captured): (String, Captured) = mock_api().await;
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url(base_url)
            // Pace fast so the test does not wait a real second per chunk.
            .per_chat_interval(Duration::from_millis(1))
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        notifier.send(ChatId::Id(1), Message::from("a".repeat(10_000)));

        assert!(notifier.flush(Duration::from_secs(10)).await);

        let bodies: Vec<Value> = captured.lock().await.clone();

        let expected: usize = 3;
        let actual: usize = bodies.len();
        assert_eq!(expected, actual);

        // Each piece announces its position in the sequence.
        for (index, body) in bodies.iter().enumerate() {
            let text: &str = body
                .get("text")
                .and_then(Value::as_str)
                .expect("text should exist");
            assert!(text.starts_with(&format!("({}/3) ", index + 1)));
        }
    }

    #[tokio::test]
    async fn an_empty_message_is_replaced_rather_than_rejected() {
        let (base_url, captured): (String, Captured) = mock_api().await;
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url(base_url)
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        notifier.send(ChatId::Id(1), Message::from("   "));

        assert!(notifier.flush(Duration::from_secs(5)).await);

        let bodies: Vec<Value> = captured.lock().await.clone();

        let expected: Value = Value::String(String::from("<no content>"));
        let actual: Value = bodies[0].get("text").cloned().expect("text should exist");
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn control_characters_are_stripped() {
        let (base_url, captured): (String, Captured) = mock_api().await;
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url(base_url)
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        notifier.send(ChatId::Id(1), Message::from("a\u{0}b\nc\td"));

        assert!(notifier.flush(Duration::from_secs(5)).await);

        let bodies: Vec<Value> = captured.lock().await.clone();

        let expected: Value = Value::String(String::from("ab\nc\td"));
        let actual: Value = bodies[0].get("text").cloned().expect("text should exist");
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn an_absurdly_large_message_is_never_sent() {
        let (base_url, captured): (String, Captured) = mock_api().await;
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url(base_url)
            .max_input_bytes(64)
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        notifier.send(ChatId::Id(1), Message::from("a".repeat(1000)));

        assert!(notifier.flush(Duration::from_secs(1)).await);

        let bodies: Vec<Value> = captured.lock().await.clone();

        let expected: usize = 0;
        let actual: usize = bodies.len();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn media_is_sent_with_a_caption() {
        let (base_url, captured): (String, Captured) = mock_api().await;
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            .base_url(base_url)
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        notifier.send(
            ChatId::Id(1),
            Message::builder()
                .bold("Pattern ready")
                .photo(crate::telegram::message::FileSource::url(
                    "https://example.com/p.png",
                ))
                .build(),
        );

        assert!(notifier.flush(Duration::from_secs(5)).await);

        let bodies: Vec<Value> = captured.lock().await.clone();

        let expected_caption: Value = Value::String(String::from("Pattern ready"));
        let actual_caption: Value = bodies[0]
            .get("caption")
            .cloned()
            .expect("caption should exist");
        assert_eq!(expected_caption, actual_caption);

        assert!(bodies[0].get("caption_entities").is_some());
        assert!(bodies[0].get("text").is_none());
    }

    #[tokio::test]
    async fn a_full_queue_drops_rather_than_growing() {
        let settings: TelegramSettings = TelegramSettings::builder(VALID)
            // Nothing is listening, so the queue cannot drain.
            .base_url("http://127.0.0.1:1")
            .queue_capacity(3)
            .build()
            .expect("settings should build");
        let notifier: ReqwestTelegram =
            ReqwestTelegram::new(settings, None).expect("notifier should build");

        for index in 0..100 {
            notifier.send(ChatId::Id(1), Message::from(format!("message {index}")));
        }

        // At most capacity is retained; one may already be in flight.
        assert!(notifier.queued() <= 3);
    }
}
