//! Failures from building or writing the asset cache.

use std::path::PathBuf;

/// Why the asset cache could not be built.
///
/// All boot-time failures. Lookups afterwards never fail —
/// [`CacheBuster::get_file`](super::CacheBuster::get_file) falls back to the
/// unhashed path rather than taking a page down over one image.
#[derive(Debug, thiserror::Error)]
pub enum CacheBusterError {
    /// A directory under the asset root could not be read.
    #[error("failed to read asset directory `{path}`")]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A file could not be read in order to hash it.
    #[error("failed to read asset `{path}`")]
    ReadFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A file could not be renamed to its content-hashed name.
    #[error("failed to rename asset `{from}` to `{to}`")]
    Rename {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A `.js` file's `sourceMappingURL` comment could not be rewritten.
    #[error("failed to rewrite the source map reference in `{path}`")]
    SourceMap {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The cache manifest could not be written.
    #[error("failed to write the cache manifest to `{path}`")]
    WriteManifest {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The manifest could not be serialized.
    #[error("failed to serialize the cache manifest")]
    SerializeManifest(#[source] serde_json::Error),
}
