//! When the site's content last changed.
//!
//! This feeds `<lastmod>`. Google uses that value only when it is
//! "consistently and verifiably accurate" — it fetches the page and compares —
//! and one bad pattern discredits the tag across the whole file. So the number
//! has to mean something.
//!
//! Server *boot* time does not: a restart with no content change would
//! re-declare the entire site as modified. The binary's own timestamp does not
//! either, because templates are loaded from disk and ship in a different image
//! layer, so editing a page leaves the binary untouched. The newest timestamp
//! across the templates, the static assets *and* the binary moves when — and
//! only when — something that determines the output actually changed.

use std::path::Path;

use chrono::{DateTime, Datelike, Utc};
use tracing::error;

/// The newest modification time across everything that determines page output.
///
/// Returns `None` when the result is implausible, in which case the caller
/// should omit `<lastmod>` rather than assert something a crawler can falsify.
/// Reproducible builds normalise timestamps to the epoch, and a skewed clock
/// can produce a future date; both are worse than saying nothing.
#[must_use]
pub fn content_modified(directories: &[&str]) -> Option<DateTime<Utc>> {
    let newest: DateTime<Utc> = directories
        .iter()
        .filter_map(|directory| newest_mtime(Path::new(directory)))
        .chain(current_exe_mtime())
        .max()?;

    let now: DateTime<Utc> = Utc::now();
    if newest.year() <= 2000 || newest > now {
        error!(
            "content modification time is `{newest}`, which is implausible; \
             omitting <lastmod>. Check that the build preserves file timestamps."
        );
        return None;
    }

    Some(newest)
}

/// The newest mtime anywhere under `root`.
fn newest_mtime(root: &Path) -> Option<DateTime<Utc>> {
    if !root.is_dir() {
        return None;
    }

    let mut newest: Option<DateTime<Utc>> = None;
    let mut directories: Vec<std::path::PathBuf> = vec![root.to_path_buf()];

    while let Some(directory) = directories.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                directories.push(path);
                continue;
            }
            let Ok(modified) = entry.metadata().and_then(|meta| meta.modified()) else {
                continue;
            };
            let modified: DateTime<Utc> = modified.into();
            if newest.is_none_or(|current| modified > current) {
                newest = Some(modified);
            }
        }
    }

    newest
}

/// The running binary's mtime, so a code-only change still moves the date.
fn current_exe_mtime() -> Option<DateTime<Utc>> {
    let exe = std::env::current_exe().ok()?;
    let modified = std::fs::metadata(exe).ok()?.modified().ok()?;
    Some(modified.into())
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, Utc};

    use super::{content_modified, newest_mtime};

    #[test]
    fn a_directory_that_does_not_exist_contributes_nothing() {
        let expected: Option<DateTime<Utc>> = None;
        let actual: Option<DateTime<Utc>> =
            newest_mtime(std::path::Path::new("/tmp/wsb-no-such-dir"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_binarys_own_timestamp_is_enough_to_produce_a_plausible_date() {
        // Even with no content directories, the test binary itself is recent,
        // so this must not fall through to the implausible branch.
        let actual: Option<DateTime<Utc>> = content_modified(&["/tmp/wsb-no-such-dir"]);
        assert!(actual.is_some());
    }
}
