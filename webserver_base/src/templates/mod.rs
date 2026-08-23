//! Handlebars rendering, and the three types that feed it.
//!
//! Split by lifetime: [`BaseTemplateData`] is per-server, [`PageTemplateData`]
//! is per-render, and [`TemplateData`] is what Handlebars sees — assembled from
//! both plus the caller's own data, borrowing rather than cloning.

mod base;
mod error;
mod page;
mod registry;
mod render;
pub mod robots;

pub use base::{BaseTemplateData, BaseTemplateDataParams};
pub use error::TemplateError;
pub use page::PageTemplateData;
pub use registry::{TEMPLATE_DIRECTORIES, TemplateRegistry};
pub use render::TemplateData;

#[cfg(feature = "preset")]
pub use base::{GODDTRIFFIN_SEE_ALSO, GoddtriffinParams};
