//! RSS 2.0.
//!
//! The format with the widest reach and the loosest specification. Two things
//! it cannot do natively are supplied by namespaced extensions that every
//! reader already understands: `content:encoded` carries full post HTML, since
//! `<description>` is only nominally a summary, and `atom:link rel="self"` is
//! required by validators despite not being RSS at all.
//!
//! There is no `<managingEditor>` or `<webMaster>`: both demand a bare email
//! address in a document written for scrapers.

use chrono::{DateTime, Utc};
use quick_xml::Writer;
use quick_xml::events::{BytesDecl, BytesText, Event};

use super::error::FeedError;
use super::{Channel, PreparedEntry, RSS_PATH};

const FORMAT: &str = "RSS";

/// Renders the RSS document.
pub fn render(
    channel: &Channel,
    entries: &[PreparedEntry],
    last_build: DateTime<Utc>,
) -> Result<String, FeedError> {
    let mut writer: Writer<Vec<u8>> = Writer::new(Vec::new());
    write(&mut writer, channel, entries, last_build).map_err(|source| FeedError::Write {
        format: FORMAT,
        source,
    })?;
    String::from_utf8(writer.into_inner()).map_err(|_| FeedError::Encoding { format: FORMAT })
}

fn write(
    writer: &mut Writer<Vec<u8>>,
    channel: &Channel,
    entries: &[PreparedEntry],
    last_build: DateTime<Utc>,
) -> std::io::Result<()> {
    let self_url: String = format!("{}{RSS_PATH}", channel.base_url);

    writer.write_event(Event::Decl(BytesDecl::new("1.0", Some("utf-8"), None)))?;
    writer
        .create_element("rss")
        .with_attribute(("version", "2.0"))
        .with_attribute(("xmlns:atom", "http://www.w3.org/2005/Atom"))
        .with_attribute(("xmlns:content", "http://purl.org/rss/1.0/modules/content/"))
        .with_attribute(("xmlns:media", "http://search.yahoo.com/mrss/"))
        .write_inner_content(|rss| {
            rss.create_element("channel")
                .write_inner_content(|element| {
                    text(element, "title", &channel.title)?;
                    text(element, "link", &channel.home_url)?;
                    text(element, "description", &channel.description)?;
                    text(element, "language", &channel.language)?;
                    text(element, "copyright", &channel.copyright)?;
                    text(element, "lastBuildDate", &rfc2822(last_build))?;

                    element
                        .create_element("atom:link")
                        .with_attribute(("href", self_url.as_str()))
                        .with_attribute(("rel", "self"))
                        .with_attribute(("type", "application/rss+xml"))
                        .write_empty()?;

                    if let Some(icon) = &channel.icon_url {
                        element
                            .create_element("image")
                            .write_inner_content(|image| {
                                text(image, "url", icon)?;
                                text(image, "title", &channel.title)?;
                                text(image, "link", &channel.home_url)
                            })?;
                    }

                    for entry in entries {
                        element
                            .create_element("item")
                            .write_inner_content(|item| self::item(item, entry))?;
                    }

                    Ok(())
                })?;
            Ok(())
        })?;

    Ok(())
}

fn item(writer: &mut Writer<Vec<u8>>, entry: &PreparedEntry) -> std::io::Result<()> {
    text(writer, "title", &entry.title)?;
    text(writer, "link", &entry.url)?;

    // The canonical URL as a permalink: stable, unique, and already the id the
    // other two formats use, so a reader that follows more than one of them
    // does not show the post twice.
    writer
        .create_element("guid")
        .with_attribute(("isPermaLink", "true"))
        .write_text_content(BytesText::new(&entry.url))?;

    text(writer, "pubDate", &rfc2822(entry.published))?;
    text(writer, "description", &entry.summary)?;

    if let Some(content) = &entry.content_html {
        text(writer, "content:encoded", content)?;
    }

    for tag in &entry.tags {
        text(writer, "category", tag)?;
    }

    // `<enclosure>` is not used: it requires a byte length this library does
    // not have, and a wrong one makes readers refuse the attachment.
    if let Some(image) = &entry.image_url {
        writer
            .create_element("media:thumbnail")
            .with_attribute(("url", image.as_str()))
            .write_empty()?;
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

/// RFC 2822, which is what RSS's RFC 822 dates are in practice.
fn rfc2822(moment: DateTime<Utc>) -> String {
    moment.to_rfc2822()
}
