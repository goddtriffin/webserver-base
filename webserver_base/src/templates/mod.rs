//! Handlebars rendering, and the three types that feed it.
//!
//! Split by lifetime: [`BaseTemplateData`] is per-server, [`PageTemplateData`]
//! is per-render, and [`TemplateData`] is what Handlebars sees — assembled from
//! both plus the caller's own data, borrowing rather than cloning.

mod base;
mod error;
mod frontend;
mod page;
mod registry;
mod render;
pub mod robots;
mod site_entity;
mod theme;

pub use base::{BaseTemplateData, BaseTemplateDataParams};
pub use error::TemplateError;
pub use frontend::{
    AnalyticsPaths, FrontendRuntime, MINIMUM_SOCIAL_IMAGE_HEIGHT, MINIMUM_SOCIAL_IMAGE_WIDTH,
    SentryBrowser, SocialImageMetadata,
};
pub use page::{Article, PageTemplateData};
pub use registry::{
    BASE_TEMPLATE_NAME, NOT_FOUND_TEMPLATE_NAME, TEMPLATE_DIRECTORIES, TEMPLATE_ROOT,
    TemplateRegistry,
};
pub use render::TemplateData;
pub use site_entity::SiteEntity;
pub use theme::{Fallback, THEME_STORAGE_KEY, ThemeColor, ThemeColorTag, ThemeScript};

#[cfg(feature = "preset")]
pub use base::{GODDTRIFFIN_SEE_ALSO, GoddtriffinParams};
