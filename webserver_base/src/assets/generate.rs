//! The build-time static-asset pipeline.
//!
//! Hashing happens here, at build time, rather than when the server starts.
//! That ordering is the whole point:
//!
//! - **It is idempotent by construction.** Renaming files at startup was not:
//!   a second run hashed the already-hashed names, the manifest keys stopped
//!   being the logical paths, and every stylesheet, script and image 404'd. An
//!   in-place `docker restart` was enough to trigger it.
//! - **The server never writes to disk**, so a production image needs no
//!   writable filesystem and two replicas cannot race over a shared volume.
//! - **One hashing pass serves both consumers.** The manifest feeds the
//!   templates; the generated TypeScript module feeds the browser. They cannot
//!   disagree, because they are two views of the same operation.
//!
//! It runs in two phases because the JavaScript *contents* depend on the
//! manifest, while the JavaScript *files* must be hashed once they exist.
//! Scripts never need their own hash — the layout takes that from the manifest.

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use tracing::{debug, info};

use super::error::CacheBusterError;
use super::manifest::{Manifest, TYPESCRIPT_MODULE_PATH};

/// The directory every project keeps its static assets in.
///
/// Not configurable. The layout beneath it is already fixed — `image/favicon/`,
/// `file/`, `stylesheet/`, `script/` — so the directory name was the one part
/// that looked adjustable without being so.
pub const STATIC_DIRECTORY: &str = "static";

/// Where the icon set lives.
pub const FAVICON_DIRECTORY: &str = "static/image/favicon";

/// The vector icon source, preferred when the art allows it.
pub const FAVICON_SVG_SOURCE: &str = "static/image/favicon/favicon.svg";

/// The raster icon source, for art that cannot be vectorised — a photograph,
/// most obviously.
///
/// The size is in the name on purpose: it must be exactly
/// [`SOURCE_PNG_SIZE`](super::icons::SOURCE_PNG_SIZE) square, and a filename
/// that states the requirement is harder to get wrong than one that does not.
pub const FAVICON_PNG_SOURCE: &str = "static/image/favicon/favicon-512.png";

/// Which half of the pipeline to run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Generate the icon set, then hash everything except `static/script/`.
    /// Emits the manifest and the TypeScript module the JS build consumes.
    NonScripts,
    /// Hash `static/script/` once the JavaScript has been built, and merge the
    /// result into the manifest.
    Scripts,
}

impl Phase {
    /// Parses the CLI subcommand this phase is invoked by.
    #[must_use]
    pub fn from_subcommand(subcommand: &str) -> Option<Self> {
        match subcommand {
            "gen-static-assets" => Some(Self::NonScripts),
            "gen-static-scripts" => Some(Self::Scripts),
            _ => None,
        }
    }

    /// Whether this phase owns `path`.
    fn owns(self, path: &Path) -> bool {
        let is_script: bool = path.starts_with("static/script");
        match self {
            Self::NonScripts => !is_script,
            Self::Scripts => is_script,
        }
    }
}

/// Runs one phase of the pipeline.
///
/// # Errors
///
/// [`CacheBusterError`] if `favicon.svg` is missing, an icon cannot be
/// rendered, a file cannot be read or renamed, or the manifest cannot be
/// written.
pub fn generate_static_assets(phase: Phase) -> Result<(), CacheBusterError> {
    let root: &Path = Path::new(STATIC_DIRECTORY);
    if !root.is_dir() {
        return Err(CacheBusterError::MissingStaticDirectory {
            path: root.to_path_buf(),
        });
    }

    // Merge rather than clobber: phase two must not discard phase one's work,
    // and a re-run must find already-hashed files by their hashed names.
    let mut manifest: Manifest = Manifest::load_or_empty()?;

    if phase == Phase::NonScripts {
        super::icons::generate_missing_icons(&manifest)?;
    }

    manifest.extend(hash_tree(root, phase)?);
    manifest.write_json()?;

    if phase == Phase::NonScripts {
        manifest.write_typescript()?;
        info!(
            "hashed {} static asset(s); wrote the manifest and {TYPESCRIPT_MODULE_PATH}",
            manifest.len()
        );
    } else {
        info!(
            "hashed the built scripts; manifest now holds {} entries",
            manifest.len()
        );
    }

    Ok(())
}

/// Walks `root`, renaming every file this phase owns to include a hash of its
/// contents.
fn hash_tree(root: &Path, phase: Phase) -> Result<BTreeMap<String, String>, CacheBusterError> {
    let mut cache: BTreeMap<String, String> = BTreeMap::new();
    let mut directories: Vec<PathBuf> = vec![root.to_path_buf()];

    while let Some(directory) = directories.pop() {
        let entries =
            fs::read_dir(&directory).map_err(|source| CacheBusterError::ReadDirectory {
                path: directory.clone(),
                source,
            })?;

        for entry in entries {
            let entry = entry.map_err(|source| CacheBusterError::ReadDirectory {
                path: directory.clone(),
                source,
            })?;
            let path: PathBuf = entry.path();

            if path.is_dir() {
                directories.push(path);
                continue;
            }
            if !phase.owns(&path) {
                continue;
            }
            // Re-hashing an already-hashed name is the bug this whole pipeline
            // exists to remove: it produces `main.<h>.<h>.css` and keys the
            // manifest by a path no template ever asks for.
            if is_content_hashed(&path) {
                debug!("`{}` is already hashed; leaving it alone", path.display());
                continue;
            }

            let hashed: PathBuf = content_hashed_path(&path, root)?;
            fs::rename(&path, &hashed).map_err(|source| CacheBusterError::Rename {
                from: path.clone(),
                to: hashed.clone(),
                source,
            })?;

            cache.insert(
                path.to_string_lossy().to_string(),
                hashed.to_string_lossy().to_string(),
            );
        }
    }

    Ok(cache)
}

/// Whether a file name already carries a 32-character hex content hash.
fn is_content_hashed(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.split('.').any(|segment| {
                segment.len() == 32 && segment.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        })
}

/// `dir/name.ext` → `dir/name.<md5>.ext`, hash inserted before the *first*
/// extension so `main.js.map` stays a `.js.map`.
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

    let relative: &Path = file_path.strip_prefix(root).unwrap_or(file_path);
    let parent: &Path = relative.parent().unwrap_or_else(|| Path::new(""));
    let name: &str = relative
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();

    let hashed_name: String = match name.split_once('.') {
        Some((stem, extension)) => format!("{stem}.{hash}.{extension}"),
        None => format!("{name}.{hash}"),
    };

    Ok(root.join(parent).join(hashed_name))
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{Phase, is_content_hashed};

    #[test]
    fn an_already_hashed_file_is_recognised_so_it_is_never_hashed_twice() {
        let hashed: PathBuf =
            PathBuf::from("static/stylesheet/main.aa676972bbd2b68e94ef8e91e81d20be.css");

        let expected: bool = true;
        let actual: bool = is_content_hashed(&hashed);
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_plain_file_is_not_mistaken_for_a_hashed_one() {
        let expected: bool = false;
        let actual: bool = is_content_hashed(Path::new("static/stylesheet/main.css"));
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_long_but_non_hex_segment_is_not_a_hash() {
        // 32 characters, but `z` is not hex.
        let path: PathBuf = PathBuf::from("static/zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz.css");

        let expected: bool = false;
        let actual: bool = is_content_hashed(&path);
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_source_map_keeps_its_double_extension() {
        let hashed: PathBuf =
            super::content_hashed_path(Path::new("Cargo.toml"), Path::new(".")).expect("readable");
        let name: &str = hashed.file_name().and_then(|n| n.to_str()).expect("named");

        // The hash goes before the FIRST dot, so `main.js.map` stays a `.js.map`
        // rather than becoming `main.js.<hash>.map`.
        assert!(name.starts_with("Cargo."));
        assert!(
            std::path::Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("toml"))
        );
    }

    #[test]
    fn each_phase_owns_a_disjoint_half_of_the_tree() {
        let script: &Path = Path::new("static/script/main.js");
        let image: &Path = Path::new("static/image/social/card.webp");

        assert!(!Phase::NonScripts.owns(script));
        assert!(Phase::NonScripts.owns(image));
        assert!(Phase::Scripts.owns(script));
        assert!(!Phase::Scripts.owns(image));
    }

    #[test]
    fn the_subcommands_map_to_their_phases() {
        assert_eq!(
            Some(Phase::NonScripts),
            Phase::from_subcommand("gen-static-assets")
        );
        assert_eq!(
            Some(Phase::Scripts),
            Phase::from_subcommand("gen-static-scripts")
        );
        assert_eq!(None, Phase::from_subcommand("serve"));
    }
}
