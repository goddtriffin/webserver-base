//! Atom 1.0, per RFC 4287.
//!
//! The richest of the three, and the one `robots.txt` advertises as a sitemap:
//! it is the only format with a genuine per-entry `<updated>`, which is what a
//! crawler reads to decide a post actually changed. RSS carries publication
//! dates only, so a revised post looks untouched.

use chrono::{DateTime, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};

use super::error::FeedError;
use super::{ATOM_PATH, Channel, PreparedEntry, rfc3339};

const FORMAT: &str = "Atom";

/// Renders the Atom document.
pub fn render(
    channel: &Channel,
    entries: &[PreparedEntry],
    updated: DateTime<Utc>,
) -> Result<String, FeedError> {
    let mut writer: Writer<Vec<u8>> = Writer::new(Vec::new());
    write(&mut writer, channel, entries, updated).map_err(|source| FeedError::Write {
        format: FORMAT,
        source,
    })?;
    String::from_utf8(writer.into_inner()).map_err(|_| FeedError::Encoding { format: FORMAT })
}

fn write(
    writer: &mut Writer<Vec<u8>>,
    channel: &Channel,
    entries: &[PreparedEntry],
    updated: DateTime<Utc>,
) -> std::io::Result<()> {
    let self_url: String = format!("{}{ATOM_PATH}", channel.base_url);

    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;
    writer
        .create_element("feed")
        .with_attribute(("xmlns", "http://www.w3.org/2005/Atom"))
        .with_attribute(("xml:lang", channel.language.as_str()))
        .write_inner_content(|feed| {
            // The id must be a permanent IRI and must never change: it is how
            // a reader decides this is the same feed it already follows.
            text(feed, "id", &self_url)?;
            text(feed, "title", &channel.title)?;
            text(feed, "subtitle", &channel.description)?;
            text(feed, "updated", &rfc3339(updated))?;

            link(feed, "self", "application/atom+xml", &self_url)?;
            link(feed, "alternate", "text/html", &channel.home_url)?;

            feed.create_element("author")
                .write_inner_content(|author| text(author, "name", &channel.author))?;

            text(feed, "rights", &channel.copyright)?;

            if let Some(icon) = &channel.favicon_url {
                text(feed, "icon", icon)?;
            }
            if let Some(logo) = &channel.icon_url {
                text(feed, "logo", logo)?;
            }

            for entry in entries {
                feed.create_element("entry")
                    .write_inner_content(|element| self::entry(element, entry))?;
            }

            Ok(())
        })?;

    Ok(())
}

fn entry(writer: &mut Writer<Vec<u8>>, entry: &PreparedEntry) -> std::io::Result<()> {
    text(writer, "id", &entry.url)?;
    text(writer, "title", &entry.title)?;
    text(writer, "updated", &rfc3339(entry.updated))?;
    text(writer, "published", &rfc3339(entry.published))?;
    link(writer, "alternate", "text/html", &entry.url)?;

    writer
        .create_element("summary")
        .with_attribute(("type", "html"))
        .write_text_content(BytesText::new(&entry.summary))?;

    if let Some(content) = &entry.content_html {
        writer
            .create_element("content")
            .with_attribute(("type", "html"))
            .write_text_content(BytesText::new(content))?;
    }

    for tag in &entry.tags {
        writer
            .create_element("category")
            .with_attribute(("term", tag.as_str()))
            .write_empty()?;
    }

    if let Some(image) = &entry.image_url {
        let mut element = writer
            .create_element("link")
            .with_attribute(("rel", "enclosure"))
            .with_attribute(("href", image.as_str()));
        if let Some(mime) = super::mime_for(image) {
            element = element.with_attribute(("type", mime));
        }
        element.write_empty()?;
    }

    Ok(())
}

/// A text-only element.
fn text(writer: &mut Writer<Vec<u8>>, name: &str, value: &str) -> std::io::Result<()> {
    writer
        .create_element(name)
        .write_text_content(BytesText::new(value))?;
    Ok(())
}

/// A self-closing `<link>`.
fn link(
    writer: &mut Writer<Vec<u8>>,
    relation: &str,
    kind: &str,
    href: &str,
) -> std::io::Result<()> {
    writer
        .create_element("link")
        .with_attribute(("rel", relation))
        .with_attribute(("type", kind))
        .with_attribute(("href", href))
        .write_empty()?;
    Ok(())
}
