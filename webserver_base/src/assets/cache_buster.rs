//! Content-hashed asset paths, and the cache headers that make them worth
//! having.

use std::collections::{BTreeMap, VecDeque};
use std::fmt::{self, Display, Formatter};
use std::fs::{self, DirEntry, File};
use std::io::Read;
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
use regex::Regex;
use tracing::{debug, error, instrument, warn};

use super::error::CacheBusterError;

/// The file [`CacheBuster::write_manifest`] writes.
pub const CACHE_MANIFEST_FILE_NAME: &str = "cache-buster.json";

/// A map from each asset's original path to its content-hashed path.
#[derive(Debug, Clone, Default)]
pub struct CacheBuster {
    asset_directory: String,
    cache: BTreeMap<String, String>,
}

impl CacheBuster {
    /// An empty cache over `asset_directory`, touching no files.
    ///
    /// Every lookup falls through to the original path, which is exactly what a
    /// test wants. Use [`CacheBuster::build`] for the real thing.
    #[must_use]
    pub fn new(asset_directory: &str) -> Self {
        Self {
            asset_directory: asset_directory.to_string(),
            cache: BTreeMap::new(),
        }
    }

    /// Hashes and renames every file under `asset_directory`, then repairs the
    /// `sourceMappingURL` comment in any script whose map was also renamed.
    ///
    /// This is the whole ritual in one call. It mutates the directory on disk,
    /// so it belongs in a build step, not a test.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError`] if any file cannot be read, renamed, or rewritten.
    #[instrument(skip_all)]
    pub fn build(asset_directory: &str) -> Result<Self, CacheBusterError> {
        let mut cache_buster: Self = Self::new(asset_directory);
        cache_buster.cache = generate_cache(Path::new(asset_directory))?;
        cache_buster.update_source_map_references()?;
        Ok(cache_buster)
    }

    /// Maps an asset's original path to its content-hashed one.
    ///
    /// e.g. `static/image/favicon.ico` → `static/image/favicon.66189abc….ico`
    ///
    /// Infallible on purpose: an asset missing from the cache logs an error and
    /// returns the path it was given. A stale filename renders a broken image;
    /// a panic here renders nothing at all.
    #[must_use]
    #[instrument(skip_all)]
    pub fn get_file(&self, original_asset_file_path: &str) -> String {
        if !original_asset_file_path.starts_with(&self.asset_directory) {
            warn!(
                "CacheBuster: `{original_asset_file_path}` is outside asset directory `{}`; returning it unchanged",
                self.asset_directory
            );
            return original_asset_file_path.to_string();
        }

        self.cache
            .get(original_asset_file_path)
            .cloned()
            .unwrap_or_else(|| {
                error!(
                    "CacheBuster: `{original_asset_file_path}` is not in the cache; returning it unchanged"
                );
                original_asset_file_path.to_string()
            })
    }

    /// The whole map, for the `cache_buster` template lookup.
    #[must_use]
    pub const fn cache(&self) -> &BTreeMap<String, String> {
        &self.cache
    }

    /// The directory this cache was built over.
    #[must_use]
    pub fn asset_directory(&self) -> &str {
        &self.asset_directory
    }

    /// Whether anything was actually hashed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }

    /// Writes the map to `<output_dir>/cache-buster.json`, for tooling outside
    /// the process that needs to resolve a hashed name.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError::WriteManifest`] or
    /// [`CacheBusterError::SerializeManifest`].
    #[instrument(skip_all)]
    pub fn write_manifest(&self, output_dir: impl AsRef<Path>) -> Result<(), CacheBusterError> {
        let output_path: PathBuf = output_dir.as_ref().join(CACHE_MANIFEST_FILE_NAME);
        let file: File =
            File::create(&output_path).map_err(|source| CacheBusterError::WriteManifest {
                path: output_path.clone(),
                source,
            })?;
        serde_json::to_writer_pretty(file, &self.cache)
            .map_err(CacheBusterError::SerializeManifest)?;
        debug!("CacheBuster: wrote manifest to `{}`", output_path.display());
        Ok(())
    }

    /// Points every `//# sourceMappingURL=` comment at the hashed map file.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError::SourceMap`] if a script cannot be read or written.
    #[instrument(skip_all)]
    fn update_source_map_references(&self) -> Result<(), CacheBusterError> {
        // A literal pattern, compiled once, which cannot fail — but `expect`
        // rather than `unwrap` so a future edit to the pattern says why.
        let source_map_regex: Regex =
            Regex::new(r"//# sourceMappingURL=(.+\.js\.map)").expect("literal regex is valid");

        for (original_path, hashed_path) in &self.cache {
            if !Path::new(original_path)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
            {
                continue;
            }

            let original_map_path: String = format!("{original_path}.map");
            let Some(hashed_map_path) = self.cache.get(&original_map_path) else {
                continue;
            };
            let Some(hashed_map_file_name) = Path::new(hashed_map_path)
                .file_name()
                .and_then(|s| s.to_str())
            else {
                continue;
            };

            let content: String =
                fs::read_to_string(hashed_path).map_err(|source| CacheBusterError::SourceMap {
                    path: PathBuf::from(hashed_path),
                    source,
                })?;
            if !source_map_regex.is_match(&content) {
                continue;
            }

            let rewritten: String = source_map_regex
                .replace(
                    &content,
                    format!("//# sourceMappingURL={hashed_map_file_name}"),
                )
                .into_owned();
            fs::write(hashed_path, rewritten).map_err(|source| CacheBusterError::SourceMap {
                path: PathBuf::from(hashed_path),
                source,
            })?;
        }

        Ok(())
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
    /// changed URL by construction.
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
        write!(
            f,
            "CacheBuster (asset directory: `{}`, {} entries):",
            self.asset_directory,
            self.cache.len()
        )?;
        for (original, hashed) in &self.cache {
            write!(f, "\n\t`{original}` -> `{hashed}`")?;
        }
        Ok(())
    }
}

/// Walks `root`, renaming every file to include a hash of its contents.
#[instrument(skip_all)]
fn generate_cache(root: &Path) -> Result<BTreeMap<String, String>, CacheBusterError> {
    let mut cache: BTreeMap<String, String> = BTreeMap::new();
    let mut directories: VecDeque<PathBuf> = VecDeque::new();
    directories.push_back(root.to_path_buf());

    while let Some(directory) = directories.pop_front() {
        let entries =
            fs::read_dir(&directory).map_err(|source| CacheBusterError::ReadDirectory {
                path: directory.clone(),
                source,
            })?;

        for entry in entries {
            let entry: DirEntry = entry.map_err(|source| CacheBusterError::ReadDirectory {
                path: directory.clone(),
                source,
            })?;
            let path: PathBuf = entry.path();

            if path.is_dir() {
                directories.push_back(path);
                continue;
            }

            let hashed_path: PathBuf = content_hashed_path(&path, root)?;
            fs::rename(&path, &hashed_path).map_err(|source| CacheBusterError::Rename {
                from: path.clone(),
                to: hashed_path.clone(),
                source,
            })?;

            cache.insert(
                path.to_string_lossy().to_string(),
                hashed_path.to_string_lossy().to_string(),
            );
        }
    }

    Ok(cache)
}

/// `dir/name.ext` → `dir/name.<md5>.ext`, hash inserted before the *first*
/// extension so `main.js.map` stays a `.js.map`.
#[instrument(skip_all)]
fn content_hashed_path(file_path: &Path, root: &Path) -> Result<PathBuf, CacheBusterError> {
    let mut file: File = File::open(file_path).map_err(|source| CacheBusterError::ReadFile {
        path: file_path.to_path_buf(),
        source,
    })?;
    let mut contents: Vec<u8> = Vec::new();
    file.read_to_end(&mut contents)
        .map_err(|source| CacheBusterError::ReadFile {
            path: file_path.to_path_buf(),
            source,
        })?;

    let hash: String = format!("{:x}", md5::compute(contents));

    let relative_path: &Path = file_path.strip_prefix(root).unwrap_or(file_path);
    let parent: &Path = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let file_name: &str = relative_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let hashed_file_name: String = match file_name.split_once('.') {
        Some((stem, extensions)) => format!("{stem}.{hash}.{extensions}"),
        None => format!("{file_name}.{hash}"),
    };

    Ok(root.join(parent).join(hashed_file_name))
}

/// Strips every header an intermediary could use to revalidate a cached copy.
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
    use std::collections::BTreeMap;
    use std::path::{Path, PathBuf};

    use super::{CacheBuster, content_hashed_path};

    #[test]
    fn an_empty_cache_returns_every_path_unchanged() {
        let cache_buster: CacheBuster = CacheBuster::new("static");

        let expected: String = String::from("static/file/robots.txt");
        let actual: String = cache_buster.get_file("static/file/robots.txt");
        assert_eq!(expected, actual);

        let expected_empty: bool = true;
        let actual_empty: bool = cache_buster.is_empty();
        assert_eq!(expected_empty, actual_empty);
    }

    #[test]
    fn a_path_outside_the_asset_directory_is_returned_unchanged() {
        let cache_buster: CacheBuster = CacheBuster::new("static");

        let expected: String = String::from("html/pages/home.hbs");
        let actual: String = cache_buster.get_file("html/pages/home.hbs");
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_hash_goes_before_the_first_extension_so_js_map_survives() {
        let root: &Path = Path::new("/tmp/wsb-cache-buster-test");
        std::fs::create_dir_all(root.join("script")).expect("temp dir");
        let script: PathBuf = root.join("script/main.js.map");
        std::fs::write(&script, b"{}").expect("temp file");

        let actual: PathBuf = content_hashed_path(&script, root).expect("readable file");
        let actual_name: &str = actual
            .file_name()
            .and_then(|name| name.to_str())
            .expect("named");

        // md5("{}") is 99914b932bd37a50b983c5e7c90ae93b
        let expected_name: &str = "main.99914b932bd37a50b983c5e7c90ae93b.js.map";
        assert_eq!(expected_name, actual_name);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn a_file_with_no_extension_gets_the_hash_appended() {
        let root: &Path = Path::new("/tmp/wsb-cache-buster-test-noext");
        std::fs::create_dir_all(root).expect("temp dir");
        let file: PathBuf = root.join("humans");
        std::fs::write(&file, b"{}").expect("temp file");

        let actual: PathBuf = content_hashed_path(&file, root).expect("readable file");
        let actual_name: &str = actual
            .file_name()
            .and_then(|name| name.to_str())
            .expect("named");

        let expected_name: &str = "humans.99914b932bd37a50b983c5e7c90ae93b";
        assert_eq!(expected_name, actual_name);

        std::fs::remove_dir_all(root).ok();
    }

    #[test]
    fn the_cache_is_exposed_for_the_template_lookup() {
        let cache_buster: CacheBuster = CacheBuster::new("static");

        let expected: BTreeMap<String, String> = BTreeMap::new();
        let actual: BTreeMap<String, String> = cache_buster.cache().clone();
        assert_eq!(expected, actual);

        let expected_directory: String = String::from("static");
        let actual_directory: String = cache_buster.asset_directory().to_string();
        assert_eq!(expected_directory, actual_directory);
    }
}
