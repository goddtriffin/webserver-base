//! Boot-time proof that every asset a page will reference actually resolves.
//!
//! Resolving a missing asset to its un-hashed path would produce a link that
//! 404s for the visitor and nothing at all for us — the worst kind of failure,
//! because it is invisible from the inside. So the check happens once, at
//! start-up, and reports *every* miss at once.
//!
//! Start-up rather than per-request on purpose: a failed request means a visitor
//! sees a broken page, while a failed boot means the deploy never cuts over and
//! the previous container keeps serving. Same signal, no outage — and locally it
//! surfaces the moment you run the server.
//!
//! This covers everything *declared*: site-wide and per-page stylesheets and
//! scripts, the social image, and the generated icon set. Assets a handler looks
//! up dynamically at request time cannot be enumerated here, and remain the
//! project's responsibility.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use super::cache_buster::CacheBuster;
use super::error::CacheBusterError;
use super::generate::FAVICON_DIRECTORY;
use super::icons::{self, ALLOWED_FAVICON_FILES, DERIVED, IconSource};

/// Checks that every declared asset is in the manifest.
///
/// An absolute URL is skipped: a CDN stylesheet will never be in the manifest,
/// and that is the one legitimate reason for a path to be absent.
///
/// # Errors
///
/// [`CacheBusterError::UnresolvedAssets`], naming every miss.
pub fn validate_declared(
    cache_buster: &CacheBuster,
    declared: &[String],
) -> Result<(), CacheBusterError> {
    let missing: Vec<&String> = declared
        .iter()
        .filter(|path| !is_external(path))
        .filter(|path| !cache_buster.is_hashed(path))
        .collect();

    if missing.is_empty() {
        return Ok(());
    }

    let mut listed: String = String::new();
    for path in &missing {
        // Writing into a String cannot fail.
        let _ = write!(listed, "\n  - {path}");
    }

    Err(CacheBusterError::UnresolvedAssets {
        count: missing.len(),
        missing: listed,
    })
}

/// Checks the icon set: one source, and every derived file present and exactly
/// the size it claims to be.
///
/// Returns which source the project authored, because the layout links an SVG
/// icon only when there is one.
///
/// # Errors
///
/// [`CacheBusterError`] if the source is missing, ambiguous, or wrongly sized,
/// or if a derived icon is absent or the wrong size.
pub fn validate_icons(cache_buster: &CacheBuster) -> Result<IconSource, CacheBusterError> {
    validate_favicon_directory()?;

    let source: IconSource = icons::resolve_source(cache_buster.manifest())?;

    for icon in &DERIVED {
        let logical: String = format!("{FAVICON_DIRECTORY}/{}", icon.file_name);
        let hashed: String = cache_buster.get_file(&logical);
        let path: PathBuf = PathBuf::from(&hashed);

        if !path.is_file() {
            return Err(CacheBusterError::IconMismatch {
                path: logical,
                expected: icon.size,
                found: String::from("missing"),
            });
        }

        // Only the PNGs are dimension-checked. An `.ico` is a container that may
        // legitimately hold several sizes — a hand-supplied one usually does —
        // so a single expected number would be wrong for it.
        if !is_png(&path) {
            continue;
        }

        let (width, height) = icons::dimensions(&path)?;
        if width != icon.size || height != icon.size {
            return Err(CacheBusterError::IconMismatch {
                path: logical,
                expected: icon.size,
                found: format!("{width}x{height}"),
            });
        }
    }

    Ok(source)
}

/// Rejects anything in the icon directory that is neither a source nor derived.
///
/// A stale `favicon-16.png` from a previous design, or a size somebody dropped
/// in expecting it to be picked up, is invisible until a wrong picture shows up
/// in a browser tab. A closed set makes that a boot failure instead.
fn validate_favicon_directory() -> Result<(), CacheBusterError> {
    let directory: &Path = Path::new(FAVICON_DIRECTORY);
    let Ok(entries) = std::fs::read_dir(directory) else {
        // Absent entirely is a missing *source*, which says something more
        // useful than "the directory has odd contents".
        return Ok(());
    };

    let mut unexpected: Vec<String> = Vec::new();
    for entry in entries.flatten() {
        let path: PathBuf = entry.path();
        if path.is_dir() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !ALLOWED_FAVICON_FILES.contains(&logical_name(name).as_str()) {
            unexpected.push(name.to_string());
        }
    }

    if unexpected.is_empty() {
        return Ok(());
    }

    unexpected.sort();
    let mut listed: String = String::new();
    for name in &unexpected {
        // Writing into a String cannot fail.
        let _ = write!(listed, "\n  - {name}");
    }

    Err(CacheBusterError::UnexpectedFavicons {
        directory: FAVICON_DIRECTORY,
        count: unexpected.len(),
        unexpected: listed,
        allowed: ALLOWED_FAVICON_FILES.join(", "),
    })
}

/// `favicon.a1b2….svg` → `favicon.svg`.
///
/// The directory is checked after hashing, so the names on disk carry a content
/// hash the allowed-set does not.
fn logical_name(name: &str) -> String {
    let parts: Vec<&str> = name.split('.').collect();
    parts
        .into_iter()
        .filter(|segment| {
            !(segment.len() == 32 && segment.bytes().all(|byte| byte.is_ascii_hexdigit()))
        })
        .collect::<Vec<&str>>()
        .join(".")
}

/// Whether a path points somewhere this server does not serve.
fn is_external(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://")
}

fn is_png(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("png"))
}

#[cfg(test)]
mod tests {
    use super::{is_external, is_png, logical_name};

    #[test]
    fn a_cdn_url_is_external_and_therefore_exempt() {
        assert!(is_external("https://cdnjs.cloudflare.com/a/b.css"));
        assert!(is_external("http://example.com/a.js"));
        assert!(!is_external("static/stylesheet/main.css"));
        assert!(!is_external("/static/stylesheet/main.css"));
    }

    #[test]
    fn a_hashed_name_reduces_to_the_one_the_allowed_set_lists() {
        assert_eq!(
            String::from("favicon.svg"),
            logical_name("favicon.aa676972bbd2b68e94ef8e91e81d20be.svg")
        );
        assert_eq!(
            String::from("favicon-512.png"),
            logical_name("favicon-512.aa676972bbd2b68e94ef8e91e81d20be.png")
        );
        // Unhashed names pass through, for the pre-build state.
        assert_eq!(String::from("icon-192.png"), logical_name("icon-192.png"));
    }

    #[test]
    fn only_the_pngs_are_dimension_checked() {
        assert!(is_png(std::path::Path::new("a/icon-192.png")));
        assert!(is_png(std::path::Path::new("a/icon-192.PNG")));
        // An .ico may legitimately hold several sizes at once.
        assert!(!is_png(std::path::Path::new("a/favicon.ico")));
        assert!(!is_png(std::path::Path::new("a/favicon.svg")));
    }
}
