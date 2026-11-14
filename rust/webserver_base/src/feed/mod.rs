//! RSS 2.0, Atom 1.0 and JSON Feed 1.1 for a site's one content stream.
//!
//! A site gets exactly one feed, served in all three formats. Which entries it
//! holds is passed in rather than derived: a blog is a
//! [`dynamic_page_group`](crate::webserver::Pages::dynamic_page_group), and a
//! dynamic page builds its template data per request, so there is nothing to
//! harvest at boot.
//!
//! Three properties are load-bearing and easy to break:
//!
//! - **The documents are byte-stable.** `<lastBuildDate>` and `<updated>` come
//!   from the newest entry, never from the clock, and ties break on path. A
//!   timestamp or a map iteration order in the body would change the bytes on
//!   every deploy, and every subscriber would re-download a feed that did not
//!   change.
//! - **Content is rewritten, not copied.** See [`rewrite`].
//! - **Text is escaped by the writer.** `quick_xml::events::BytesText::new`
//!   escapes; nothing here hand-rolls it. What escaping cannot fix — characters
//!   outside XML's `Char` production — is stripped in [`sanitize`].

mod atom;
mod error;
mod json;
mod rewrite;
mod rss;
mod sanitize;

use chrono::{DateTime, Utc};
use tracing::{error, instrument};

pub use error::FeedError;

/// Where the RSS 2.0 document is served.
///
/// `/rss.xml` rather than `/feed.xml`: the latter names RSS on some sites and
/// Atom on others, so it is a path that has to be looked up to be understood.
pub const RSS_PATH: &str = "/rss.xml";

/// Where the Atom 1.0 document is served.
pub const ATOM_PATH: &str = "/atom.xml";

/// Where the JSON Feed 1.1 document is served.
pub const JSON_PATH: &str = "/feed.json";

/// How many entries a feed carries, newest first.
///
/// The complete inventory is `sitemap.xml`'s job; a feed is the recency signal,
/// and Google's own guidance for a feed-as-sitemap is that it holds only what
/// changed recently. Twenty is roughly a year of a personal blog's cadence.
pub const MAX_FEED_ENTRIES: usize = 20;

/// How long a feed may be served from a cache before revalidating.
///
/// Feeds are the most-polled document a blog serves. Thirty minutes is shorter
/// than every default poll interval that matters, so it costs no real freshness
/// while letting an intermediary absorb duplicate polls.
pub const FEED_MAX_AGE_SECONDS: u32 = 1800;

/// Channel-level metadata, all of it derived from what the site already
/// declares rather than configured a second time.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedSite {
    /// The site origin, without a trailing slash.
    pub base_url: String,
    /// Who wrote it.
    pub author: String,
    /// An RFC 5646 tag, e.g. `en-US`.
    pub language: String,
    /// Absolute URL of the large square icon, if the icon set produced one.
    pub icon_url: Option<String>,
    /// Absolute URL of the favicon, if one exists.
    pub favicon_url: Option<String>,
    /// The rights line, e.g. `© 1998–2026 Todd Everett Griffin`.
    pub copyright: String,
}

/// A site's one feed.
///
/// A named-field struct rather than a builder on purpose: a forgotten
/// `content_html` is a teaser feed nobody meant to ship, and here it has to be
/// typed as `None` to be omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Feed {
    /// What a reader shows in its subscription list. Prefer the page's
    /// `display_name` shape — `Blog | Todd Everett Griffin` — because a bare
    /// `Blog` is unidentifiable beside forty other subscriptions.
    pub title: String,
    /// One sentence describing the stream, not the site.
    pub description: String,
    /// The HTML page this feed mirrors, e.g. `/blog`.
    pub page_url: String,
    /// Any order; the feed is sorted newest-first and truncated to
    /// [`MAX_FEED_ENTRIES`].
    pub entries: Vec<FeedEntry>,
}

/// One item in a feed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedEntry {
    /// Site-relative, e.g. `/blog/rust-tips`. Becomes both the link and the
    /// permanent id, so changing it republishes the post to every subscriber.
    pub path: String,
    pub title: String,
    /// Required: an entry with no summary is a bare headline in every reader
    /// that shows previews.
    pub summary: String,
    /// The rendered post HTML. `None` ships a teaser.
    pub content_html: Option<String>,
    pub published: DateTime<Utc>,
    /// Atom requires a per-entry `<updated>`; this falls back to `published`.
    pub modified: Option<DateTime<Utc>>,
    pub tags: Vec<String>,
    /// The post's own card image, site-relative. Unlike an image inside the
    /// content, this one is *declared*, so an unresolved path fails the boot.
    pub image: Option<String>,
}

impl FeedEntry {
    /// `modified`, or `published` when the post was never revised.
    #[must_use]
    pub fn updated(&self) -> DateTime<Utc> {
        self.modified.unwrap_or(self.published)
    }
}

/// One rendered feed document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedDocument {
    path: &'static str,
    content_type: &'static str,
    body: String,
    etag: String,
}

impl FeedDocument {
    /// Where it is served.
    #[must_use]
    pub const fn path(&self) -> &'static str {
        self.path
    }

    /// Its `Content-Type`.
    #[must_use]
    pub const fn content_type(&self) -> &'static str {
        self.content_type
    }

    /// The document itself.
    #[must_use]
    pub fn body(&self) -> &str {
        &self.body
    }

    /// A strong validator over the body, quoted and ready for the header.
    #[must_use]
    pub fn etag(&self) -> &str {
        &self.etag
    }
}

/// All three documents, plus what they share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedSet {
    documents: Vec<FeedDocument>,
    last_modified: DateTime<Utc>,
}

impl FeedSet {
    /// Every document, in serving order.
    #[must_use]
    pub fn documents(&self) -> &[FeedDocument] {
        &self.documents
    }

    /// The newest entry's date, for `Last-Modified`.
    #[must_use]
    pub const fn last_modified(&self) -> DateTime<Utc> {
        self.last_modified
    }
}

/// Channel-level strings, already stripped of characters XML cannot hold.
///
/// Sanitizing at one boundary rather than at each use is deliberate: the bug
/// this prevents is *forgetting a field*, and a per-field call site is exactly
/// where that happens.
#[derive(Debug, Clone)]
struct Channel {
    base_url: String,
    title: String,
    description: String,
    author: String,
    copyright: String,
    language: String,
    home_url: String,
    icon_url: Option<String>,
    favicon_url: Option<String>,
}

/// An entry with its content already rewritten and its URLs already absolute,
/// which is all three serializers need.
#[derive(Debug, Clone)]
struct PreparedEntry {
    url: String,
    title: String,
    summary: String,
    content_html: Option<String>,
    published: DateTime<Utc>,
    updated: DateTime<Utc>,
    tags: Vec<String>,
    image_url: Option<String>,
}

/// Builds all three feed documents.
///
/// `resolve` maps a manifest path to its content-hashed one and returns `None`
/// for an asset the manifest does not know.
///
/// # Errors
///
/// [`FeedError`] for anything that makes the feed wrong rather than merely
/// degraded: an entry missing a title, an unrooted path, `srcset` in content,
/// an unresolved declared image, or a serialization failure.
#[instrument(skip_all)]
pub fn build_feeds<F>(site: &FeedSite, feed: &Feed, resolve: F) -> Result<FeedSet, FeedError>
where
    F: Fn(&str) -> Option<String>,
{
    for diagnostic in diagnostics(feed, Utc::now()) {
        // Boots and serves, but subscribers get something wrong and nothing
        // else will say so. `warn!` is never seen by anyone.
        error!("{diagnostic}");
    }

    let channel: Channel = channel(site, feed);
    let entries: Vec<PreparedEntry> = prepare(site, feed, &resolve)?;

    let last_modified: DateTime<Utc> = entries
        .iter()
        .map(|entry| entry.updated)
        .max()
        // An empty feed still needs a coherent `<updated>`; the site's own
        // epoch is meaningless, so fall back to the start of time rather than
        // to the clock, which would break byte-stability.
        .unwrap_or_else(|| DateTime::UNIX_EPOCH.to_utc());

    let documents: Vec<FeedDocument> = vec![
        document(
            RSS_PATH,
            "application/rss+xml; charset=utf-8",
            rss::render(&channel, &entries, last_modified)?,
        ),
        document(
            ATOM_PATH,
            "application/atom+xml; charset=utf-8",
            atom::render(&channel, &entries, last_modified)?,
        ),
        document(
            JSON_PATH,
            "application/feed+json",
            json::render(&channel, &entries)?,
        ),
    ];

    Ok(FeedSet {
        documents,
        last_modified,
    })
}

/// Sanitizes every channel-level string once, and resolves the home link.
fn channel(site: &FeedSite, feed: &Feed) -> Channel {
    let clean = |text: &str| -> String { sanitize::strip_illegal(text).0 };

    Channel {
        base_url: String::from(site.base_url.trim_end_matches('/')),
        title: clean(&feed.title),
        description: clean(&feed.description),
        author: clean(&site.author),
        copyright: clean(&site.copyright),
        language: clean(&site.language),
        home_url: absolute(&site.base_url, &feed.page_url),
        icon_url: site.icon_url.clone(),
        favicon_url: site.favicon_url.clone(),
    }
}

/// Wraps a rendered body with the validator the route will serve it under.
fn document(path: &'static str, content_type: &'static str, body: String) -> FeedDocument {
    let etag: String = format!("\"{:x}\"", md5::compute(body.as_bytes()));
    FeedDocument {
        path,
        content_type,
        body,
        etag,
    }
}

/// Sorts, truncates, validates and rewrites, in that order.
fn prepare<F>(site: &FeedSite, feed: &Feed, resolve: &F) -> Result<Vec<PreparedEntry>, FeedError>
where
    F: Fn(&str) -> Option<String>,
{
    let mut ordered: Vec<&FeedEntry> = feed.entries.iter().collect();
    // Path breaks ties so the bytes cannot depend on the caller's iteration
    // order — a `HashMap` upstream would otherwise change the ETag at random.
    ordered.sort_by(|left, right| {
        right
            .published
            .cmp(&left.published)
            .then_with(|| left.path.cmp(&right.path))
    });
    ordered.truncate(MAX_FEED_ENTRIES);

    let mut prepared: Vec<PreparedEntry> = Vec::with_capacity(ordered.len());
    let mut unresolved: Vec<String> = Vec::new();
    let mut stripped: usize = 0;

    for entry in ordered {
        validate_entry(entry)?;

        let url: String = absolute(&site.base_url, &entry.path);

        let image_url: Option<String> = match &entry.image {
            Some(image) if is_external(image) => Some(image.clone()),
            Some(image) => {
                let key: &str = image.trim_start_matches('/');
                let hashed: String = resolve(key).ok_or_else(|| FeedError::UnresolvedImage {
                    path: entry.path.clone(),
                    image: image.clone(),
                })?;
                Some(absolute(&site.base_url, &hashed))
            }
            None => None,
        };

        let (title, removed) = sanitize::strip_illegal(&entry.title);
        stripped += removed;
        let (summary, removed) = sanitize::strip_illegal(&entry.summary);
        stripped += removed;

        let mut tags: Vec<String> = Vec::with_capacity(entry.tags.len());
        for tag in &entry.tags {
            let (tag, removed) = sanitize::strip_illegal(tag);
            stripped += removed;
            tags.push(tag);
        }

        let content_html: Option<String> = match &entry.content_html {
            Some(content) => {
                let (content, removed) = sanitize::strip_illegal(content);
                stripped += removed;
                let rewritten: rewrite::Rewritten =
                    rewrite::rewrite_content(&content, &site.base_url, &url, resolve);
                unresolved.extend(rewritten.unresolved);
                Some(rewritten.html)
            }
            None => None,
        };

        prepared.push(PreparedEntry {
            url,
            title,
            summary,
            content_html,
            published: entry.published,
            updated: entry.updated(),
            tags,
            image_url,
        });
    }

    if stripped > 0 {
        error!(
            "stripped {stripped} character(s) that XML cannot represent from the feed; the \
             documents are valid, but something upstream is emitting control characters"
        );
    }
    for path in &unresolved {
        error!(
            "feed content references `{path}`, which is not in the asset manifest; it is a broken \
             image in every reader, and on the page itself"
        );
    }

    Ok(prepared)
}

/// The boot failures: an entry in this state produces output nobody wants.
fn validate_entry(entry: &FeedEntry) -> Result<(), FeedError> {
    if entry.path.trim().is_empty() {
        return Err(FeedError::IncompleteEntry {
            path: entry.path.clone(),
            field: "path",
        });
    }
    if !entry.path.starts_with('/') {
        return Err(FeedError::UnrootedPath {
            path: entry.path.clone(),
        });
    }
    if entry.title.trim().is_empty() {
        return Err(FeedError::IncompleteEntry {
            path: entry.path.clone(),
            field: "title",
        });
    }
    if entry.summary.trim().is_empty() {
        return Err(FeedError::IncompleteEntry {
            path: entry.path.clone(),
            field: "summary",
        });
    }
    if let Some(content) = &entry.content_html
        && rewrite::contains_srcset(content)
    {
        return Err(FeedError::SrcsetUnsupported {
            path: entry.path.clone(),
        });
    }
    Ok(())
}

/// The `error!`-tier findings: the feed serves, but a human should look.
///
/// Pure and clock-injected so the reporting itself is testable.
fn diagnostics(feed: &Feed, now: DateTime<Utc>) -> Vec<String> {
    let mut findings: Vec<String> = Vec::new();

    if feed.entries.is_empty() {
        findings.push(String::from(
            "the feed is configured but has no entries; subscribers get an empty document",
        ));
    }

    let mut seen: Vec<&str> = Vec::new();
    for entry in &feed.entries {
        if seen.contains(&entry.path.as_str()) {
            findings.push(format!(
                "feed entries share the path `{}`; readers deduplicate by id, so one of those \
                 posts never reaches a subscriber",
                entry.path
            ));
        } else {
            seen.push(&entry.path);
        }

        if entry.published > now {
            findings.push(format!(
                "feed entry `{}` is published in the future; it pins to the top of the feed and \
                 some readers hide it entirely",
                entry.path
            ));
        }

        if let Some(modified) = entry.modified
            && modified < entry.published
        {
            findings.push(format!(
                "feed entry `{}` was modified before it was published",
                entry.path
            ));
        }
    }

    findings
}

/// RFC 3339 to the second, with `Z` rather than `+00:00`.
///
/// Shared by Atom and JSON Feed so the two cannot disagree about an instant.
fn rfc3339(moment: DateTime<Utc>) -> String {
    moment.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

/// The MIME type an image URL implies, for the one place Atom wants it.
///
/// Extension-derived rather than probed: the file has already been proved to
/// exist, and re-reading its header bytes here would be a second source of
/// truth for something the manifest path already states.
fn mime_for(url: &str) -> Option<&'static str> {
    let extension: &str = url.rsplit('.').next()?;
    match extension.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "webp" => Some("image/webp"),
        "gif" => Some("image/gif"),
        "avif" => Some("image/avif"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

/// Whether a reference already names its own origin.
fn is_external(reference: &str) -> bool {
    reference.starts_with("http://") || reference.starts_with("https://")
}

/// Joins a site origin and a path, tolerating a slash on either side or both.
///
/// Deliberately the same rule as [`crate::sitemap`]: a second, subtly different
/// one is how a feed and a sitemap come to disagree about a URL.
fn absolute(base_url: &str, path: &str) -> String {
    if is_external(path) {
        return String::from(path);
    }
    let base: &str = base_url.trim_end_matches('/');
    let path: &str = path.trim_start_matches('/');
    if path.is_empty() {
        format!("{base}/")
    } else {
        format!("{base}/{path}")
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::Value;

    use super::{
        ATOM_PATH, Feed, FeedDocument, FeedEntry, FeedError, FeedSet, FeedSite, JSON_PATH,
        MAX_FEED_ENTRIES, RSS_PATH, build_feeds, diagnostics,
    };

    const BASE: &str = "https://www.example.com";

    fn moment(year: i32, month: u32, day: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, 12, 0, 0)
            .single()
            .expect("a real instant")
    }

    fn site() -> FeedSite {
        FeedSite {
            base_url: String::from(BASE),
            author: String::from("Todd Everett Griffin"),
            language: String::from("en-US"),
            icon_url: Some(format!("{BASE}/icon-512.png")),
            favicon_url: Some(format!("{BASE}/favicon.ico")),
            copyright: String::from("© 1998–2026 Todd Everett Griffin"),
        }
    }

    fn entry(slug: &str, published: DateTime<Utc>) -> FeedEntry {
        FeedEntry {
            path: format!("/blog/{slug}"),
            title: format!("Post {slug}"),
            summary: format!("A summary of {slug}."),
            content_html: Some(format!("<p>The body of {slug}.</p>")),
            published,
            modified: None,
            tags: vec![String::from("rust")],
            image: None,
        }
    }

    fn feed(entries: Vec<FeedEntry>) -> Feed {
        Feed {
            title: String::from("Blog | Todd Everett Griffin"),
            description: String::from("Writing on Rust and WebGPU."),
            page_url: String::from("/blog"),
            entries,
        }
    }

    fn resolver() -> impl Fn(&str) -> Option<String> {
        let mut manifest: BTreeMap<String, String> = BTreeMap::new();
        manifest.insert(
            String::from("static/image/blog/a.png"),
            String::from("static/image/blog/a.abc123.png"),
        );
        move |path: &str| manifest.get(path).cloned()
    }

    fn build(feed: &Feed) -> FeedSet {
        build_feeds(&site(), feed, resolver()).expect("builds")
    }

    fn body(set: &FeedSet, path: &str) -> String {
        set.documents()
            .iter()
            .find(|document| document.path() == path)
            .map_or_else(
                || panic!("no document at {path}"),
                |document| String::from(document.body()),
            )
    }

    // ---- well-formedness, proved by a parser that did not write the document

    #[test]
    fn every_xml_document_parses_under_a_strict_conformant_parser() {
        // Deliberately not quick-xml: it wrote these, and it is a non-validating
        // pull parser that accepts characters outside XML's `Char` production.
        // A generator checked by its own parser waves through exactly the bug
        // this test exists for.
        let set: FeedSet = build(&feed(vec![
            entry("first", moment(2024, 3, 10)),
            entry("second", moment(2026, 1, 2)),
        ]));

        for path in [RSS_PATH, ATOM_PATH] {
            let document: String = body(&set, path);
            roxmltree::Document::parse(&document)
                .unwrap_or_else(|error| panic!("{path} is not well-formed XML: {error}"));
        }
    }

    #[test]
    fn hostile_text_survives_a_round_trip_instead_of_breaking_the_document() {
        // Every character that has ever broken a hand-rolled feed, in one post.
        let mut hostile: FeedEntry = entry("hostile", moment(2026, 1, 2));
        hostile.title = String::from("Tom & Jerry <script> \"quoted\" 'single' ]]> &amp;");
        hostile.summary = String::from("5 < 6 && 7 > 6");
        hostile.content_html = Some(String::from("<p>a &amp; b ]]&gt; c</p>"));

        let set: FeedSet = build(&feed(vec![hostile]));

        for path in [RSS_PATH, ATOM_PATH] {
            let document: String = body(&set, path);
            roxmltree::Document::parse(&document)
                .unwrap_or_else(|error| panic!("{path} broke on hostile text: {error}"));
        }

        let channel: rss::Channel =
            rss::Channel::read_from(body(&set, RSS_PATH).as_bytes()).expect("valid RSS");
        let expected: &str = "Tom & Jerry <script> \"quoted\" 'single' ]]> &amp;";
        let actual: &str = channel.items()[0].title().expect("a title");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_control_character_is_stripped_rather_than_escaped_because_no_escape_exists() {
        let mut broken: FeedEntry = entry("broken", moment(2026, 1, 2));
        broken.title = String::from("before\u{0B}after");
        broken.tags = vec![String::from("ru\u{0C}st")];

        let set: FeedSet = build(&feed(vec![broken]));

        // roxmltree is the only oracle that catches this: quick-xml's reader
        // and both syndication crates parse it happily.
        for path in [RSS_PATH, ATOM_PATH] {
            let document: String = body(&set, path);
            roxmltree::Document::parse(&document)
                .unwrap_or_else(|error| panic!("{path} kept a non-XML character: {error}"));
        }

        assert!(body(&set, RSS_PATH).contains("beforeafter"));
        assert!(body(&set, RSS_PATH).contains("rust"));
    }

    // ---- spec conformance, proved by independent implementations

    #[test]
    fn the_rss_document_round_trips_through_an_independent_rss_implementation() {
        let set: FeedSet = build(&feed(vec![entry("first", moment(2024, 3, 10))]));
        let channel: rss::Channel =
            rss::Channel::read_from(body(&set, RSS_PATH).as_bytes()).expect("valid RSS 2.0");

        assert_eq!("Blog | Todd Everett Griffin", channel.title());
        assert_eq!("https://www.example.com/blog", channel.link());
        assert_eq!("Writing on Rust and WebGPU.", channel.description());
        assert_eq!(Some("en-US"), channel.language());
        assert_eq!(
            Some("© 1998–2026 Todd Everett Griffin"),
            channel.copyright()
        );

        let expected_items: usize = 1;
        let actual_items: usize = channel.items().len();
        assert_eq!(expected_items, actual_items);

        let item: &rss::Item = &channel.items()[0];
        assert_eq!(Some("Post first"), item.title());
        assert_eq!(Some("https://www.example.com/blog/first"), item.link());
        assert_eq!(Some("A summary of first."), item.description());
        assert_eq!(
            Some("https://www.example.com/blog/first"),
            item.guid().map(rss::Guid::value)
        );
        assert_eq!(Some(true), item.guid().map(rss::Guid::is_permalink));
        assert_eq!(Some("<p>The body of first.</p>"), item.content());
    }

    #[test]
    fn the_atom_document_round_trips_through_an_independent_atom_implementation() {
        let set: FeedSet = build(&feed(vec![entry("first", moment(2024, 3, 10))]));
        let parsed: atom_syndication::Feed =
            atom_syndication::Feed::read_from(body(&set, ATOM_PATH).as_bytes())
                .expect("valid Atom 1.0");

        assert_eq!("Blog | Todd Everett Griffin", parsed.title().as_str());
        assert_eq!("https://www.example.com/atom.xml", parsed.id());
        assert_eq!(
            Some("Writing on Rust and WebGPU."),
            parsed.subtitle().map(atom_syndication::Text::as_str)
        );
        assert_eq!(
            vec![String::from("Todd Everett Griffin")],
            parsed
                .authors()
                .iter()
                .map(|author| author.name().to_string())
                .collect::<Vec<String>>()
        );

        let entry: &atom_syndication::Entry = &parsed.entries()[0];
        assert_eq!("https://www.example.com/blog/first", entry.id());
        assert_eq!("Post first", entry.title().as_str());
        assert_eq!(
            Some("<p>The body of first.</p>"),
            entry.content().and_then(atom_syndication::Content::value)
        );
        assert_eq!(
            vec![String::from("rust")],
            entry
                .categories()
                .iter()
                .map(|category| category.term().to_string())
                .collect::<Vec<String>>()
        );
    }

    #[test]
    fn the_json_document_carries_every_field_the_1_1_spec_requires() {
        let set: FeedSet = build(&feed(vec![entry("first", moment(2024, 3, 10))]));
        let parsed: Value = serde_json::from_str(&body(&set, JSON_PATH)).expect("valid JSON");

        assert_eq!("https://jsonfeed.org/version/1.1", parsed["version"]);
        assert_eq!("Blog | Todd Everett Griffin", parsed["title"]);
        assert_eq!("https://www.example.com/blog", parsed["home_page_url"]);
        assert_eq!("https://www.example.com/feed.json", parsed["feed_url"]);
        assert_eq!("en-US", parsed["language"]);
        assert_eq!("Todd Everett Griffin", parsed["authors"][0]["name"]);

        let item: &Value = &parsed["items"][0];
        assert_eq!("https://www.example.com/blog/first", item["id"]);
        assert_eq!("https://www.example.com/blog/first", item["url"]);
        assert_eq!("Post first", item["title"]);
        assert_eq!("<p>The body of first.</p>", item["content_html"]);
        assert_eq!("2024-03-10T12:00:00Z", item["date_published"]);
    }

    // ---- the properties the caching design depends on

    #[test]
    fn building_the_same_feed_twice_produces_identical_bytes() {
        // This is what makes the ETag work. A timestamp anywhere in a body
        // would re-download the feed for every subscriber on every deploy, and
        // nothing else in the suite would notice.
        let declaration: Feed = feed(vec![
            entry("first", moment(2024, 3, 10)),
            entry("second", moment(2026, 1, 2)),
        ]);

        let expected: FeedSet = build(&declaration);
        let actual: FeedSet = build(&declaration);
        assert_eq!(expected, actual);
    }

    #[test]
    fn entries_sharing_an_instant_are_ordered_deterministically_not_by_caller_order() {
        // A `HashMap` upstream hands entries over in a different order each
        // run; without a tie-break the bytes — and so the ETag — would move.
        let same: DateTime<Utc> = moment(2026, 1, 2);
        let forward: FeedSet = build(&feed(vec![
            entry("alpha", same),
            entry("beta", same),
            entry("gamma", same),
        ]));
        let reversed: FeedSet = build(&feed(vec![
            entry("gamma", same),
            entry("beta", same),
            entry("alpha", same),
        ]));

        assert_eq!(forward, reversed);
    }

    #[test]
    fn the_feed_date_is_the_newest_entry_rather_than_the_clock() {
        let newest: DateTime<Utc> = moment(2026, 1, 2);
        let set: FeedSet = build(&feed(vec![
            entry("old", moment(2020, 1, 1)),
            entry("new", newest),
        ]));

        assert_eq!(newest, set.last_modified());
        assert!(body(&set, ATOM_PATH).contains("2026-01-02T12:00:00Z"));
    }

    #[test]
    fn each_document_gets_its_own_strong_validator() {
        let set: FeedSet = build(&feed(vec![entry("first", moment(2024, 3, 10))]));

        let etags: Vec<&str> = set.documents().iter().map(FeedDocument::etag).collect();

        let expected: usize = 3;
        let actual: usize = etags.len();
        assert_eq!(expected, actual);

        for etag in &etags {
            assert!(etag.starts_with('"') && etag.ends_with('"'), "{etag}");
        }
        assert_ne!(etags[0], etags[1]);
        assert_ne!(etags[1], etags[2]);
    }

    // ---- ordering and truncation

    #[test]
    fn the_newest_entries_are_kept_not_the_first_ones_supplied() {
        let entries: Vec<FeedEntry> = (1..=MAX_FEED_ENTRIES + 5)
            .map(|day| {
                entry(
                    &format!("post-{day:02}"),
                    Utc.with_ymd_and_hms(2026, 1, u32::try_from(day).expect("small"), 12, 0, 0)
                        .single()
                        .expect("a real instant"),
                )
            })
            .collect();

        let set: FeedSet = build(&feed(entries));
        let channel: rss::Channel =
            rss::Channel::read_from(body(&set, RSS_PATH).as_bytes()).expect("valid RSS");

        let expected_count: usize = MAX_FEED_ENTRIES;
        let actual_count: usize = channel.items().len();
        assert_eq!(expected_count, actual_count);

        // Newest first: day 25 down to day 6.
        assert_eq!(Some("Post post-25"), channel.items()[0].title());
        assert_eq!(
            Some("Post post-06"),
            channel.items()[MAX_FEED_ENTRIES - 1].title()
        );
    }

    #[test]
    fn a_post_never_revised_reports_its_publication_date_as_its_update() {
        let set: FeedSet = build(&feed(vec![entry("first", moment(2024, 3, 10))]));
        let parsed: atom_syndication::Feed =
            atom_syndication::Feed::read_from(body(&set, ATOM_PATH).as_bytes())
                .expect("valid Atom");

        let entry: &atom_syndication::Entry = &parsed.entries()[0];
        assert_eq!(
            entry.published().map(chrono::DateTime::to_rfc3339),
            Some(entry.updated().to_rfc3339())
        );
    }

    #[test]
    fn a_revision_date_reaches_the_documents_that_can_express_one() {
        let mut revised: FeedEntry = entry("first", moment(2024, 3, 10));
        revised.modified = Some(moment(2026, 1, 2));

        let set: FeedSet = build(&feed(vec![revised]));

        assert!(body(&set, ATOM_PATH).contains("<updated>2026-01-02T12:00:00Z</updated>"));
        assert!(body(&set, JSON_PATH).contains("\"date_modified\": \"2026-01-02T12:00:00Z\""));
    }

    // ---- content rewriting, end to end

    #[test]
    fn post_content_reaches_a_reader_with_urls_that_resolve_off_this_origin() {
        let mut illustrated: FeedEntry = entry("first", moment(2026, 1, 2));
        illustrated.content_html = Some(String::from(
            "<p><img src=\"/static/image/blog/a.png\"><a href=\"/blog/other\">more</a></p>",
        ));

        let set: FeedSet = build(&feed(vec![illustrated]));
        let channel: rss::Channel =
            rss::Channel::read_from(body(&set, RSS_PATH).as_bytes()).expect("valid RSS");

        let expected: &str = "<p><img src=\"https://www.example.com/static/image/blog/a.abc123.png\">\
             <a href=\"https://www.example.com/blog/other\">more</a></p>";
        let actual: &str = channel.items()[0].content().expect("content");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_declared_entry_image_is_hashed_absolutised_and_reaches_all_three_documents() {
        let mut illustrated: FeedEntry = entry("first", moment(2026, 1, 2));
        illustrated.image = Some(String::from("/static/image/blog/a.png"));

        let set: FeedSet = build(&feed(vec![illustrated]));
        let hashed: &str = "https://www.example.com/static/image/blog/a.abc123.png";

        assert!(body(&set, RSS_PATH).contains(hashed));
        assert!(body(&set, ATOM_PATH).contains(hashed));
        assert!(body(&set, JSON_PATH).contains(hashed));
        // Atom's enclosure needs a type; it is derived from the extension.
        assert!(body(&set, ATOM_PATH).contains("type=\"image/png\""));
    }

    // ---- boot failures

    #[test]
    fn an_entry_missing_a_title_stops_the_deploy_rather_than_shipping_a_blank_row() {
        let mut blank: FeedEntry = entry("first", moment(2026, 1, 2));
        blank.title = String::from("   ");

        let error: FeedError =
            build_feeds(&site(), &feed(vec![blank]), resolver()).expect_err("refuses");
        assert!(matches!(
            error,
            FeedError::IncompleteEntry { ref path, field } if path == "/blog/first" && field == "title"
        ));
    }

    #[test]
    fn an_entry_missing_a_summary_stops_the_deploy() {
        let mut blank: FeedEntry = entry("first", moment(2026, 1, 2));
        blank.summary = String::new();

        let error: FeedError =
            build_feeds(&site(), &feed(vec![blank]), resolver()).expect_err("refuses");
        assert!(matches!(
            error,
            FeedError::IncompleteEntry { field, .. } if field == "summary"
        ));
    }

    #[test]
    fn an_unrooted_path_is_refused_because_it_cannot_become_a_stable_id() {
        let mut floating: FeedEntry = entry("first", moment(2026, 1, 2));
        floating.path = String::from("blog/first");

        let error: FeedError =
            build_feeds(&site(), &feed(vec![floating]), resolver()).expect_err("refuses");
        assert!(matches!(
            error,
            FeedError::UnrootedPath { ref path } if path == "blog/first"
        ));
    }

    #[test]
    fn srcset_is_refused_rather_than_half_rewritten() {
        let mut responsive: FeedEntry = entry("first", moment(2026, 1, 2));
        responsive.content_html = Some(String::from(
            "<img srcset=\"/a.png 1x, /b.png 2x\" src=\"/a.png\">",
        ));

        let error: FeedError =
            build_feeds(&site(), &feed(vec![responsive]), resolver()).expect_err("refuses");
        assert!(matches!(
            error,
            FeedError::SrcsetUnsupported { ref path } if path == "/blog/first"
        ));
    }

    #[test]
    fn a_declared_image_absent_from_the_manifest_stops_the_deploy() {
        let mut illustrated: FeedEntry = entry("first", moment(2026, 1, 2));
        illustrated.image = Some(String::from("/static/image/blog/gone.png"));

        let error: FeedError =
            build_feeds(&site(), &feed(vec![illustrated]), resolver()).expect_err("refuses");
        assert!(matches!(
            error,
            FeedError::UnresolvedImage { ref image, .. }
                if image == "/static/image/blog/gone.png"
        ));
    }

    // ---- the `error!`-tier findings

    #[test]
    fn a_configured_feed_with_no_entries_is_reported() {
        let findings: Vec<String> = diagnostics(&feed(Vec::new()), moment(2026, 1, 2));

        let expected: usize = 1;
        let actual: usize = findings.len();
        assert_eq!(expected, actual);
        assert!(findings[0].contains("no entries"));
    }

    #[test]
    fn two_entries_sharing_a_path_are_reported_because_one_post_would_vanish() {
        let declaration: Feed = feed(vec![
            entry("first", moment(2024, 3, 10)),
            entry("first", moment(2026, 1, 2)),
        ]);

        let findings: Vec<String> = diagnostics(&declaration, moment(2026, 6, 1));

        let expected: usize = 1;
        let actual: usize = findings.len();
        assert_eq!(expected, actual);
        assert!(findings[0].contains("/blog/first"));
    }

    #[test]
    fn a_post_dated_in_the_future_is_reported_because_it_pins_to_the_top_forever() {
        let declaration: Feed = feed(vec![entry("first", moment(2027, 1, 1))]);
        let findings: Vec<String> = diagnostics(&declaration, moment(2026, 1, 2));

        let expected: usize = 1;
        let actual: usize = findings.len();
        assert_eq!(expected, actual);
        assert!(findings[0].contains("future"));
    }

    #[test]
    fn a_revision_predating_publication_is_reported() {
        let mut backwards: FeedEntry = entry("first", moment(2026, 1, 2));
        backwards.modified = Some(moment(2020, 1, 1));

        let findings: Vec<String> = diagnostics(&feed(vec![backwards]), moment(2026, 6, 1));

        let expected: usize = 1;
        let actual: usize = findings.len();
        assert_eq!(expected, actual);
        assert!(findings[0].contains("modified before"));
    }

    #[test]
    fn a_healthy_feed_reports_nothing() {
        let declaration: Feed = feed(vec![
            entry("first", moment(2024, 3, 10)),
            entry("second", moment(2026, 1, 2)),
        ]);

        let expected: Vec<String> = Vec::new();
        let actual: Vec<String> = diagnostics(&declaration, moment(2026, 6, 1));
        assert_eq!(expected, actual);
    }

    // ---- the shape of what gets served

    #[test]
    fn all_three_documents_are_produced_at_their_fixed_paths_with_their_own_media_types() {
        let set: FeedSet = build(&feed(vec![entry("first", moment(2026, 1, 2))]));

        let expected: Vec<(&str, &str)> = vec![
            (RSS_PATH, "application/rss+xml; charset=utf-8"),
            (ATOM_PATH, "application/atom+xml; charset=utf-8"),
            (JSON_PATH, "application/feed+json"),
        ];
        let actual: Vec<(&str, &str)> = set
            .documents()
            .iter()
            .map(|document| (document.path(), document.content_type()))
            .collect();
        assert_eq!(expected, actual);
    }
}
