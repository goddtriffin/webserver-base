//! Per-server frontend data, computed once at boot.
//!
//! Everything here is true of every page but owned by neither
//! [`BaseTemplateData`](super::BaseTemplateData) (which is what the *author*
//! declares) nor [`PageTemplateData`](super::PageTemplateData) (which is what
//! *this render* is): it is what the server worked out for itself at startup.

use serde::Serialize;

/// Where the first-party Plausible proxy lives on this origin.
///
/// Both paths are derived from the project name rather than configured. Two
/// reasons: Plausible's own guidance is to avoid their default paths because
/// blocklists target them, and a single library-wide constant shared across
/// every site would be one filter rule away from breaking all of them at once.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AnalyticsPaths {
    /// The proxied script, e.g. `/script/boggledygook-a3f2c1d8.js`.
    pub script_path: String,
    /// The proxied event endpoint, e.g. `/api/v1/boggledygook-a3f2c1d8`.
    pub event_path: String,
}

/// Browser-side error monitoring.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SentryBrowser {
    /// The proxied Sentry loader script.
    pub script_path: String,
    /// Where the browser SDK posts envelopes, relayed verbatim to Sentry.
    pub tunnel_path: String,
    /// The Sentry `environment` tag.
    pub environment: String,
}

/// What the social card image actually is.
///
/// Read from the image's header bytes at boot rather than declared, because a
/// hand-typed dimension can drift from the file and a wrong `og:image:width`
/// makes platforms render the card at the wrong aspect ratio — worse than
/// omitting it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize)]
pub struct SocialImageMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub mime_type: Option<&'static str>,
}

/// The smallest image X/Twitter will render as a `summary_large_image` card.
pub const MINIMUM_SOCIAL_IMAGE_WIDTH: u32 = 600;
/// The matching minimum height.
pub const MINIMUM_SOCIAL_IMAGE_HEIGHT: u32 = 314;

impl SocialImageMetadata {
    /// Whether this image is large enough for the wide card the layout
    /// declares. A smaller one silently degrades to a thumbnail.
    #[must_use]
    pub const fn is_large_enough(&self) -> bool {
        match (self.width, self.height) {
            (Some(width), Some(height)) => {
                width >= MINIMUM_SOCIAL_IMAGE_WIDTH && height >= MINIMUM_SOCIAL_IMAGE_HEIGHT
            }
            // Unknown dimensions are not a failure; nothing is claimed.
            _ => true,
        }
    }
}

/// Everything a frontend computes once and reuses on every render.
#[derive(Debug, Clone, Serialize)]
pub struct FrontendRuntime {
    /// The inline pre-paint theme script.
    pub theme_script: String,
    /// The social image's real dimensions and MIME type.
    pub social_image: SocialImageMetadata,
    /// Whether an SVG icon exists to link. A project whose art is a photograph
    /// authors a PNG instead, and the layout must not link a file that is not
    /// there.
    pub has_svg_icon: bool,
    /// Analytics proxy paths.
    pub analytics: AnalyticsPaths,
    /// Browser error monitoring.
    pub sentry_browser: SentryBrowser,
}

#[cfg(test)]
mod tests {
    use super::{MINIMUM_SOCIAL_IMAGE_HEIGHT, MINIMUM_SOCIAL_IMAGE_WIDTH, SocialImageMetadata};

    #[test]
    fn an_image_at_the_floor_is_large_enough() {
        let image: SocialImageMetadata = SocialImageMetadata {
            width: Some(MINIMUM_SOCIAL_IMAGE_WIDTH),
            height: Some(MINIMUM_SOCIAL_IMAGE_HEIGHT),
            mime_type: Some("image/webp"),
        };

        let expected: bool = true;
        let actual: bool = image.is_large_enough();
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_image_below_the_floor_would_degrade_to_a_thumbnail() {
        let image: SocialImageMetadata = SocialImageMetadata {
            width: Some(400),
            height: Some(210),
            mime_type: Some("image/webp"),
        };

        let expected: bool = false;
        let actual: bool = image.is_large_enough();
        assert_eq!(expected, actual);
    }

    #[test]
    fn unknown_dimensions_are_not_treated_as_a_failure() {
        let expected: bool = true;
        let actual: bool = SocialImageMetadata::default().is_large_enough();
        assert_eq!(expected, actual);
    }
}
