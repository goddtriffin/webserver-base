//! What the social card image actually is.
//!
//! Read from the file's header bytes rather than declared, because a hand-typed
//! dimension drifts from the file the moment someone swaps the image — and a
//! wrong `og:image:width` makes platforms lay the card out at the wrong aspect
//! ratio, which is worse than omitting the tag.

use std::path::Path;

use crate::templates::SocialImageMetadata;

/// Reads `path`'s dimensions and MIME type.
///
/// Returns an empty result rather than an error when the file is absent or
/// unreadable: a missing social image should not stop a server from booting,
/// and omitting the tags asserts nothing.
#[must_use]
pub fn probe_social_image(path: impl AsRef<Path>) -> SocialImageMetadata {
    let path: &Path = path.as_ref();
    let mime_type: Option<&'static str> = mime_type_of(path);

    match imagesize::size(path) {
        Ok(size) => SocialImageMetadata {
            width: u32::try_from(size.width).ok(),
            height: u32::try_from(size.height).ok(),
            mime_type,
        },
        Err(_) => SocialImageMetadata {
            width: None,
            height: None,
            mime_type,
        },
    }
}

/// The MIME type implied by a file extension.
///
/// Every one of these renders as an Open Graph image on Facebook, X,
/// `LinkedIn`, Discord and Slack. `WebP` is included deliberately —
/// `LinkedIn` was the last holdout and added support in December 2024 —
/// though very old `WhatsApp` clients still will not render it.
fn mime_type_of(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "webp" => Some("image/webp"),
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use crate::templates::SocialImageMetadata;

    use super::{mime_type_of, probe_social_image};

    #[test]
    fn the_mime_type_follows_the_extension() {
        assert_eq!(Some("image/webp"), mime_type_of(Path::new("a/card.webp")));
        assert_eq!(Some("image/png"), mime_type_of(Path::new("a/card.PNG")));
        assert_eq!(Some("image/jpeg"), mime_type_of(Path::new("a/card.jpeg")));
        assert_eq!(None, mime_type_of(Path::new("a/card.svg")));
    }

    #[test]
    fn an_unreadable_image_yields_no_dimensions_rather_than_an_error() {
        let expected: SocialImageMetadata = SocialImageMetadata {
            width: None,
            height: None,
            mime_type: Some("image/png"),
        };
        let actual: SocialImageMetadata = probe_social_image("/tmp/wsb-does-not-exist.png");
        assert_eq!(expected, actual);
    }
}
