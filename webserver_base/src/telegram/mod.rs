//! An outbound Telegram Bot API notifier.
//!
//! This module exists to replace the hand-rolled `sendMessage` calls which had
//! been copied between projects, each with a different subset of the hard parts
//! missing. It is send-only on purpose: it has no polling, no webhooks, and no
//! update handling, because none of the consuming projects receive anything.
//!
//! # Formatting without escaping
//!
//! Messages are built from [`Message::builder`], which emits Telegram
//! *entities* rather than `parse_mode` markup. Telegram's own documentation
//! describes entities as what a Markdown or HTML parser is converted *into*, so
//! nothing is lost by skipping that step — and because no markup is ever
//! embedded in the text, interpolated values never need escaping:
//!
//! ```no_run
//! use webserver_base::telegram::{ChatId, Message, ReqwestTelegram, Telegram, TelegramSettings};
//!
//! # async fn example(untrusted_filename: &str) -> Result<(), Box<dyn std::error::Error>> {
//! let settings = TelegramSettings::builder("123456789:AA...").build()?;
//! let telegram = ReqwestTelegram::new(settings, None)?;
//!
//! telegram.send(
//!     ChatId::Id(1234),
//!     Message::builder()
//!         .text("🎨 ")
//!         .bold("New pattern")
//!         .text("\nInput: ")
//!         .code(untrusted_filename) // no escaping, ever
//!         .build(),
//! );
//! # Ok(())
//! # }
//! ```
//!
//! # What it handles
//!
//! - **Length.** Telegram caps text at 4096 and captions at 1024 UTF-16 code
//!   units. Oversized messages are split at natural boundaries, with entities
//!   clamped and rebased onto each piece, capped at a configurable number of
//!   chunks so an upstream bug cannot become a flood.
//! - **Rate limits.** Sends are queued per chat and paced at Telegram's
//!   documented limits: one message per second per chat, thirty per second
//!   overall.
//! - **Retries.** A `429` is retried after the `retry_after` Telegram supplies,
//!   up to a ceiling; transient failures back off exponentially; permanent
//!   client errors are never retried, and misconfiguration is logged loudly.
//! - **Secrets.** The bot token is a path segment of every request URL, so it
//!   is held in a [`BotToken`] which refuses to print itself, and every
//!   `reqwest` error has its URL stripped before it can reach a log.

mod chat_id;
mod chunk;
mod entity;
mod error;
mod message;
mod notifier;
mod queue;
mod settings;
mod token;
mod utf16;

pub use chat_id::ChatId;
pub use entity::{Entity, EntityKind, Style};
pub use error::TelegramError;
pub use message::{FileSource, InlineBuilder, Media, Message, MessageBuilder, SendOptions};
pub use notifier::{MockTelegram, ReqwestTelegram, SentMessage, Telegram};
pub use settings::{
    DEFAULT_CONNECT_TIMEOUT, DEFAULT_GLOBAL_PER_SECOND, DEFAULT_MAX_CHUNKS,
    DEFAULT_MAX_INPUT_BYTES, DEFAULT_MAX_RETRIES, DEFAULT_MAX_RETRY_AFTER,
    DEFAULT_PER_CHAT_INTERVAL, DEFAULT_QUEUE_CAPACITY, DEFAULT_REQUEST_TIMEOUT, MAX_CAPTION_LENGTH,
    MAX_TEXT_LENGTH, TELEGRAM_API_BASE_URL, TelegramSettings, TelegramSettingsBuilder,
};
pub use token::BotToken;
