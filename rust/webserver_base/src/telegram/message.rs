use super::entity::{Entity, EntityKind, Style};
use super::utf16::{utf16_len, utf16_len_rtrimmed};

/// The accumulated text and entities of a message under construction.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Parts {
    text: String,
    entities: Vec<Entity>,
}

impl Parts {
    /// Appends text carrying zero or more inline styles.
    ///
    /// Each style in the set produces its own entity over the same span, which
    /// is how Telegram represents combined formatting.
    fn push_styled(&mut self, text: &str, style: Style) {
        let offset: usize = utf16_len(&self.text);
        // Telegram requires entity lengths to exclude trailing whitespace.
        let length: usize = utf16_len_rtrimmed(text);

        self.text.push_str(text);

        if length == 0 {
            return;
        }

        for kind in style.kinds() {
            self.entities.push(Entity::new(kind, offset, length));
        }
    }

    /// Appends text carrying exactly one entity kind.
    fn push_kind(&mut self, text: &str, kind: EntityKind) {
        let offset: usize = utf16_len(&self.text);
        let length: usize = utf16_len_rtrimmed(text);

        self.text.push_str(text);

        if length == 0 {
            return;
        }

        self.entities.push(Entity::new(kind, offset, length));
    }

    /// Appends another set of parts, rebasing its entities, and wraps the
    /// appended span in `wrapper`.
    fn push_block(&mut self, inner: Self, wrapper: EntityKind) {
        let offset: usize = utf16_len(&self.text);
        let length: usize = utf16_len_rtrimmed(&inner.text);

        for mut entity in inner.entities {
            entity.offset += offset;
            self.entities.push(entity);
        }

        self.text.push_str(&inner.text);

        if length == 0 {
            return;
        }

        self.entities.push(Entity::new(wrapper, offset, length));
    }
}

/// Builds the contents of a block-level entity, such as a blockquote.
///
/// This deliberately exposes only *inline* formatting. Telegram documents that
/// `pre` cannot be nested inside other entities, and nesting a blockquote in a
/// blockquote is meaningless, so neither is reachable here — the restriction is
/// enforced by the type system rather than discovered at runtime.
#[derive(Debug, Clone, Default)]
pub struct InlineBuilder {
    parts: Parts,
}

impl InlineBuilder {
    /// Appends unformatted text.
    #[must_use]
    pub fn text(mut self, text: impl AsRef<str>) -> Self {
        self.parts.push_styled(text.as_ref(), Style::NONE);
        self
    }

    /// Appends text carrying an arbitrary combination of inline styles.
    #[must_use]
    pub fn styled(mut self, text: impl AsRef<str>, style: Style) -> Self {
        self.parts.push_styled(text.as_ref(), style);
        self
    }

    /// Appends **bold** text.
    #[must_use]
    pub fn bold(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::BOLD)
    }

    /// Appends *italic* text.
    #[must_use]
    pub fn italic(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::ITALIC)
    }

    /// Appends underlined text.
    #[must_use]
    pub fn underline(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::UNDERLINE)
    }

    /// Appends struck-through text.
    #[must_use]
    pub fn strikethrough(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::STRIKETHROUGH)
    }

    /// Appends text hidden behind a spoiler.
    #[must_use]
    pub fn spoiler(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::SPOILER)
    }

    /// Appends `monospaced` inline code.
    #[must_use]
    pub fn code(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::CODE)
    }

    /// Appends text which links to a URL.
    #[must_use]
    pub fn link(mut self, text: impl AsRef<str>, url: impl Into<String>) -> Self {
        self.parts
            .push_kind(text.as_ref(), EntityKind::TextLink { url: url.into() });
        self
    }

    /// Appends a mention of a user who has no username.
    #[must_use]
    pub fn mention(mut self, text: impl AsRef<str>, user_id: i64) -> Self {
        self.parts
            .push_kind(text.as_ref(), EntityKind::TextMention { user_id });
        self
    }

    /// Appends an inline custom emoji sticker.
    #[must_use]
    pub fn custom_emoji(
        mut self,
        text: impl AsRef<str>,
        custom_emoji_id: impl Into<String>,
    ) -> Self {
        self.parts.push_kind(
            text.as_ref(),
            EntityKind::CustomEmoji {
                custom_emoji_id: custom_emoji_id.into(),
            },
        );
        self
    }

    /// Consumes this builder, yielding its accumulated parts.
    fn into_parts(self) -> Parts {
        self.parts
    }
}

/// Builds a [`Message`] from styled spans.
///
/// Formatting is expressed as Telegram *entities* rather than as `parse_mode`
/// markup, which means interpolated text is never parsed and therefore never
/// needs escaping. A filename, a username, or an arbitrary error string can be
/// passed straight through with no sanitizing step.
#[derive(Debug, Clone, Default)]
pub struct MessageBuilder {
    parts: Parts,
    media: Option<Media>,
    options: SendOptions,
}

impl MessageBuilder {
    /// Appends unformatted text.
    #[must_use]
    pub fn text(mut self, text: impl AsRef<str>) -> Self {
        self.parts.push_styled(text.as_ref(), Style::NONE);
        self
    }

    /// Appends text carrying an arbitrary combination of inline styles.
    #[must_use]
    pub fn styled(mut self, text: impl AsRef<str>, style: Style) -> Self {
        self.parts.push_styled(text.as_ref(), style);
        self
    }

    /// Appends **bold** text.
    #[must_use]
    pub fn bold(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::BOLD)
    }

    /// Appends *italic* text.
    #[must_use]
    pub fn italic(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::ITALIC)
    }

    /// Appends underlined text.
    #[must_use]
    pub fn underline(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::UNDERLINE)
    }

    /// Appends struck-through text.
    #[must_use]
    pub fn strikethrough(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::STRIKETHROUGH)
    }

    /// Appends text hidden behind a spoiler.
    #[must_use]
    pub fn spoiler(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::SPOILER)
    }

    /// Appends `monospaced` inline code.
    #[must_use]
    pub fn code(self, text: impl AsRef<str>) -> Self {
        self.styled(text, Style::CODE)
    }

    /// Appends text which links to a URL.
    #[must_use]
    pub fn link(mut self, text: impl AsRef<str>, url: impl Into<String>) -> Self {
        self.parts
            .push_kind(text.as_ref(), EntityKind::TextLink { url: url.into() });
        self
    }

    /// Appends a mention of a user who has no username.
    #[must_use]
    pub fn mention(mut self, text: impl AsRef<str>, user_id: i64) -> Self {
        self.parts
            .push_kind(text.as_ref(), EntityKind::TextMention { user_id });
        self
    }

    /// Appends an inline custom emoji sticker.
    #[must_use]
    pub fn custom_emoji(
        mut self,
        text: impl AsRef<str>,
        custom_emoji_id: impl Into<String>,
    ) -> Self {
        self.parts.push_kind(
            text.as_ref(),
            EntityKind::CustomEmoji {
                custom_emoji_id: custom_emoji_id.into(),
            },
        );
        self
    }

    /// Appends a preformatted block, optionally tagged with a language.
    ///
    /// Only available at the top level: Telegram does not permit `pre` to be
    /// nested inside other entities.
    #[must_use]
    pub fn pre(mut self, text: impl AsRef<str>, language: Option<&str>) -> Self {
        self.parts.push_kind(
            text.as_ref(),
            EntityKind::Pre {
                language: language.map(str::to_string),
            },
        );
        self
    }

    /// Appends a block quotation containing inline-formatted content.
    #[must_use]
    pub fn blockquote(mut self, build: impl FnOnce(InlineBuilder) -> InlineBuilder) -> Self {
        let inner: Parts = build(InlineBuilder::default()).into_parts();
        self.parts.push_block(inner, EntityKind::Blockquote);
        self
    }

    /// Appends a block quotation which is collapsed by default.
    #[must_use]
    pub fn expandable_blockquote(
        mut self,
        build: impl FnOnce(InlineBuilder) -> InlineBuilder,
    ) -> Self {
        let inner: Parts = build(InlineBuilder::default()).into_parts();
        self.parts
            .push_block(inner, EntityKind::ExpandableBlockquote);
        self
    }

    /// Attaches a photo, making the accumulated text its caption.
    #[must_use]
    pub fn photo(mut self, source: FileSource) -> Self {
        self.media = Some(Media::Photo(source));
        self
    }

    /// Attaches a document, making the accumulated text its caption.
    #[must_use]
    pub fn document(mut self, source: FileSource) -> Self {
        self.media = Some(Media::Document(source));
        self
    }

    /// Delivers the message silently, without a notification sound.
    #[must_use]
    pub const fn disable_notification(mut self, disable: bool) -> Self {
        self.options.disable_notification = disable;
        self
    }

    /// Prevents the message from being forwarded or saved.
    #[must_use]
    pub const fn protect_content(mut self, protect: bool) -> Self {
        self.options.protect_content = protect;
        self
    }

    /// Controls whether a link in the message generates a preview card.
    #[must_use]
    pub const fn disable_link_preview(mut self, disable: bool) -> Self {
        self.options.disable_link_preview = Some(disable);
        self
    }

    /// Targets a specific topic within a forum chat.
    #[must_use]
    pub const fn message_thread_id(mut self, thread_id: i64) -> Self {
        self.options.message_thread_id = Some(thread_id);
        self
    }

    /// Sends the message as a reply to an existing one.
    #[must_use]
    pub const fn reply_to_message_id(mut self, message_id: i64) -> Self {
        self.options.reply_to_message_id = Some(message_id);
        self
    }

    /// Finalizes the message.
    #[must_use]
    pub fn build(self) -> Message {
        Message {
            text: self.parts.text,
            entities: self.parts.entities,
            media: self.media,
            options: self.options,
        }
    }
}

/// Where the bytes of an uploaded photo or document come from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileSource {
    /// Raw bytes held in memory, uploaded as multipart form data.
    ///
    /// The natural fit for generated content — a rendered image which never
    /// touches disk.
    Bytes {
        /// The filename Telegram should display.
        filename: String,
        /// The file's contents.
        bytes: Vec<u8>,
    },
    /// An HTTPS URL which Telegram fetches itself, costing no upload bandwidth.
    Url(String),
    /// A file already uploaded to Telegram, re-sent by its identifier.
    FileId(String),
}

impl FileSource {
    /// Creates a [`FileSource::Bytes`].
    #[must_use]
    pub fn bytes(filename: impl Into<String>, bytes: impl Into<Vec<u8>>) -> Self {
        Self::Bytes {
            filename: filename.into(),
            bytes: bytes.into(),
        }
    }

    /// Creates a [`FileSource::Url`].
    #[must_use]
    pub fn url(url: impl Into<String>) -> Self {
        Self::Url(url.into())
    }

    /// Creates a [`FileSource::FileId`].
    #[must_use]
    pub fn file_id(file_id: impl Into<String>) -> Self {
        Self::FileId(file_id.into())
    }
}

/// An attachment carried alongside a message's caption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Media {
    /// A photo, displayed inline and re-encoded by Telegram.
    Photo(FileSource),
    /// A document, delivered as-is.
    Document(FileSource),
}

impl Media {
    /// The file this attachment carries.
    #[must_use]
    pub const fn source(&self) -> &FileSource {
        match self {
            Self::Photo(source) | Self::Document(source) => source,
        }
    }

    /// The Bot API method which sends this attachment.
    #[must_use]
    pub(crate) const fn api_method(&self) -> &'static str {
        match self {
            Self::Photo(_) => "sendPhoto",
            Self::Document(_) => "sendDocument",
        }
    }

    /// The request field name which carries this attachment.
    #[must_use]
    pub(crate) const fn field_name(&self) -> &'static str {
        match self {
            Self::Photo(_) => "photo",
            Self::Document(_) => "document",
        }
    }
}

/// Optional per-send parameters.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SendOptions {
    /// Deliver silently, without a notification sound.
    pub disable_notification: bool,
    /// Prevent forwarding and saving.
    pub protect_content: bool,
    /// Whether to suppress link preview cards. `None` leaves Telegram's default.
    pub disable_link_preview: Option<bool>,
    /// Target a specific topic within a forum chat.
    pub message_thread_id: Option<i64>,
    /// Send as a reply to an existing message.
    pub reply_to_message_id: Option<i64>,
}

/// A fully constructed message, ready to send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Message {
    pub(crate) text: String,
    pub(crate) entities: Vec<Entity>,
    pub(crate) media: Option<Media>,
    pub(crate) options: SendOptions,
}

impl Message {
    /// Starts building a message.
    #[must_use]
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }

    /// Creates an unformatted, single-span message.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            entities: Vec::new(),
            media: None,
            options: SendOptions::default(),
        }
    }

    /// The message's text, without any markup.
    #[must_use]
    pub fn as_text(&self) -> &str {
        &self.text
    }

    /// The message's formatting entities.
    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// The message's attachment, if any.
    #[must_use]
    pub const fn media(&self) -> Option<&Media> {
        self.media.as_ref()
    }

    /// The message's optional send parameters.
    #[must_use]
    pub const fn options(&self) -> &SendOptions {
        &self.options
    }
}

impl From<&str> for Message {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

impl From<String> for Message {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

impl From<&String> for Message {
    fn from(text: &String) -> Self {
        Self::text(text.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{FileSource, Media, Message};
    use crate::telegram::entity::{Entity, EntityKind, Style};

    #[test]
    fn plain_text_produces_no_entities() {
        let message: Message = Message::from("Game over!");

        let expected_text: &str = "Game over!";
        let actual_text: &str = message.as_text();
        assert_eq!(expected_text, actual_text);

        let expected_entities: Vec<Entity> = Vec::new();
        let actual_entities: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected_entities, actual_entities);
    }

    #[test]
    fn bold_span_gets_correct_offset_and_length() {
        let message: Message = Message::builder().text("Hi ").bold("there").build();

        let expected_text: &str = "Hi there";
        let actual_text: &str = message.as_text();
        assert_eq!(expected_text, actual_text);

        let expected: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 3, 5)];
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn offsets_account_for_surrogate_pairs() {
        // "🎨 " is 3 UTF-16 code units (2 for the emoji, 1 for the space),
        // even though it is only 2 Rust chars.
        let message: Message = Message::builder().text("🎨 ").bold("New").build();

        let expected: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 3, 3)];
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn combined_styles_emit_one_entity_each_over_the_same_span() {
        let message: Message = Message::builder()
            .styled("wow", Style::BOLD | Style::ITALIC)
            .build();

        let expected: Vec<Entity> = vec![
            Entity::new(EntityKind::Bold, 0, 3),
            Entity::new(EntityKind::Italic, 0, 3),
        ];
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn entity_length_excludes_trailing_whitespace() {
        // Telegram requires entities to be rtrimmed before their length is computed.
        let message: Message = Message::builder().bold("Header\n\t").text("body").build();

        let expected_text: &str = "Header\n\tbody";
        let actual_text: &str = message.as_text();
        assert_eq!(expected_text, actual_text);

        let expected: Vec<Entity> = vec![Entity::new(EntityKind::Bold, 0, 6)];
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn whitespace_only_span_produces_no_entity() {
        let message: Message = Message::builder().bold("   ").text("x").build();

        let expected: Vec<Entity> = Vec::new();
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn untrusted_text_is_never_escaped_or_altered() {
        // The entire point of the entities design: markup characters which
        // would break MarkdownV2 are just ordinary text here.
        let hostile: &str = r"*_[]()~`>#+-=|{}.!\ <b>&";
        let message: Message = Message::builder().code(hostile).build();

        let expected: String = hostile.to_string();
        let actual: String = message.as_text().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn link_carries_its_url() {
        let message: Message = Message::builder()
            .link("View", "https://example.com")
            .build();

        let expected: Vec<Entity> = vec![Entity::new(
            EntityKind::TextLink {
                url: String::from("https://example.com"),
            },
            0,
            4,
        )];
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn blockquote_wraps_its_inner_content_and_rebases_entities() {
        let message: Message = Message::builder()
            .text("before ")
            .blockquote(|b| b.text("note: ").italic("generated"))
            .build();

        let expected_text: &str = "before note: generated";
        let actual_text: &str = message.as_text();
        assert_eq!(expected_text, actual_text);

        let expected: Vec<Entity> = vec![
            Entity::new(EntityKind::Italic, 13, 9),
            Entity::new(EntityKind::Blockquote, 7, 15),
        ];
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn pre_records_its_language() {
        let message: Message = Message::builder().pre("let x = 1;", Some("rust")).build();

        let expected: Vec<Entity> = vec![Entity::new(
            EntityKind::Pre {
                language: Some(String::from("rust")),
            },
            0,
            10,
        )];
        let actual: Vec<Entity> = message.entities().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn photo_attaches_media_and_keeps_text_as_caption() {
        let message: Message = Message::builder()
            .bold("Pattern ready")
            .photo(FileSource::url("https://example.com/p.png"))
            .build();

        let expected_text: &str = "Pattern ready";
        let actual_text: &str = message.as_text();
        assert_eq!(expected_text, actual_text);

        let expected: Option<Media> = Some(Media::Photo(FileSource::Url(String::from(
            "https://example.com/p.png",
        ))));
        let actual: Option<Media> = message.media().cloned();
        assert_eq!(expected, actual);
    }

    #[test]
    fn send_options_round_trip() {
        let message: Message = Message::builder()
            .text("quiet")
            .disable_notification(true)
            .protect_content(true)
            .disable_link_preview(true)
            .message_thread_id(7)
            .reply_to_message_id(11)
            .build();

        assert!(message.options().disable_notification);
        assert!(message.options().protect_content);

        let expected_preview: Option<bool> = Some(true);
        let actual_preview: Option<bool> = message.options().disable_link_preview;
        assert_eq!(expected_preview, actual_preview);

        let expected_thread: Option<i64> = Some(7);
        let actual_thread: Option<i64> = message.options().message_thread_id;
        assert_eq!(expected_thread, actual_thread);

        let expected_reply: Option<i64> = Some(11);
        let actual_reply: Option<i64> = message.options().reply_to_message_id;
        assert_eq!(expected_reply, actual_reply);
    }
}
