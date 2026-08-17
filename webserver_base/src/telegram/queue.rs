use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use reqwest::{Client, Response, multipart::Form, multipart::Part};
use serde::Deserialize;
use serde_json::{Map, Value};
use tokio::sync::Notify;
use tracing::{debug, error, instrument, warn};

use super::chat_id::ChatId;
use super::entity::Entity;
use super::error::TelegramError;
use super::message::{FileSource, Media, SendOptions};
use super::settings::TelegramSettings;

/// How long the worker sleeps before re-checking whether it should exit.
const IDLE_POLL: Duration = Duration::from_secs(1);

/// How often dropped-message counts are summarized into a single log line.
const DROP_REPORT_INTERVAL: Duration = Duration::from_mins(1);

/// How long [`Worker::flush`] waits between checks of the queue.
const FLUSH_POLL: Duration = Duration::from_millis(25);

/// The longest excerpt of an error response body which is logged.
const MAX_ERROR_BODY: usize = 500;

/// One prepared request: a single chunk, bound for a single chat.
#[derive(Debug, Clone)]
pub(crate) struct Outgoing {
    pub(crate) chat_id: ChatId,
    pub(crate) text: String,
    pub(crate) entities: Vec<Entity>,
    pub(crate) media: Option<Media>,
    pub(crate) options: SendOptions,
}

/// Messages waiting for one chat, and when that chat may next be written to.
#[derive(Debug)]
struct ChatQueue {
    items: VecDeque<Outgoing>,
    next_allowed: Instant,
}

/// Everything the worker and the sender share.
#[derive(Debug)]
struct QueueState {
    chats: BTreeMap<ChatId, ChatQueue>,
    recent_sends: VecDeque<Instant>,
    dropped: u64,
    last_drop_report: Instant,
}

impl QueueState {
    fn new() -> Self {
        Self {
            chats: BTreeMap::new(),
            recent_sends: VecDeque::new(),
            dropped: 0,
            last_drop_report: Instant::now(),
        }
    }

    fn is_empty(&self) -> bool {
        self.chats
            .values()
            .all(|queue: &ChatQueue| queue.items.is_empty())
    }

    fn queued(&self) -> usize {
        self.chats
            .values()
            .map(|queue: &ChatQueue| queue.items.len())
            .sum()
    }
}

/// What the worker should do next.
enum Take {
    /// A message is ready to send now.
    Ready(Box<Outgoing>),
    /// Nothing is ready until this moment.
    WaitUntil(Instant),
    /// Nothing is queued at all.
    Idle,
}

/// The shared machinery behind a notifier: queue, pacing, and HTTP.
#[derive(Debug)]
pub(crate) struct Worker {
    state: Mutex<QueueState>,
    notify: Notify,
    settings: TelegramSettings,
    client: Client,
}

impl Worker {
    /// Creates the worker and its shared state.
    pub(crate) fn new(settings: TelegramSettings, client: Client) -> Self {
        Self {
            state: Mutex::new(QueueState::new()),
            notify: Notify::new(),
            settings,
            client,
        }
    }

    /// Locks the queue, recovering rather than propagating a poisoned lock.
    ///
    /// A notification must never panic a request handler, so a lock poisoned by
    /// an unrelated panic is recovered from instead of unwrapped.
    fn lock(&self) -> MutexGuard<'_, QueueState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Enqueues one prepared chunk, dropping it if the chat's queue is full.
    ///
    /// Returns `true` if the message was accepted.
    pub(crate) fn enqueue(&self, outgoing: Outgoing) -> bool {
        let capacity: usize = self.settings.queue_capacity;
        let mut state: MutexGuard<'_, QueueState> = self.lock();

        let accepted: bool = {
            let queue: &mut ChatQueue =
                state
                    .chats
                    .entry(outgoing.chat_id.clone())
                    .or_insert_with(|| ChatQueue {
                        items: VecDeque::new(),
                        next_allowed: Instant::now(),
                    });

            if queue.items.len() >= capacity {
                false
            } else {
                queue.items.push_back(outgoing);
                true
            }
        };

        if !accepted {
            state.dropped += 1;
            Self::report_drops_if_due(&mut state);
            return false;
        }

        drop(state);

        self.notify.notify_one();
        true
    }

    /// Emits a single aggregated warning if enough time has passed.
    ///
    /// Reporting every drop individually is exactly what a flood wants: it
    /// converts a message flood into a log flood.
    fn report_drops_if_due(state: &mut QueueState) {
        let now: Instant = Instant::now();

        if now.duration_since(state.last_drop_report) < DROP_REPORT_INTERVAL {
            return;
        }

        if state.dropped > 0 {
            warn!(
                "telegram queue full: dropped {} message(s) since the last report",
                state.dropped
            );
        }

        state.dropped = 0;
        state.last_drop_report = now;
    }

    /// Takes the next message which pacing permits sending.
    fn take(&self) -> Take {
        let now: Instant = Instant::now();
        let global_window: Duration = Duration::from_secs(1);
        let mut state: MutexGuard<'_, QueueState> = self.lock();

        Self::report_drops_if_due(&mut state);

        // Prune the global rate window before consulting it.
        while state
            .recent_sends
            .front()
            .is_some_and(|sent: &Instant| now.duration_since(*sent) >= global_window)
        {
            state.recent_sends.pop_front();
        }

        if state.is_empty() {
            return Take::Idle;
        }

        // Global ceiling reached: wait for the oldest send to age out.
        if state.recent_sends.len() >= self.settings.global_per_second as usize {
            let oldest: Instant = state.recent_sends.front().copied().unwrap_or(now);
            return Take::WaitUntil(oldest + global_window);
        }

        let mut ready: Option<ChatId> = None;
        let mut earliest: Option<Instant> = None;

        for (chat_id, queue) in &state.chats {
            if queue.items.is_empty() {
                continue;
            }

            if queue.next_allowed <= now {
                ready = Some(chat_id.clone());
                break;
            }

            earliest = Some(earliest.map_or(queue.next_allowed, |current: Instant| {
                current.min(queue.next_allowed)
            }));
        }

        if let Some(chat_id) = ready {
            let taken: Option<Outgoing> =
                state
                    .chats
                    .get_mut(&chat_id)
                    .and_then(|queue: &mut ChatQueue| {
                        let taken: Option<Outgoing> = queue.items.pop_front();
                        if taken.is_some() {
                            queue.next_allowed = now + self.settings.per_chat_interval;
                        }
                        taken
                    });

            if let Some(outgoing) = taken {
                state.recent_sends.push_back(now);
                return Take::Ready(Box::new(outgoing));
            }
        }

        earliest.map_or(Take::Idle, Take::WaitUntil)
    }

    /// Whether every chat's queue is empty.
    pub(crate) fn is_drained(&self) -> bool {
        self.lock().is_empty()
    }

    /// How many messages are waiting across all chats.
    pub(crate) fn queued(&self) -> usize {
        self.lock().queued()
    }

    /// Waits for the queue to drain, up to `timeout`.
    ///
    /// Returns `true` if it drained. Intended for shutdown, so that the last
    /// notifications — often the most important ones — are not lost when the
    /// process exits.
    pub(crate) async fn flush(&self, timeout: Duration) -> bool {
        let deadline: Instant = Instant::now() + timeout;

        loop {
            if self.is_drained() {
                return true;
            }

            if Instant::now() >= deadline {
                warn!(
                    "telegram queue did not drain within {:?}; {} message(s) abandoned",
                    timeout,
                    self.queued()
                );
                return false;
            }

            tokio::time::sleep(FLUSH_POLL).await;
        }
    }

    /// Drives the queue until the owning notifier is dropped.
    pub(crate) async fn run(worker: Weak<Self>) {
        loop {
            let Some(worker) = worker.upgrade() else {
                return;
            };

            match worker.take() {
                Take::Ready(outgoing) => worker.deliver(*outgoing).await,
                Take::WaitUntil(instant) => {
                    let wait: Duration = instant
                        .saturating_duration_since(Instant::now())
                        .min(IDLE_POLL);
                    tokio::time::sleep(wait).await;
                }
                Take::Idle => {
                    // Wake early when work arrives, but still re-check
                    // periodically so the worker can notice it should exit.
                    tokio::select! {
                        () = worker.notify.notified() => {}
                        () = tokio::time::sleep(IDLE_POLL) => {}
                    }
                }
            }
        }
    }

    /// Sends one message, retrying transient failures.
    #[instrument(skip_all)]
    async fn deliver(&self, outgoing: Outgoing) {
        let mut attempt: u32 = 0;

        loop {
            let error: TelegramError = match self.send_once(&outgoing).await {
                Ok(()) => {
                    debug!("telegram message sent to chat {}", outgoing.chat_id);
                    return;
                }
                Err(error) => error,
            };

            if error.is_misconfiguration() {
                // Every future send will fail the same way; this deserves a
                // louder signal than an ordinary delivery failure.
                error!("telegram is misconfigured, notifications will keep failing: {error}");
                return;
            }

            if let TelegramError::RateLimited { retry_after } = &error {
                let retry_after: Duration = *retry_after;

                if retry_after > self.settings.max_retry_after {
                    error!(
                        "telegram asked to wait {:?}, which exceeds the {:?} ceiling; dropping message",
                        retry_after, self.settings.max_retry_after
                    );
                    return;
                }

                warn!("telegram rate limited us; waiting {retry_after:?} before retrying");
                tokio::time::sleep(retry_after).await;
                continue;
            }

            if error.is_permanent() {
                error!("telegram permanently rejected a message: {error}");
                return;
            }

            attempt += 1;

            if attempt > self.settings.max_retries {
                error!("telegram send failed after {attempt} attempt(s): {error}");
                return;
            }

            let backoff: Duration = Self::backoff(attempt);
            warn!("telegram send failed ({error}); retrying in {backoff:?}");
            tokio::time::sleep(backoff).await;
        }
    }

    /// Exponential backoff with jitter, derived from the clock to avoid a
    /// dependency on a random number generator.
    fn backoff(attempt: u32) -> Duration {
        let base_millis: u64 = 250_u64.saturating_mul(1_u64 << attempt.min(6));
        let jitter_millis: u64 = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| u64::from(elapsed.subsec_nanos() % 250));

        Duration::from_millis(base_millis + jitter_millis)
    }

    /// Performs exactly one HTTP request.
    async fn send_once(&self, outgoing: &Outgoing) -> Result<(), TelegramError> {
        let method: &str = outgoing
            .media
            .as_ref()
            .map_or("sendMessage", Media::api_method);
        let url: String = format!(
            "{}/bot{}/{method}",
            self.settings.base_url,
            self.settings.token.expose()
        );

        let response: Response = match outgoing.media.as_ref() {
            Some(media) if matches!(media.source(), FileSource::Bytes { .. }) => {
                let form: Form = Self::build_multipart(outgoing, media)?;
                self.client.post(&url).multipart(form).send().await?
            }
            _ => {
                let body: Value = Value::Object(Self::build_json(outgoing));
                self.client.post(&url).json(&body).send().await?
            }
        };

        Self::interpret(response).await
    }

    /// Builds the JSON body for a text or non-uploaded-media send.
    fn build_json(outgoing: &Outgoing) -> Map<String, Value> {
        let mut body: Map<String, Value> = Map::new();
        body.insert(
            String::from("chat_id"),
            Value::String(outgoing.chat_id.to_string()),
        );

        let (text_field, entities_field): (&str, &str) = if outgoing.media.is_some() {
            ("caption", "caption_entities")
        } else {
            ("text", "entities")
        };

        body.insert(
            String::from(text_field),
            Value::String(outgoing.text.clone()),
        );

        if !outgoing.entities.is_empty()
            && let Ok(entities) = serde_json::to_value(&outgoing.entities)
        {
            body.insert(String::from(entities_field), entities);
        }

        if let Some(media) = outgoing.media.as_ref() {
            let reference: Option<&str> = match media.source() {
                FileSource::Url(url) => Some(url.as_str()),
                FileSource::FileId(file_id) => Some(file_id.as_str()),
                FileSource::Bytes { .. } => None,
            };

            if let Some(reference) = reference {
                body.insert(
                    String::from(media.field_name()),
                    Value::String(String::from(reference)),
                );
            }
        }

        Self::insert_options(&mut body, &outgoing.options, outgoing.media.is_some());
        body
    }

    /// Adds the optional per-send parameters to a JSON body.
    fn insert_options(body: &mut Map<String, Value>, options: &SendOptions, has_media: bool) {
        if options.disable_notification {
            body.insert(String::from("disable_notification"), Value::Bool(true));
        }

        if options.protect_content {
            body.insert(String::from("protect_content"), Value::Bool(true));
        }

        // Link previews only apply to text messages.
        if !has_media && let Some(disabled) = options.disable_link_preview {
            let mut preview: Map<String, Value> = Map::new();
            preview.insert(String::from("is_disabled"), Value::Bool(disabled));
            body.insert(String::from("link_preview_options"), Value::Object(preview));
        }

        if let Some(thread_id) = options.message_thread_id {
            body.insert(
                String::from("message_thread_id"),
                Value::Number(thread_id.into()),
            );
        }

        if let Some(message_id) = options.reply_to_message_id {
            let mut reply: Map<String, Value> = Map::new();
            reply.insert(String::from("message_id"), Value::Number(message_id.into()));
            body.insert(String::from("reply_parameters"), Value::Object(reply));
        }
    }

    /// Builds a multipart form for an in-memory file upload.
    fn build_multipart(outgoing: &Outgoing, media: &Media) -> Result<Form, TelegramError> {
        let FileSource::Bytes { filename, bytes } = media.source() else {
            // Only reachable if the caller routed a non-upload source here.
            return Err(TelegramError::Internal(String::from(
                "multipart requested for a non-byte file source",
            )));
        };

        let mut form: Form = Form::new().text("chat_id", outgoing.chat_id.to_string());

        if !outgoing.text.is_empty() {
            form = form.text("caption", outgoing.text.clone());
        }

        if !outgoing.entities.is_empty() {
            let entities: String = serde_json::to_string(&outgoing.entities)?;
            form = form.text("caption_entities", entities);
        }

        if outgoing.options.disable_notification {
            form = form.text("disable_notification", "true");
        }

        if outgoing.options.protect_content {
            form = form.text("protect_content", "true");
        }

        if let Some(thread_id) = outgoing.options.message_thread_id {
            form = form.text("message_thread_id", thread_id.to_string());
        }

        let part: Part = Part::bytes(bytes.clone()).file_name(filename.clone());
        Ok(form.part(String::from(media.field_name()), part))
    }

    /// Turns a Telegram response into a success or a typed error.
    async fn interpret(response: Response) -> Result<(), TelegramError> {
        let status: reqwest::StatusCode = response.status();
        let body: String = response.text().await?;

        let Ok(parsed) = serde_json::from_str::<ApiResponse>(&body) else {
            if status.is_success() {
                return Ok(());
            }

            return Err(TelegramError::Api {
                error_code: i32::from(status.as_u16()),
                description: truncate(&body, MAX_ERROR_BODY),
            });
        };

        if parsed.ok {
            return Ok(());
        }

        let error_code: i32 = parsed
            .error_code
            .unwrap_or_else(|| i32::from(status.as_u16()));
        let description: String = parsed.description.unwrap_or_default();

        if error_code == 429
            && let Some(seconds) = parsed
                .parameters
                .and_then(|p: ResponseParameters| p.retry_after)
        {
            return Err(TelegramError::RateLimited {
                retry_after: Duration::from_secs(seconds),
            });
        }

        Err(TelegramError::Api {
            error_code,
            description,
        })
    }
}

/// Shortens a string to at most `max` characters, on a character boundary.
fn truncate(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_string();
    }

    text.chars().take(max).collect()
}

/// Telegram's envelope, which wraps every response.
#[derive(Debug, Deserialize)]
struct ApiResponse {
    ok: bool,
    description: Option<String>,
    error_code: Option<i32>,
    parameters: Option<ResponseParameters>,
}

/// The `parameters` object Telegram attaches to some errors.
#[derive(Debug, Deserialize)]
struct ResponseParameters {
    retry_after: Option<u64>,
}

/// Spawns the background worker, returning a handle to the shared state.
pub(crate) fn spawn(worker: &Arc<Worker>) {
    let weak: Weak<Worker> = Arc::downgrade(worker);
    tokio::spawn(Worker::run(weak));
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{Outgoing, Worker, truncate};
    use crate::telegram::chat_id::ChatId;
    use crate::telegram::entity::{Entity, EntityKind};
    use crate::telegram::message::{FileSource, Media, SendOptions};
    use crate::telegram::settings::TelegramSettings;
    use reqwest::Client;
    use serde_json::{Map, Value};

    const VALID: &str = "123456789:AAFNpHzr6wq4YimAMwIjqVrFU8TO5kcayEI";

    fn settings() -> TelegramSettings {
        TelegramSettings::builder(VALID)
            .queue_capacity(2)
            .build()
            .expect("valid token should build")
    }

    fn outgoing(text: &str) -> Outgoing {
        Outgoing {
            chat_id: ChatId::Id(1),
            text: String::from(text),
            entities: Vec::new(),
            media: None,
            options: SendOptions::default(),
        }
    }

    #[tokio::test]
    async fn enqueue_accepts_up_to_capacity() {
        let worker: Worker = Worker::new(settings(), Client::new());

        assert!(worker.enqueue(outgoing("one")));
        assert!(worker.enqueue(outgoing("two")));

        let expected: usize = 2;
        let actual: usize = worker.queued();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn enqueue_drops_the_newest_when_full() {
        let worker: Worker = Worker::new(settings(), Client::new());

        assert!(worker.enqueue(outgoing("one")));
        assert!(worker.enqueue(outgoing("two")));
        assert!(!worker.enqueue(outgoing("three")));

        let expected: usize = 2;
        let actual: usize = worker.queued();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn capacity_is_tracked_per_chat() {
        let worker: Worker = Worker::new(settings(), Client::new());

        let mut other: Outgoing = outgoing("other");
        other.chat_id = ChatId::Id(2);

        assert!(worker.enqueue(outgoing("one")));
        assert!(worker.enqueue(outgoing("two")));
        // A different chat has its own budget.
        assert!(worker.enqueue(other));

        let expected: usize = 3;
        let actual: usize = worker.queued();
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn an_empty_queue_is_drained() {
        let worker: Worker = Worker::new(settings(), Client::new());

        assert!(worker.is_drained());
    }

    #[tokio::test]
    async fn flush_returns_false_when_the_queue_cannot_drain() {
        // No worker is running, so nothing will ever be taken off the queue.
        let worker: Worker = Worker::new(settings(), Client::new());
        worker.enqueue(outgoing("stuck"));

        let expected: bool = false;
        let actual: bool = worker.flush(Duration::from_millis(60)).await;
        assert_eq!(expected, actual);
    }

    #[tokio::test]
    async fn flush_returns_true_immediately_when_already_drained() {
        let worker: Worker = Worker::new(settings(), Client::new());

        let expected: bool = true;
        let actual: bool = worker.flush(Duration::from_millis(60)).await;
        assert_eq!(expected, actual);
    }

    #[test]
    fn text_body_uses_text_and_entities_fields() {
        let mut item: Outgoing = outgoing("hello");
        item.entities = vec![Entity::new(EntityKind::Bold, 0, 5)];

        let body: Map<String, Value> = Worker::build_json(&item);

        assert!(body.contains_key("text"));
        assert!(body.contains_key("entities"));
        assert!(!body.contains_key("caption"));
    }

    #[test]
    fn media_body_uses_caption_fields() {
        let mut item: Outgoing = outgoing("a caption");
        item.entities = vec![Entity::new(EntityKind::Bold, 0, 1)];
        item.media = Some(Media::Photo(FileSource::url("https://example.com/x.png")));

        let body: Map<String, Value> = Worker::build_json(&item);

        assert!(body.contains_key("caption"));
        assert!(body.contains_key("caption_entities"));
        assert!(!body.contains_key("text"));

        let expected: Value = Value::String(String::from("https://example.com/x.png"));
        let actual: Value = body
            .get("photo")
            .cloned()
            .expect("photo field should exist");
        assert_eq!(expected, actual);
    }

    #[test]
    fn file_id_media_is_sent_as_a_plain_string() {
        let mut item: Outgoing = outgoing("");
        item.media = Some(Media::Document(FileSource::file_id("abc123")));

        let body: Map<String, Value> = Worker::build_json(&item);

        let expected: Value = Value::String(String::from("abc123"));
        let actual: Value = body
            .get("document")
            .cloned()
            .expect("document field should exist");
        assert_eq!(expected, actual);
    }

    #[test]
    fn chat_id_is_always_serialized_as_a_string() {
        let body: Map<String, Value> = Worker::build_json(&outgoing("hi"));

        let expected: Value = Value::String(String::from("1"));
        let actual: Value = body.get("chat_id").cloned().expect("chat_id should exist");
        assert_eq!(expected, actual);
    }

    #[test]
    fn options_are_omitted_when_unset() {
        let body: Map<String, Value> = Worker::build_json(&outgoing("hi"));

        assert!(!body.contains_key("disable_notification"));
        assert!(!body.contains_key("protect_content"));
        assert!(!body.contains_key("link_preview_options"));
        assert!(!body.contains_key("reply_parameters"));
    }

    #[test]
    fn options_are_included_when_set() {
        let mut item: Outgoing = outgoing("hi");
        item.options = SendOptions {
            disable_notification: true,
            protect_content: true,
            disable_link_preview: Some(true),
            message_thread_id: Some(7),
            reply_to_message_id: Some(11),
        };

        let body: Map<String, Value> = Worker::build_json(&item);

        assert_eq!(Some(&Value::Bool(true)), body.get("disable_notification"));
        assert_eq!(Some(&Value::Bool(true)), body.get("protect_content"));
        assert!(body.contains_key("link_preview_options"));
        assert!(body.contains_key("message_thread_id"));
        assert!(body.contains_key("reply_parameters"));
    }

    #[test]
    fn link_preview_options_are_dropped_for_media() {
        let mut item: Outgoing = outgoing("caption");
        item.media = Some(Media::Photo(FileSource::url("https://example.com/x.png")));
        item.options.disable_link_preview = Some(true);

        let body: Map<String, Value> = Worker::build_json(&item);

        assert!(!body.contains_key("link_preview_options"));
    }

    #[test]
    fn backoff_grows_with_each_attempt() {
        let first: Duration = Worker::backoff(1);
        let third: Duration = Worker::backoff(3);

        assert!(third > first);
    }

    #[test]
    fn truncate_leaves_short_strings_alone() {
        let expected: String = String::from("hello");
        let actual: String = truncate("hello", 10);
        assert_eq!(expected, actual);
    }

    #[test]
    fn truncate_shortens_long_strings() {
        let expected: usize = 5;
        let actual: usize = truncate(&"a".repeat(100), 5).chars().count();
        assert_eq!(expected, actual);
    }

    #[test]
    fn truncate_respects_character_boundaries() {
        let expected: String = String::from("🎨🎨");
        let actual: String = truncate("🎨🎨🎨", 2);
        assert_eq!(expected, actual);
    }
}
