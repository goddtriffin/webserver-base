//! The runtime half of cache busting: read the manifest, set the headers.
//!
//! Nothing here touches the filesystem beyond one read at boot. Hashing is a
//! build step — see [`generate`](super::generate) — so the server can run on a
//! read-only image, and a restart cannot re-hash already-hashed files.

use std::collections::BTreeMap;
use std::fmt::{self, Display, Formatter};
use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::extract::Request;
use axum::http::header::{
    CACHE_CONTROL, ETAG, EXPIRES, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_RANGE,
    IF_UNMODIFIED_SINCE, PRAGMA,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::middleware::Next;
use axum::response::Response;
use chrono::{DateTime, Duration, TimeDelta, Utc};
use tracing::instrument;

use super::error::CacheBusterError;
use super::generate::STATIC_DIRECTORY;
use super::manifest::{MANIFEST_PATH, Manifest};

/// Resolves a logical asset path to its content-hashed one.
#[derive(Debug, Clone, Default)]
pub struct CacheBuster {
    manifest: Manifest,
}

impl CacheBuster {
    /// A cache buster that knows about nothing, for a server with no static
    /// assets at all.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// Loads the manifest the build produced.
    ///
    /// A project with no `static/` gets an empty map and serves no `/static`
    /// route. A project *with* `static/` but no manifest is a build that never
    /// ran its asset step: every hashed URL would 404, so that is an error
    /// rather than a silent degradation.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError::MissingManifest`] if there are assets but no
    /// manifest, or [`CacheBusterError::ParseManifest`] if it is malformed.
    #[instrument(skip_all)]
    pub fn load() -> Result<Self, CacheBusterError> {
        if !Path::new(STATIC_DIRECTORY).is_dir() {
            return Ok(Self::empty());
        }
        if !Path::new(MANIFEST_PATH).is_file() {
            return Err(CacheBusterError::MissingManifest {
                path: PathBuf::from(MANIFEST_PATH),
            });
        }
        Ok(Self {
            manifest: Manifest::load()?,
        })
    }

    /// The hashed path for `original`, or `original` itself when it is not a
    /// hashed asset.
    ///
    /// Never fails: falling back is better than taking a page down over one
    /// image.
    #[must_use]
    pub fn get_file(&self, original: &str) -> String {
        self.manifest.resolve(original).to_string()
    }

    /// Whether `original` is a known hashed asset.
    ///
    /// The distinction [`get_file`](Self::get_file) cannot make: it returns the
    /// input unchanged for anything it does not know, which is right for an
    /// absolute URL and wrong for a typo. Boot-time validation needs to tell
    /// those apart.
    #[must_use]
    pub fn is_hashed(&self, original: &str) -> bool {
        self.manifest.contains(original)
    }

    /// The manifest itself.
    #[must_use]
    pub const fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// The whole map, as the templates see it.
    #[must_use]
    pub const fn cache(&self) -> &BTreeMap<String, String> {
        self.manifest.entries()
    }

    /// Whether any asset is hashed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.manifest.is_empty()
    }

    /// Marks a response as never cacheable.
    ///
    /// For everything whose URL does *not* carry a content hash: pages, API
    /// responses, `robots.txt`. Conditional-request headers are stripped as
    /// well, so an intermediary cannot revalidate its way back to a stale copy.
    ///
    /// # Errors
    ///
    /// Never; the signature matches axum's middleware shape.
    #[instrument(skip_all)]
    pub async fn never_cache_middleware(
        request: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        let mut response: Response<Body> = next.run(request).await;
        let headers: &mut HeaderMap = response.headers_mut();

        remove_conditional_headers(headers);
        headers.insert(
            EXPIRES,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"),
        );
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate, private, max-age=0"),
        );
        headers.insert(PRAGMA, HeaderValue::from_static("no-cache"));

        Ok(response)
    }

    /// Marks a response as immutable for a year.
    ///
    /// Only ever correct for content-hashed URLs, where a changed file is a
    /// changed URL by construction. That invariant is why every byte under
    /// `/static` is hashed, including the well-known icons.
    ///
    /// # Errors
    ///
    /// Never; the signature matches axum's middleware shape.
    #[instrument(skip_all)]
    pub async fn forever_cache_middleware(
        request: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        let mut response: Response<Body> = next.run(request).await;
        let headers: &mut HeaderMap = response.headers_mut();

        remove_conditional_headers(headers);

        let one_year: TimeDelta = Duration::days(365);
        let expires: DateTime<Utc> = Utc::now() + one_year;
        if let Ok(expires) = HeaderValue::from_str(&expires.to_rfc2822()) {
            headers.insert(EXPIRES, expires);
        }
        headers.insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, must-revalidate, immutable"),
        );

        Ok(response)
    }
}

impl Display for CacheBuster {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "CacheBuster ({} entries):", self.manifest.len())?;
        for (original, hashed) in self.manifest.entries() {
            write!(f, "\n\t`{original}` -> `{hashed}`")?;
        }
        Ok(())
    }
}

fn remove_conditional_headers(headers: &mut HeaderMap) {
    headers.remove(ETAG);
    headers.remove(IF_MODIFIED_SINCE);
    headers.remove(IF_MATCH);
    headers.remove(IF_NONE_MATCH);
    headers.remove(IF_RANGE);
    headers.remove(IF_UNMODIFIED_SINCE);
}

#[cfg(test)]
mod tests {
    use super::CacheBuster;

    #[test]
    fn an_empty_cache_buster_returns_paths_unchanged() {
        let cache_buster: CacheBuster = CacheBuster::empty();

        let expected: String = String::from("static/stylesheet/main.css");
        let actual: String = cache_buster.get_file("static/stylesheet/main.css");
        assert_eq!(expected, actual);

        let expected: bool = true;
        let actual: bool = cache_buster.is_empty();
        assert_eq!(expected, actual);
    }
}
