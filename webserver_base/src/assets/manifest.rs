//! The cache-buster manifest: logical asset path to content-hashed path.
//!
//! Two consumers read it, and both read the *same* build output. The server
//! loads the JSON at boot to resolve `{{lookup cache_buster …}}` in templates;
//! the JavaScript build inlines the generated TypeScript module so browser code
//! can resolve an asset without a network round trip.
//!
//! Both are build outputs, so both belong in `.gitignore`. They used to be
//! committed because the server wrote the manifest and the *next* build
//! consumed it — a cycle that made a stale checked-in copy able to feed the JS
//! build silently. Generating them before anything reads them removes the cycle
//! and the whole class of bug.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};

use super::error::CacheBusterError;

/// Where the JSON manifest is written, relative to the project root.
pub const MANIFEST_PATH: &str = "cache-buster.json";

/// Where the generated TypeScript module is written.
pub const TYPESCRIPT_MODULE_PATH: &str = "static/script/generated/cache-buster.ts";

/// A logical-path to hashed-path map.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Manifest {
    entries: BTreeMap<String, String>,
}

impl Manifest {
    /// Reads the manifest, or an empty one if it does not exist yet.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError::ReadFile`] if it exists but cannot be read, or
    /// [`CacheBusterError::ParseManifest`] if it is not valid JSON.
    pub fn load_or_empty() -> Result<Self, CacheBusterError> {
        let path: &Path = Path::new(MANIFEST_PATH);
        if !path.is_file() {
            return Ok(Self::default());
        }
        Self::load()
    }

    /// Reads the manifest.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError::ReadFile`] if it cannot be read, or
    /// [`CacheBusterError::ParseManifest`] if it is not valid JSON.
    pub fn load() -> Result<Self, CacheBusterError> {
        let path: PathBuf = PathBuf::from(MANIFEST_PATH);
        let contents: String =
            std::fs::read_to_string(&path).map_err(|source| CacheBusterError::ReadFile {
                path: path.clone(),
                source,
            })?;
        let entries: BTreeMap<String, String> = serde_json::from_str(&contents)
            .map_err(|source| CacheBusterError::ParseManifest { path, source })?;
        Ok(Self { entries })
    }

    /// Adds or replaces entries.
    pub fn extend(&mut self, entries: BTreeMap<String, String>) {
        self.entries.extend(entries);
    }

    /// How many assets are hashed.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether nothing is hashed.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The underlying map, as the templates see it.
    #[must_use]
    pub const fn entries(&self) -> &BTreeMap<String, String> {
        &self.entries
    }

    /// Consumes this manifest for its map.
    #[must_use]
    pub fn into_entries(self) -> BTreeMap<String, String> {
        self.entries
    }

    /// The hashed path for `original`, or `original` itself when it is not a
    /// hashed asset.
    #[must_use]
    pub fn resolve<'a>(&'a self, original: &'a str) -> &'a str {
        let key: &str = original.trim_start_matches('/');
        self.entries.get(key).map_or(original, String::as_str)
    }

    /// Whether this manifest knows `original` as a hashed asset.
    #[must_use]
    pub fn contains(&self, original: &str) -> bool {
        self.entries.contains_key(original.trim_start_matches('/'))
    }

    /// Writes the JSON manifest.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError::WriteManifest`] if it cannot be created or written.
    pub fn write_json(&self) -> Result<(), CacheBusterError> {
        let path: PathBuf = PathBuf::from(MANIFEST_PATH);
        let file: File = File::create(&path).map_err(|source| CacheBusterError::WriteManifest {
            path: path.clone(),
            source,
        })?;
        serde_json::to_writer_pretty(BufWriter::new(file), &self.entries).map_err(|source| {
            CacheBusterError::WriteManifest {
                path,
                source: std::io::Error::other(source),
            }
        })
    }

    /// Writes the generated TypeScript module.
    ///
    /// It is emitted `as const` with a key union, so a mistyped asset path is a
    /// compile error rather than a silent `undefined` and a broken image.
    ///
    /// # Errors
    ///
    /// [`CacheBusterError::WriteManifest`] if the file cannot be created or
    /// written.
    pub fn write_typescript(&self) -> Result<(), CacheBusterError> {
        let path: PathBuf = PathBuf::from(TYPESCRIPT_MODULE_PATH);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|source| CacheBusterError::WriteManifest {
                path: parent.to_path_buf(),
                source,
            })?;
        }

        let mut source: String = String::from(
            "// Generated by `gen_static_assets`. Do not edit, do not commit.\n\n\
             /**\n\
             \x20* Maps a logical static-asset path to its content-hashed one. The build\n\
             \x20* inlines this, so resolving an asset costs the browser nothing at\n\
             \x20* runtime.\n\
             \x20*/\n\
             export const CACHE_BUSTER = {\n",
        );
        for (original, hashed) in &self.entries {
            // The write cannot fail: the target is an in-memory String.
            let _ = writeln!(source, "  {original:?}: {hashed:?},");
        }
        source.push_str(
            "} as const;\n\n\
             /** Every asset path the build knows about. */\n\
             export type CacheBustedPath = keyof typeof CACHE_BUSTER;\n\n\
             /** Resolves a static asset to its root-absolute, hashed URL. */\n\
             export function asset(path: CacheBustedPath): string {\n\
             \x20 return `/${CACHE_BUSTER[path]}`;\n\
             }\n",
        );

        std::fs::write(&path, source)
            .map_err(|source| CacheBusterError::WriteManifest { path, source })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::Manifest;

    fn manifest() -> Manifest {
        let mut entries: BTreeMap<String, String> = BTreeMap::new();
        entries.insert(
            String::from("static/stylesheet/main.css"),
            String::from("static/stylesheet/main.abc123.css"),
        );
        Manifest { entries }
    }

    #[test]
    fn a_known_asset_resolves_to_its_hashed_path() {
        let manifest: Manifest = manifest();

        let expected: &str = "static/stylesheet/main.abc123.css";
        let actual: &str = manifest.resolve("static/stylesheet/main.css");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_leading_slash_still_resolves() {
        let manifest: Manifest = manifest();

        let expected: &str = "static/stylesheet/main.abc123.css";
        let actual: &str = manifest.resolve("/static/stylesheet/main.css");
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_unknown_asset_is_returned_unchanged() {
        let manifest: Manifest = manifest();

        let expected: &str = "https://cdn.example.com/a.css";
        let actual: &str = manifest.resolve("https://cdn.example.com/a.css");
        assert_eq!(expected, actual);
    }

    #[test]
    fn extending_replaces_an_existing_entry_rather_than_duplicating_it() {
        let mut manifest: Manifest = manifest();
        let mut second: BTreeMap<String, String> = BTreeMap::new();
        second.insert(
            String::from("static/stylesheet/main.css"),
            String::from("static/stylesheet/main.def456.css"),
        );
        manifest.extend(second);

        let expected: usize = 1;
        let actual: usize = manifest.len();
        assert_eq!(expected, actual);

        let expected: &str = "static/stylesheet/main.def456.css";
        let actual: &str = manifest.resolve("static/stylesheet/main.css");
        assert_eq!(expected, actual);
    }
}
