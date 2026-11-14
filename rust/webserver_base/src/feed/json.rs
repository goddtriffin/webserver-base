//! JSON Feed 1.1.
//!
//! Built with `serde_json` rather than by hand so escaping and UTF-8 are the
//! serializer's problem. Nothing here needs the sanitizer's help — JSON can
//! represent every control character — but the strings arrive already cleaned,
//! because all three documents must describe the same posts.

use serde::Serialize;
use serde_json::Value;

use super::error::FeedError;
use super::{Channel, JSON_PATH, PreparedEntry};

/// The version URI is the spec's own identifier for 1.1, not a version string.
const VERSION: &str = "https://jsonfeed.org/version/1.1";

#[derive(Debug, Serialize)]
struct Document<'a> {
    version: &'static str,
    title: &'a str,
    home_page_url: &'a str,
    feed_url: String,
    description: &'a str,
    language: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    icon: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    favicon: Option<&'a String>,
    authors: Vec<Author<'a>>,
    items: Vec<Item<'a>>,
}

#[derive(Debug, Serialize)]
struct Author<'a> {
    name: &'a str,
}

#[derive(Debug, Serialize)]
struct Item<'a> {
    id: &'a str,
    url: &'a str,
    title: &'a str,
    summary: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    content_html: Option<&'a String>,
    date_published: String,
    date_modified: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    tags: Vec<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<&'a String>,
}

/// Renders the JSON Feed document.
pub fn render(channel: &Channel, entries: &[PreparedEntry]) -> Result<String, FeedError> {
    let document: Document<'_> = Document {
        version: VERSION,
        title: &channel.title,
        home_page_url: &channel.home_url,
        feed_url: format!("{}{JSON_PATH}", channel.base_url),
        description: &channel.description,
        language: &channel.language,
        icon: channel.icon_url.as_ref(),
        favicon: channel.favicon_url.as_ref(),
        authors: vec![Author {
            name: &channel.author,
        }],
        items: entries
            .iter()
            .map(|entry| Item {
                id: &entry.url,
                url: &entry.url,
                title: &entry.title,
                summary: &entry.summary,
                content_html: entry.content_html.as_ref(),
                date_published: super::rfc3339(entry.published),
                date_modified: super::rfc3339(entry.updated),
                tags: entry.tags.iter().map(String::as_str).collect(),
                image: entry.image_url.as_ref(),
            })
            .collect(),
    };

    let value: Value =
        serde_json::to_value(&document).map_err(|source| FeedError::Json { source })?;
    serde_json::to_string_pretty(&value).map_err(|source| FeedError::Json { source })
}
