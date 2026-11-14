use std::ops::{BitOr, BitOrAssign};

use serde::{Serialize, Serializer, ser::SerializeMap};

/// A set of inline text styles which can be combined.
///
/// Telegram permits entities to overlap, so combining styles simply emits one
/// entity per style over the same span. That is why this is a bit set rather
/// than a nesting API: `Style::BOLD | Style::ITALIC` needs no nesting at all.
///
/// Styles which carry data — links, mentions, preformatted blocks, custom
/// emoji — are not expressible as flags and have dedicated builder methods.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct Style(u8);

impl Style {
    /// No styling; the text is plain.
    pub const NONE: Self = Self(0);
    /// **Bold** text.
    pub const BOLD: Self = Self(1 << 0);
    /// *Italic* text.
    pub const ITALIC: Self = Self(1 << 1);
    /// Underlined text.
    pub const UNDERLINE: Self = Self(1 << 2);
    /// ~~Struck through~~ text.
    pub const STRIKETHROUGH: Self = Self(1 << 3);
    /// Text hidden behind a spoiler.
    pub const SPOILER: Self = Self(1 << 4);
    /// `Monospaced` inline code.
    pub const CODE: Self = Self(1 << 5);

    /// Every flag, paired with the entity it produces, in a stable order.
    const ALL: [(Self, EntityKind); 6] = [
        (Self::BOLD, EntityKind::Bold),
        (Self::ITALIC, EntityKind::Italic),
        (Self::UNDERLINE, EntityKind::Underline),
        (Self::STRIKETHROUGH, EntityKind::Strikethrough),
        (Self::SPOILER, EntityKind::Spoiler),
        (Self::CODE, EntityKind::Code),
    ];

    /// Whether every flag in `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        other.0 != 0 && (self.0 & other.0) == other.0
    }

    /// Whether no flags are set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Expands this set into the individual entity kinds it represents.
    #[must_use]
    pub(crate) fn kinds(self) -> Vec<EntityKind> {
        Self::ALL
            .iter()
            .filter(|(flag, _)| self.contains(*flag))
            .map(|(_, kind)| kind.clone())
            .collect()
    }
}

impl BitOr for Style {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        Self(self.0 | rhs.0)
    }
}

impl BitOrAssign for Style {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

/// The kind of a message entity, including any data it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntityKind {
    /// **Bold** text.
    Bold,
    /// *Italic* text.
    Italic,
    /// Underlined text.
    Underline,
    /// ~~Struck through~~ text.
    Strikethrough,
    /// Text hidden behind a spoiler.
    Spoiler,
    /// `Monospaced` inline code.
    Code,
    /// A preformatted block, optionally tagged with a language for highlighting.
    ///
    /// Telegram documents that this entity **cannot be nested inside other
    /// entities**, which is why the builder only exposes it at the top level.
    Pre {
        /// The programming language of the block's contents.
        language: Option<String>,
    },
    /// Text which links to a URL.
    TextLink {
        /// The URL to open.
        url: String,
    },
    /// Text which mentions a user who has no username.
    TextMention {
        /// The mentioned user's numeric id.
        user_id: i64,
    },
    /// An inline custom emoji sticker.
    CustomEmoji {
        /// The custom emoji's identifier.
        custom_emoji_id: String,
    },
    /// A block quotation.
    Blockquote,
    /// A block quotation which is collapsed by default.
    ExpandableBlockquote,
}

impl EntityKind {
    /// The value Telegram expects in the entity's `type` field.
    #[must_use]
    pub(crate) const fn wire_name(&self) -> &'static str {
        match self {
            Self::Bold => "bold",
            Self::Italic => "italic",
            Self::Underline => "underline",
            Self::Strikethrough => "strikethrough",
            Self::Spoiler => "spoiler",
            Self::Code => "code",
            Self::Pre { .. } => "pre",
            Self::TextLink { .. } => "text_link",
            Self::TextMention { .. } => "text_mention",
            Self::CustomEmoji { .. } => "custom_emoji",
            Self::Blockquote => "blockquote",
            Self::ExpandableBlockquote => "expandable_blockquote",
        }
    }
}

/// A styled span of a message, positioned in UTF-16 code units.
///
/// This is Telegram's canonical representation of formatting. Sending entities
/// directly — rather than a `parse_mode` string — means there is no markup
/// embedded in the text, and therefore nothing which ever needs escaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    /// What kind of formatting this span carries.
    pub kind: EntityKind,
    /// Offset of the span's start, in UTF-16 code units.
    pub offset: usize,
    /// Length of the span, in UTF-16 code units.
    pub length: usize,
}

impl Entity {
    /// Creates a new [`Entity`].
    #[must_use]
    pub const fn new(kind: EntityKind, offset: usize, length: usize) -> Self {
        Self {
            kind,
            offset,
            length,
        }
    }

    /// The exclusive end of this span, in UTF-16 code units.
    #[must_use]
    pub const fn end(&self) -> usize {
        self.offset + self.length
    }
}

impl Serialize for Entity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Telegram's wire format is flat: the discriminant is a `type` field
        // alongside the data fields, rather than a nested object.
        let extra_fields: usize = match &self.kind {
            EntityKind::Pre { language } => usize::from(language.is_some()),
            EntityKind::TextLink { .. }
            | EntityKind::TextMention { .. }
            | EntityKind::CustomEmoji { .. } => 1,
            _ => 0,
        };

        let mut map: S::SerializeMap = serializer.serialize_map(Some(3 + extra_fields))?;
        map.serialize_entry("type", self.kind.wire_name())?;
        map.serialize_entry("offset", &self.offset)?;
        map.serialize_entry("length", &self.length)?;

        match &self.kind {
            EntityKind::Pre {
                language: Some(language),
            } => map.serialize_entry("language", language)?,
            EntityKind::TextLink { url } => map.serialize_entry("url", url)?,
            EntityKind::TextMention { user_id } => {
                map.serialize_entry("user", &TextMentionUser { id: *user_id })?;
            }
            EntityKind::CustomEmoji { custom_emoji_id } => {
                map.serialize_entry("custom_emoji_id", custom_emoji_id)?;
            }
            _ => {}
        }

        map.end()
    }
}

/// Telegram expects `text_mention` to carry a nested `User` object.
#[derive(Serialize)]
struct TextMentionUser {
    id: i64,
}

#[cfg(test)]
mod tests {
    use super::{Entity, EntityKind, Style};

    #[test]
    fn combined_styles_contain_each_component() {
        let style: Style = Style::BOLD | Style::ITALIC;

        assert!(style.contains(Style::BOLD));
        assert!(style.contains(Style::ITALIC));
        assert!(!style.contains(Style::CODE));
    }

    #[test]
    fn empty_style_contains_nothing() {
        let style: Style = Style::NONE;

        assert!(style.is_empty());
        assert!(!style.contains(Style::BOLD));
    }

    #[test]
    fn combined_styles_expand_to_one_kind_each() {
        let expected: Vec<EntityKind> = vec![EntityKind::Bold, EntityKind::Italic];
        let actual: Vec<EntityKind> = (Style::BOLD | Style::ITALIC).kinds();
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_style_flag_expands() {
        let all: Style = Style::BOLD
            | Style::ITALIC
            | Style::UNDERLINE
            | Style::STRIKETHROUGH
            | Style::SPOILER
            | Style::CODE;

        let expected: usize = 6;
        let actual: usize = all.kinds().len();
        assert_eq!(expected, actual);
    }

    #[test]
    fn entity_end_is_offset_plus_length() {
        let entity: Entity = Entity::new(EntityKind::Bold, 5, 3);

        let expected: usize = 8;
        let actual: usize = entity.end();
        assert_eq!(expected, actual);
    }

    #[test]
    fn simple_entity_serializes_flat() {
        let expected: String = String::from(r#"{"type":"bold","offset":0,"length":4}"#);
        let actual: String = serde_json::to_string(&Entity::new(EntityKind::Bold, 0, 4))
            .expect("entity should serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn text_link_serializes_its_url() {
        let entity: Entity = Entity::new(
            EntityKind::TextLink {
                url: String::from("https://example.com"),
            },
            2,
            4,
        );

        let expected: String = String::from(
            r#"{"type":"text_link","offset":2,"length":4,"url":"https://example.com"}"#,
        );
        let actual: String = serde_json::to_string(&entity).expect("entity should serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn pre_omits_language_when_absent() {
        let entity: Entity = Entity::new(EntityKind::Pre { language: None }, 0, 5);

        let expected: String = String::from(r#"{"type":"pre","offset":0,"length":5}"#);
        let actual: String = serde_json::to_string(&entity).expect("entity should serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn pre_includes_language_when_present() {
        let entity: Entity = Entity::new(
            EntityKind::Pre {
                language: Some(String::from("rust")),
            },
            0,
            5,
        );

        let expected: String =
            String::from(r#"{"type":"pre","offset":0,"length":5,"language":"rust"}"#);
        let actual: String = serde_json::to_string(&entity).expect("entity should serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn text_mention_serializes_a_nested_user() {
        let entity: Entity = Entity::new(EntityKind::TextMention { user_id: 42 }, 0, 3);

        let expected: String =
            String::from(r#"{"type":"text_mention","offset":0,"length":3,"user":{"id":42}}"#);
        let actual: String = serde_json::to_string(&entity).expect("entity should serialize");
        assert_eq!(expected, actual);
    }
}
