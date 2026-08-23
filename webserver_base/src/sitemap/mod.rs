//! `sitemap.xml` generation.
//!
//! Which URLs a site has comes from [`Pages`](crate::webserver::Pages), so the
//! sitemap is derived from the route table rather than kept beside it.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use sitemap_rs::image::Image;
use sitemap_rs::url::{ChangeFrequency, DEFAULT_PRIORITY, Url};
use sitemap_rs::url_builder::UrlBuilder;
use sitemap_rs::url_set::UrlSet;
use tracing::{debug, instrument};

/// Where the sitemap is written, relative to the working directory.
pub const SITEMAP_OUTPUT_PATH: &str = "static/file/sitemap.xml";

/// The change frequency a URL gets when it does not name one.
pub const DEFAULT_CHANGE_FREQUENCY: ChangeFrequency = ChangeFrequency::Weekly;

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

    /// The `<urlset>` was rejected — usually the 50,000-URL limit.
    #[error("invalid sitemap url set ({count} urls)")]
    UrlSet {
        count: usize,
        #[source]
        source: sitemap_rs::url_set_error::UrlSetError,
    },

    /// The output file could not be created.
    #[error("failed to create sitemap at `{path}`")]
    Create {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The XML could not be written.
    #[error("failed to write sitemap to `{path}`")]
    Write {
        path: PathBuf,
        #[source]
        source: xml_builder::XMLError,
    },
}

/// One entry in the sitemap. Holds a site-relative path; the origin is
/// prepended at write time, so a URL cannot be listed under the wrong host.
#[derive(Debug, Clone)]
pub struct SitemapUrl {
    path: String,
    last_modified: Option<DateTime<Utc>>,
    change_frequency: ChangeFrequency,
    priority: f32,
    images: Vec<String>,
}

impl SitemapUrl {
    /// An entry for a site-relative `path`, e.g. `/blog/rust-tips`.
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            last_modified: None,
            change_frequency: DEFAULT_CHANGE_FREQUENCY,
            priority: DEFAULT_PRIORITY,
            images: Vec::new(),
        }
    }

    /// Sets `<lastmod>`. Defaults to the moment the sitemap is written.
    #[must_use]
    pub const fn with_last_modified(mut self, last_modified: DateTime<Utc>) -> Self {
        self.last_modified = Some(last_modified);
        self
    }

    /// Sets `<changefreq>`.
    #[must_use]
    pub const fn with_change_frequency(mut self, change_frequency: ChangeFrequency) -> Self {
        self.change_frequency = change_frequency;
        self
    }

    /// Sets `<priority>`.
    #[must_use]
    pub const fn with_priority(mut self, priority: f32) -> Self {
        self.priority = priority;
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

    /// The `<changefreq>` this entry will be written with.
    #[must_use]
    pub const fn change_frequency(&self) -> ChangeFrequency {
        self.change_frequency
    }

    /// The `<priority>` this entry will be written with.
    #[must_use]
    pub const fn priority(&self) -> f32 {
        self.priority
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

    /// Builds the `sitemap-rs` entry, resolving `path` against `base_url`.
    fn build(&self, base_url: &str, now: DateTime<Utc>) -> Result<Url, SitemapError> {
        let location: String = absolute(base_url, &self.path);

        let mut builder: UrlBuilder = Url::builder(location.clone());
        builder
            .last_modified(DateTime::from(self.last_modified.unwrap_or(now)))
            .change_frequency(self.change_frequency)
            .priority(self.priority);

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

/// Writes a sitemap for `urls` to `output_path`, creating parent directories.
///
/// # Errors
///
/// [`SitemapError`] if an entry or the set is invalid, or the write fails.
#[instrument(skip_all)]
pub fn write_sitemap(
    base_url: &str,
    urls: &[SitemapUrl],
    output_path: impl AsRef<Path>,
) -> Result<(), SitemapError> {
    let output_path: &Path = output_path.as_ref();
    let now: DateTime<Utc> = Utc::now();

    let built: Vec<Url> = urls
        .iter()
        .map(|url| url.build(base_url, now))
        .collect::<Result<Vec<Url>, SitemapError>>()?;

    let count: usize = built.len();
    let url_set: UrlSet =
        UrlSet::new(built).map_err(|source| SitemapError::UrlSet { count, source })?;

    if let Some(parent) = output_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|source| SitemapError::Create {
            path: parent.to_path_buf(),
            source,
        })?;
    }

    let file: File = File::create(output_path).map_err(|source| SitemapError::Create {
        path: output_path.to_path_buf(),
        source,
    })?;

    url_set
        .write(BufWriter::new(file))
        .map_err(|source| SitemapError::Write {
            path: output_path.to_path_buf(),
            source,
        })?;

    debug!("wrote {count} urls to `{}`", output_path.display());
    Ok(())
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
    use std::fs;
    use std::path::PathBuf;

    use chrono::{TimeZone, Utc};
    use sitemap_rs::url::ChangeFrequency;

    use super::{SitemapUrl, absolute, write_sitemap};

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
    fn a_written_sitemap_contains_every_url_and_its_metadata() {
        let output: PathBuf = PathBuf::from("/tmp/wsb-sitemap-test/static/file/sitemap.xml");
        let urls: Vec<SitemapUrl> = vec![
            SitemapUrl::new("/")
                .with_last_modified(Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap())
                .extend_images(["/static/image/social/card.webp"]),
            SitemapUrl::new("/blog").with_change_frequency(ChangeFrequency::Monthly),
        ];

        write_sitemap("https://www.example.com", &urls, &output).expect("writes");

        let actual: String = fs::read_to_string(&output).expect("written");
        assert!(actual.contains("<loc>https://www.example.com/</loc>"));
        assert!(actual.contains("<loc>https://www.example.com/blog</loc>"));
        assert!(actual.contains("<changefreq>monthly</changefreq>"));
        assert!(actual.contains("2026-01-02"));
        assert!(actual.contains("https://www.example.com/static/image/social/card.webp"));

        fs::remove_dir_all("/tmp/wsb-sitemap-test").ok();
    }

    #[test]
    fn the_path_is_kept_site_relative_until_write_time() {
        let url: SitemapUrl = SitemapUrl::new("/blog");

        let expected: String = String::from("/blog");
        let actual: String = url.path().to_string();
        assert_eq!(expected, actual);
    }
}
