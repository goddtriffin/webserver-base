//! Sitemap generation: always an index, always at least one url set.
//!
//! Which URLs a site has comes from [`Pages`](crate::webserver::Pages), so the
//! sitemap is derived from the route table rather than kept beside it.
//!
//! The shape is deliberately uniform. `/sitemap.xml` is a `<sitemapindex>` on a
//! three-page site and on a three-million-page one, so `robots.txt` and any
//! Search Console submission point at one URL forever. A site that grows past
//! the protocol's 50,000-URL limit simply gains another `/sitemap-N.xml` inside
//! the index; nothing outside the server changes, and nothing has to be
//! resubmitted. The alternative — a bare `<urlset>` that silently becomes an
//! index at the boundary — invalidates that submission at exactly the moment
//! the site matters most.

use chrono::{DateTime, Utc};
use sitemap_rs::image::Image;
use sitemap_rs::sitemap::Sitemap;
use sitemap_rs::sitemap_index::SitemapIndex;
use sitemap_rs::url::Url;
use sitemap_rs::url_builder::UrlBuilder;
use sitemap_rs::url_set::UrlSet;
use tracing::{debug, instrument};

/// The route the sitemap index is served at.
pub const SITEMAP_INDEX_PATH: &str = "/sitemap.xml";

/// URLs per url set. The protocol allows 50,000; the margin absorbs the
/// difference between an entry's typical and worst-case size.
pub const MAX_URLS_PER_SITEMAP: usize = 45_000;

/// Bytes per url set. The protocol allows 50 MB uncompressed. `sitemap-rs`
/// enforces only the URL count, so a set of image-heavy entries can pass that
/// check and still be rejected — this is the guard for that case.
pub const MAX_BYTES_PER_SITEMAP: usize = 45 * 1024 * 1024;

/// Why a sitemap could not be produced.
#[derive(Debug, thiserror::Error)]
pub enum SitemapError {
    /// A single `<url>` entry was rejected.
    #[error("invalid sitemap entry for `{location}`")]
    Url {
        location: String,
        #[source]
        source: sitemap_rs::url_error::UrlError,
    },

    /// A `<urlset>` was rejected.
    #[error("invalid sitemap url set ({count} urls)")]
    UrlSet {
        count: usize,
        #[source]
        source: sitemap_rs::url_set_error::UrlSetError,
    },

    /// The `<sitemapindex>` was rejected — more than 50,000 url sets, which is
    /// 2.25 billion URLs.
    #[error("invalid sitemap index ({count} sitemaps)")]
    Index {
        count: usize,
        #[source]
        source: sitemap_rs::sitemap_index_error::SitemapIndexError,
    },

    /// The XML could not be serialized.
    #[error("failed to serialize the sitemap")]
    Write {
        #[source]
        source: xml_builder::XMLError,
    },

    /// Serialized XML was not valid UTF-8, which cannot happen for input this
    /// crate produces.
    #[error("the serialized sitemap was not valid UTF-8")]
    Encoding,
}

/// One entry in a sitemap. Holds a site-relative path; the origin is prepended
/// at build time, so a URL cannot be listed under the wrong host.
///
/// Deliberately absent: `<changefreq>` and `<priority>`. Google ignores both —
/// priority is subjective and change frequency is guessed — so carrying them
/// would be markup with no consumer.
#[derive(Debug, Clone)]
pub struct SitemapUrl {
    path: String,
    last_modified: Option<DateTime<Utc>>,
    images: Vec<String>,
}

impl SitemapUrl {
    /// An entry for a site-relative `path`, e.g. `/blog/rust-tips`.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            last_modified: None,
            images: Vec::new(),
        }
    }

    /// Sets `<lastmod>` for this page specifically.
    ///
    /// Prefer a real modification date. Google uses `lastmod` only when it is
    /// "consistently and verifiably accurate" — it fetches the page and checks
    /// — and one bad pattern discredits the whole file, so a made-up date is
    /// worse than none.
    #[must_use]
    pub const fn with_last_modified(mut self, last_modified: DateTime<Utc>) -> Self {
        self.last_modified = Some(last_modified);
        self
    }

    /// Adds `<image:image>` entries, as site-relative or absolute URLs.
    #[must_use]
    pub fn extend_images<I, S>(mut self, images: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.images.extend(images.into_iter().map(Into::into));
        self
    }

    /// The site-relative path.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    /// The `<lastmod>` override, if one was set.
    #[must_use]
    pub const fn last_modified(&self) -> Option<DateTime<Utc>> {
        self.last_modified
    }

    /// The `<image:image>` entries, as supplied.
    #[must_use]
    pub fn images(&self) -> &[String] {
        &self.images
    }

    /// Rewrites every image path through `resolve`.
    ///
    /// Images are declared by their logical path but served only at their
    /// content-hashed one, so without this the sitemap advertises URLs that
    /// 404 — and an image sitemap full of dead links is worse than none.
    #[must_use]
    pub fn map_images<F>(mut self, resolve: F) -> Self
    where
        F: Fn(&str) -> String,
    {
        self.images = self.images.iter().map(|image| resolve(image)).collect();
        self
    }

    /// Builds the `sitemap-rs` entry, resolving `path` against `base_url`.
    fn build(
        &self,
        base_url: &str,
        fallback_last_modified: Option<DateTime<Utc>>,
    ) -> Result<Url, SitemapError> {
        let location: String = absolute(base_url, &self.path);

        let mut builder: UrlBuilder = Url::builder(location.clone());
        if let Some(last_modified) = self.last_modified.or(fallback_last_modified) {
            builder.last_modified(DateTime::from(last_modified));
        }

        if !self.images.is_empty() {
            builder.images(
                self.images
                    .iter()
                    .map(|image| Image::new(absolute(base_url, image)))
                    .collect(),
            );
        }

        builder
            .build()
            .map_err(|source| SitemapError::Url { location, source })
    }
}

/// A complete sitemap set: one index naming one or more url sets.
///
/// Held in memory and served from there. Nothing is written to disk, so a
/// production image needs no writable filesystem, and there is no generated
/// file to hash, cache, or accidentally commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SitemapSet {
    index: String,
    chunks: Vec<String>,
}

impl SitemapSet {
    /// The `<sitemapindex>` document, served at [`SITEMAP_INDEX_PATH`].
    #[must_use]
    pub fn index(&self) -> &str {
        &self.index
    }

    /// The `<urlset>` documents, in order. Chunk `n` is served at
    /// `/sitemap-{n+1}.xml`.
    #[must_use]
    pub fn chunks(&self) -> &[String] {
        &self.chunks
    }

    /// Every path this set is served at, index first.
    ///
    /// Index first is load-bearing for `robots.txt`: Google discards anything
    /// past 500 KiB, and truncation is positional, so the one line that must
    /// survive goes at the top.
    #[must_use]
    pub fn paths(&self) -> Vec<String> {
        let mut paths: Vec<String> = vec![String::from(SITEMAP_INDEX_PATH)];
        paths.extend((1..=self.chunks.len()).map(|n| format!("/sitemap-{n}.xml")));
        paths
    }
}

/// Builds the sitemap index and its url sets for `urls`.
///
/// `fallback_last_modified` is used for entries that name no date of their own;
/// pass `None` to omit `<lastmod>` rather than assert something unverifiable.
///
/// # Errors
///
/// [`SitemapError`] if an entry, a url set, or the index is invalid, or if the
/// XML cannot be serialized.
#[instrument(skip_all)]
pub fn build_sitemaps(
    base_url: &str,
    urls: &[SitemapUrl],
    fallback_last_modified: Option<DateTime<Utc>>,
) -> Result<SitemapSet, SitemapError> {
    let mut chunks: Vec<String> = Vec::new();

    // An empty site still gets one (empty) url set, so the index is never a
    // dangling reference and the served shape never varies.
    for batch in urls.chunks(MAX_URLS_PER_SITEMAP).chain(if urls.is_empty() {
        Some([].as_slice())
    } else {
        None
    }) {
        chunks.extend(render_url_set(base_url, batch, fallback_last_modified)?);
    }

    let sitemaps: Vec<Sitemap> = (1..=chunks.len())
        .map(|n| {
            Sitemap::new(
                absolute(base_url, &format!("/sitemap-{n}.xml")),
                fallback_last_modified.map(DateTime::from),
            )
        })
        .collect();

    let count: usize = sitemaps.len();
    let index: SitemapIndex =
        SitemapIndex::new(sitemaps).map_err(|source| SitemapError::Index { count, source })?;

    let mut buffer: Vec<u8> = Vec::new();
    index
        .write(&mut buffer)
        .map_err(|source| SitemapError::Write { source })?;
    let index: String = String::from_utf8(buffer).map_err(|_| SitemapError::Encoding)?;

    debug!("built {} url(s) across {count} sitemap(s)", urls.len());
    Ok(SitemapSet { index, chunks })
}

/// Serializes one batch, splitting it further if the result exceeds the
/// protocol's byte limit.
fn render_url_set(
    base_url: &str,
    batch: &[SitemapUrl],
    fallback_last_modified: Option<DateTime<Utc>>,
) -> Result<Vec<String>, SitemapError> {
    let built: Vec<Url> = batch
        .iter()
        .map(|url| url.build(base_url, fallback_last_modified))
        .collect::<Result<Vec<Url>, SitemapError>>()?;

    let count: usize = built.len();
    let url_set: UrlSet =
        UrlSet::new(built).map_err(|source| SitemapError::UrlSet { count, source })?;

    let mut buffer: Vec<u8> = Vec::new();
    url_set
        .write(&mut buffer)
        .map_err(|source| SitemapError::Write { source })?;

    if buffer.len() <= MAX_BYTES_PER_SITEMAP || batch.len() < 2 {
        return Ok(vec![
            String::from_utf8(buffer).map_err(|_| SitemapError::Encoding)?,
        ]);
    }

    // Too large despite an acceptable URL count — image-heavy entries. Halve
    // and retry; each half is re-measured, so this terminates.
    let (left, right) = batch.split_at(batch.len() / 2);
    let mut rendered: Vec<String> = render_url_set(base_url, left, fallback_last_modified)?;
    rendered.extend(render_url_set(base_url, right, fallback_last_modified)?);
    Ok(rendered)
}

/// Joins a site origin and a path, tolerating a slash on either side or both.
fn absolute(base_url: &str, path: &str) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
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
    use chrono::{DateTime, TimeZone, Utc};

    use super::{SitemapSet, SitemapUrl, absolute, build_sitemaps};

    #[test]
    fn joining_never_doubles_or_drops_a_slash() {
        let cases: [(&str, &str, &str); 5] = [
            ("https://a.com", "/blog", "https://a.com/blog"),
            ("https://a.com/", "/blog", "https://a.com/blog"),
            ("https://a.com", "blog", "https://a.com/blog"),
            ("https://a.com", "/", "https://a.com/"),
            ("https://a.com", "", "https://a.com/"),
        ];
        for (base, path, expected) in cases {
            let actual: String = absolute(base, path);
            assert_eq!(expected, actual, "base {base:?} path {path:?}");
        }
    }

    #[test]
    fn an_absolute_image_url_is_left_alone() {
        let expected: String = String::from("https://cdn.example.com/card.png");
        let actual: String = absolute("https://a.com", "https://cdn.example.com/card.png");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_small_site_still_gets_an_index_so_the_shape_never_changes() {
        let urls: Vec<SitemapUrl> = vec![SitemapUrl::new("/"), SitemapUrl::new("/blog")];
        let set: SitemapSet =
            build_sitemaps("https://www.example.com", &urls, None).expect("builds");

        assert!(set.index().contains("<sitemapindex"));
        assert!(
            set.index()
                .contains("https://www.example.com/sitemap-1.xml")
        );

        let expected: usize = 1;
        let actual: usize = set.chunks().len();
        assert_eq!(expected, actual);

        assert!(set.chunks()[0].contains("<loc>https://www.example.com/</loc>"));
        assert!(set.chunks()[0].contains("<loc>https://www.example.com/blog</loc>"));
    }

    #[test]
    fn a_site_with_no_pages_still_produces_a_well_formed_pair() {
        let set: SitemapSet = build_sitemaps("https://www.example.com", &[], None).expect("builds");

        let expected: usize = 1;
        let actual: usize = set.chunks().len();
        assert_eq!(expected, actual);
        assert!(set.index().contains("<sitemapindex"));
    }

    #[test]
    fn urls_beyond_the_limit_spill_into_another_url_set() {
        let urls: Vec<SitemapUrl> = (0..super::MAX_URLS_PER_SITEMAP + 10)
            .map(|n| SitemapUrl::new(format!("/page/{n}")))
            .collect();
        let set: SitemapSet =
            build_sitemaps("https://www.example.com", &urls, None).expect("builds");

        let expected: usize = 2;
        let actual: usize = set.chunks().len();
        assert_eq!(expected, actual);
        assert!(
            set.index()
                .contains("https://www.example.com/sitemap-2.xml")
        );
    }

    #[test]
    fn the_index_path_comes_first_so_it_survives_truncation() {
        let urls: Vec<SitemapUrl> = (0..=super::MAX_URLS_PER_SITEMAP)
            .map(|n| SitemapUrl::new(format!("/page/{n}")))
            .collect();
        let set: SitemapSet =
            build_sitemaps("https://www.example.com", &urls, None).expect("builds");

        let expected: Vec<String> = vec![
            String::from("/sitemap.xml"),
            String::from("/sitemap-1.xml"),
            String::from("/sitemap-2.xml"),
        ];
        let actual: Vec<String> = set.paths();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_page_without_a_date_gets_no_lastmod_rather_than_a_false_one() {
        let urls: Vec<SitemapUrl> = vec![SitemapUrl::new("/")];
        let set: SitemapSet =
            build_sitemaps("https://www.example.com", &urls, None).expect("builds");

        assert!(!set.chunks()[0].contains("<lastmod>"));
    }

    #[test]
    fn a_page_date_wins_over_the_site_wide_fallback() {
        let page_date: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        let site_date: DateTime<Utc> = Utc.with_ymd_and_hms(2020, 1, 1, 0, 0, 0).unwrap();

        let urls: Vec<SitemapUrl> = vec![
            SitemapUrl::new("/blog").with_last_modified(page_date),
            SitemapUrl::new("/"),
        ];
        let set: SitemapSet =
            build_sitemaps("https://www.example.com", &urls, Some(site_date)).expect("builds");

        assert!(set.chunks()[0].contains("2026-01-02"));
        assert!(set.chunks()[0].contains("2020-01-01"));
    }

    #[test]
    fn images_are_rewritten_to_the_paths_they_are_actually_served_at() {
        let url: SitemapUrl = SitemapUrl::new("/")
            .extend_images(["/static/image/social/card.webp"])
            .map_images(|image| image.replace("card.webp", "card.abc123.webp"));

        let expected: Vec<String> = vec![String::from("/static/image/social/card.abc123.webp")];
        let actual: Vec<String> = url.images().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn images_are_resolved_against_the_site_origin() {
        let urls: Vec<SitemapUrl> =
            vec![SitemapUrl::new("/").extend_images(["/static/image/social/card.webp"])];
        let set: SitemapSet =
            build_sitemaps("https://www.example.com", &urls, None).expect("builds");

        assert!(set.chunks()[0].contains("https://www.example.com/static/image/social/card.webp"));
    }
}
