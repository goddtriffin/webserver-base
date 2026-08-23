//! Content-hashed static assets.
//!
//! Every file under the asset directory is renamed to include a hash of its
//! contents, which is what lets hashed assets be cached forever and everything
//! else never.

mod cache_buster;
mod error;

pub use cache_buster::{CACHE_MANIFEST_FILE_NAME, CacheBuster};
pub use error::CacheBusterError;
