//! Content-hashed static assets.
//!
//! Hashing happens at **build time**, in [`generate`], not when the server
//! starts. At runtime this module only reads the manifest that build produced
//! and applies the two cache-control policies: hashed URLs are immutable for a
//! year, everything else is never cached.

mod cache_buster;
mod error;
pub mod generate;
pub mod icons;
mod manifest;
mod modified;
// Probing returns a template type, and only a frontend has a social card.
#[cfg(feature = "templates")]
mod probe;
mod validate;

pub use cache_buster::CacheBuster;
pub use error::CacheBusterError;
pub use generate::{
    FAVICON_DIRECTORY, FAVICON_PNG_SOURCE, FAVICON_SVG_SOURCE, Phase, STATIC_DIRECTORY,
    generate_static_assets,
};
pub use icons::{DERIVED, IconSource, SOURCE_PNG_SIZE, dimensions, resolve_source};
pub use manifest::{MANIFEST_PATH, Manifest, TYPESCRIPT_MODULE_PATH};
pub use modified::content_modified;
#[cfg(feature = "templates")]
pub use probe::probe_social_image;
pub use validate::{validate_declared, validate_icons};
