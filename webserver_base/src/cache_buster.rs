use std::{
    collections::BTreeMap,
    fmt::{self, Display},
    fs::{self, File},
    io::Read,
};
use std::{collections::VecDeque, path::Path};
use std::{fs::DirEntry, path::PathBuf};

use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, HeaderValue},
    middleware::Next,
    response::Response,
};
use chrono::{DateTime, Duration, TimeDelta, Utc};
use regex::Regex;
use reqwest::{
    StatusCode,
    header::{
        CACHE_CONTROL, ETAG, EXPIRES, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_RANGE,
        IF_UNMODIFIED_SINCE, PRAGMA,
    },
};
use tracing::{error, instrument, warn};

#[derive(Debug, Clone)]
pub struct CacheBuster {
    asset_directory: String,

    cache: BTreeMap<String, String>,
}

impl CacheBuster {
    #[must_use]
    #[instrument(skip_all)]
    pub fn new(asset_directory: &str) -> Self {
        Self {
            asset_directory: asset_directory.to_string(),
            cache: BTreeMap::new(),
        }
    }

    #[instrument(skip_all)]
    pub fn gen_cache(&mut self) {
        self.cache = gen_cache(Path::new(&self.asset_directory));
    }

    /// Takes a path from root domain to a static asset (as it would be called from a browser, so with a leading slash)
    /// and returns the version of the filepath that contains a unique hash.
    ///
    /// e.g. "/static/image/favicon/favicon.ico" -> "/static/image/favicon/favicon.66189abc248d80832e458ee37e93c9e8.ico"
    ///
    /// # Panics
    ///
    /// Panics if the file is not found in the cache.
    #[must_use]
    #[instrument(skip_all)]
    pub fn get_file(&self, original_asset_file_path: &str) -> String {
        // return the original path if the path does not start with the asset directory
        if !original_asset_file_path.starts_with(&self.asset_directory) {
            warn!(
                "CacheBuster: File path does not start with asset directory: '{original_asset_file_path:?}'. Returning original path: '{original_asset_file_path:?}'."
            );
            return original_asset_file_path.to_string();
        }

        self.cache
            .get(original_asset_file_path)
            .cloned()
            .unwrap_or_else(|| {
                error!(
                    "CacheBuster: File not found in cache: '{original_asset_file_path:?}'. Returning original path."
                );
                original_asset_file_path.to_string()
            })
    }

    #[must_use]
    #[instrument(skip_all)]
    pub fn get_cache(&self) -> BTreeMap<String, String> {
        self.cache.clone()
    }

    /// # Panics
    ///
    /// Panics if the file cannot be created or written to.
    #[instrument(skip_all)]
    pub fn print_to_file(&self, output_dir: &str) {
        let output_path: PathBuf = Path::new(output_dir).join("cache-buster.json");
        let file: File = File::create(&output_path)
            .unwrap_or_else(|_| panic!("Failed to create file: {output_path:?}"));

        serde_json::to_writer_pretty(file, &self.cache)
            .unwrap_or_else(|_| panic!("Failed to write JSON to file: {output_path:?}"));
    }

    /// Updates the sourceMappingURL comment in `.js` files to point to the hashed `.js.map` file.
    ///
    /// # Panics
    ///
    /// Panics if the file cannot be read or parsed.
    #[instrument(skip_all)]
    pub fn update_source_map_references(&self) {
        let source_map_regex: Regex = Regex::new(r"//# sourceMappingURL=(.+\.js\.map)")
            .unwrap_or_else(|_| panic!("Failed to compile sourceMappingURL regex"));

        for (original_path, hashed_path) in &self.cache {
            // only process `.js` files
            if !std::path::Path::new(original_path)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("js"))
            {
                continue;
            }

            // check for corresponding `.map` file
            let original_map_path: String = format!("{original_path}.map");
            let hashed_map_path: &String = match self.cache.get(&original_map_path) {
                Some(path) => path,
                None => continue,
            };

            // read `.map` file content
            let mut content: String = fs::read_to_string(hashed_path)
                .unwrap_or_else(|_| panic!("Failed to read file: {hashed_path}"));

            // get just the `.map` filename
            let hashed_map_filename: &str = Path::new(hashed_map_path)
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or_else(|| panic!("Invalid hashed map path"));

            // replace the `sourceMappingURL` comment
            if source_map_regex.is_match(&content) {
                content = source_map_regex
                    .replace(
                        &content,
                        format!("//# sourceMappingURL={hashed_map_filename}"),
                    )
                    .into_owned();

                // Write the updated content back to the file
                fs::write(hashed_path, content)
                    .unwrap_or_else(|_| panic!("Failed to write file: {hashed_path}"));
            }
        }
    }

    /// Middleware to set never-cache headers for all responses.
    #[instrument(skip_all)]
    pub async fn never_cache_middleware(req: Request, next: Next) -> Result<Response, StatusCode> {
        let mut response: Response<Body> = next.run(req).await;

        // remove ETag-related headers from the request
        remove_etag_headers(response.headers_mut());

        // set never-cache headers
        response.headers_mut().insert(
            EXPIRES,
            HeaderValue::from_static("Thu, 01 Jan 1970 00:00:00 GMT"),
        );
        response.headers_mut().insert(
            CACHE_CONTROL,
            HeaderValue::from_static("no-cache, no-store, must-revalidate, private, max-age=0"),
        );
        response
            .headers_mut()
            .insert(PRAGMA, HeaderValue::from_static("no-cache"));

        Ok(response)
    }

    /// Middleware to set forever cache headers for all responses.
    #[instrument(skip_all)]
    pub async fn forever_cache_middleware(
        req: Request,
        next: Next,
    ) -> Result<Response, StatusCode> {
        warn!(
            "CacheBuster: Forever-cacheing resource: '{}'",
            req.uri().path()
        );
        let mut response: Response<Body> = next.run(req).await;

        // remove ETag-related headers from the request
        remove_etag_headers(response.headers_mut());

        // set forever-cache headers (1 year)
        let one_year: TimeDelta = Duration::days(365);
        let expires: DateTime<Utc> = Utc::now() + one_year;
        response.headers_mut().insert(
            EXPIRES,
            HeaderValue::from_str(&expires.to_rfc2822()).unwrap(),
        );
        response.headers_mut().insert(
            CACHE_CONTROL,
            HeaderValue::from_static("public, max-age=31536000, must-revalidate, immutable"),
        );

        Ok(response)
    }
}

impl Display for CacheBuster {
    #[instrument(skip_all)]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // sort alphabetically by key
        let mut keys: Vec<&String> = self.cache.keys().collect();
        keys.sort();

        write!(
            f,
            "CacheBuster (asset directory: '{}'):",
            self.asset_directory
        )?;
        for key in keys {
            write!(f, "\n\t'{}' -> '{}'", key, self.cache.get(key).unwrap())?;
        }
        Ok(())
    }
}

#[instrument(skip_all)]
fn gen_cache(root: &Path) -> BTreeMap<String, String> {
    let mut cache: BTreeMap<String, String> = BTreeMap::new();

    let mut dirs_to_visit: VecDeque<PathBuf> = VecDeque::new();
    dirs_to_visit.push_back(root.to_path_buf());
    while let Some(dir_path) = dirs_to_visit.pop_front() {
        for entry in fs::read_dir(&dir_path)
            .unwrap_or_else(|_| panic!("Failed to read directory: {dir_path:?}"))
        {
            let error_msg: String =
                format!("Failed to read directory entry: {dir_path:?} -> {entry:?}");
            let entry: DirEntry = entry.expect(&error_msg);
            let path: PathBuf = entry.path();

            if path.is_dir() {
                dirs_to_visit.push_back(path);
            } else {
                let original_file_path: String = path.to_string_lossy().to_string();
                let new_file_path: String = generate_cache_busted_path(&path, root)
                    .to_string_lossy()
                    .to_string();

                // rename the files on disk
                fs::rename(&original_file_path, &new_file_path).unwrap_or_else(|_| {
                    panic!("Failed to rename file: {original_file_path:?} -> {new_file_path:?}")
                });

                cache.insert(original_file_path, new_file_path);
            }
        }
    }

    cache
}

#[instrument(skip_all)]
fn generate_cache_busted_path(file_path: &Path, root: &Path) -> PathBuf {
    // read the file contents
    let mut file: File = File::open(file_path)
        .unwrap_or_else(|_| panic!("Failed to open file: {root:?} -> {file_path:?}"));
    let mut contents: Vec<u8> = Vec::new();
    file.read_to_end(&mut contents)
        .unwrap_or_else(|_| panic!("Failed to read file: {root:?} -> {file_path:?}"));

    // generate MD5 hash
    let hash: String = format!("{:x}", md5::compute(contents));

    // get the relative path components
    let relative_path: &Path = file_path.strip_prefix(root).unwrap_or(file_path);
    let parent: &Path = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let file_name: &str = relative_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let new_filename: String = if file_name.contains('.') {
        // if at least one extension, insert hash before first period
        let (name, rest) = file_name.split_once('.').unwrap();
        format!("{name}.{hash}.{rest}")
    } else {
        // if no extension, append hash at the end
        format!("{file_name}.{hash}")
    };

    // Combine with parent path and root
    root.join(parent).join(new_filename)
}

#[instrument(skip_all)]
fn remove_etag_headers(headers: &mut HeaderMap) {
    headers.remove(ETAG);
    headers.remove(IF_MODIFIED_SINCE);
    headers.remove(IF_MATCH);
    headers.remove(IF_NONE_MATCH);
    headers.remove(IF_RANGE);
    headers.remove(IF_UNMODIFIED_SINCE);
}
