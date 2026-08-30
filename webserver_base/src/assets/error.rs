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

    /// The static-asset pipeline was run somewhere without a `static/`.
    #[error(
        "`{path}` does not exist. The static-asset pipeline runs from the \
         project root, where `static/` lives."
    )]
    MissingStaticDirectory { path: PathBuf },

    /// A frontend shipped no source icon.
    #[error(
        "no icon source. Provide `static/image/favicon/favicon.svg`, or a \
         512x512 `static/image/favicon/favicon-512.png` if the art cannot be \
         vectorised. The .ico, apple-touch icon and web-manifest icons are \
         generated from whichever you provide."
    )]
    MissingFavicon,

    /// A frontend shipped both icon sources.
    #[error(
        "both `favicon.svg` and `favicon-512.png` exist. Delete one: two \
         sources drift the moment somebody updates only one of them."
    )]
    AmbiguousFavicon,

    /// The icon directory holds something that is neither a source nor derived.
    #[error(
        "`{directory}` contains {count} file(s) that do not belong:{unexpected}\n\n\
         Only these are allowed: {allowed}.\n\
         Provide exactly one source — `favicon.svg`, or a 512x512 \
         `favicon-512.png` — and let the rest be generated."
    )]
    UnexpectedFavicons {
        directory: &'static str,
        count: usize,
        unexpected: String,
        allowed: String,
    },

    /// The raster icon source is not the required size.
    #[error("`{path}` must be exactly {expected}x{expected}, but it is {width}x{height}")]
    FaviconDimensions {
        path: PathBuf,
        expected: u32,
        width: u32,
        height: u32,
    },

    /// An image's dimensions could not be read.
    #[error("failed to read the dimensions of `{path}`: {reason}")]
    ReadImage { path: PathBuf, reason: String },

    /// A declared asset is not in the manifest, so its URL would 404.
    #[error(
        "{count} declared asset(s) are missing from the manifest, so every URL \
         pointing at them would 404:{missing}\n\nRun the static-asset pipeline, \
         or correct the paths."
    )]
    UnresolvedAssets { count: usize, missing: String },

    /// A generated icon is absent or the wrong size.
    #[error("`{path}` should be {expected}x{expected} but is {found}")]
    IconMismatch {
        path: String,
        expected: u32,
        found: String,
    },

    /// `favicon.svg` could not be parsed.
    #[error(
        "failed to parse `favicon.svg`. Note that it must not rely on system \
         fonts — convert any text to paths."
    )]
    RenderIcon {
        #[source]
        source: resvg::usvg::Error,
    },

    /// A pixel buffer of the requested size could not be allocated.
    #[error("cannot rasterise an icon at {size}x{size}")]
    IconDimensions { size: u32 },

    /// A rendered icon could not be encoded. Carried as text because the
    /// encoder's error type belongs to a crate this one does not name directly.
    #[error("failed to encode a generated icon: {reason}")]
    EncodeIcon { reason: String },

    /// A generated icon could not be written.
    #[error("failed to write the generated icon `{path}`")]
    WriteIcon {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// The manifest exists but is not valid JSON.
    #[error("failed to parse the cache manifest at `{path}`")]
    ParseManifest {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },

    /// A site has static assets but no manifest, so every hashed URL would 404.
    #[error(
        "`{path}` is missing but `static/` exists. Run `make gen_static_assets` \
         — asset hashing happens at build time, not at startup."
    )]
    MissingManifest { path: PathBuf },

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
