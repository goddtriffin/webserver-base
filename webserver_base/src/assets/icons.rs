//! The icon set, rendered from the one icon a project authors.
//!
//! A project ships exactly one source — `favicon.svg`, or a 512×512
//! `favicon.png` — and the `.ico`, the apple-touch icon and the two web-manifest
//! icons are rasterised from it at build time. Five files of setup collapse to
//! one, and they cannot drift from the source.
//!
//! Two formats because vector is not always available. SVG is preferred: it
//! scales exactly, and it can carry its own `prefers-color-scheme` rules, which
//! gives a dark-mode-adaptive tab icon for free. But a photographic mark cannot
//! be vectorised well — autotracing a face yields either a posterised caricature
//! or a multi-megabyte pile of paths — so a raster source is the honest answer
//! there. Downscaling 512 → 192 → 180 → 32 is faithful; upscaling never is.
//!
//! Exactly one of the two must exist. Both is an error rather than a preference,
//! because two sources silently drift the moment someone updates one of them.
//!
//! **An SVG must not rely on system fonts.** `resvg` is built without them, so
//! any `<text>` should be converted to paths.

use std::path::{Path, PathBuf};

use resvg::tiny_skia::{FilterQuality, Pixmap, PixmapPaint, Transform};
use resvg::usvg::{Options, Tree};
use tracing::info;

use super::error::CacheBusterError;
use super::generate::{FAVICON_DIRECTORY, FAVICON_PNG_SOURCE, FAVICON_SVG_SOURCE};
use super::manifest::Manifest;

/// The exact dimensions a raster source must have.
///
/// Not a minimum. Every size the set needs is a downscale from 512, so this is
/// the natural source size, it makes `icon-512.png` a pixel-perfect copy, and
/// one exact number is a clearer instruction than a range.
pub const SOURCE_PNG_SIZE: u32 = 512;

/// One icon derived from the source.
pub struct DerivedIcon {
    pub file_name: &'static str,
    pub size: u32,
    format: IconFormat,
}

enum IconFormat {
    Png,
    /// A single-image ICO wrapping a PNG, which every browser since IE 11
    /// understands and which avoids hand-rolling a BMP encoder.
    Ico,
}

/// The full derived set, in the order they are generated.
pub const DERIVED: [DerivedIcon; 4] = [
    DerivedIcon {
        file_name: "favicon.ico",
        size: 32,
        format: IconFormat::Ico,
    },
    DerivedIcon {
        file_name: "apple-touch-icon.png",
        size: 180,
        format: IconFormat::Png,
    },
    DerivedIcon {
        file_name: "icon-192.png",
        size: 192,
        format: IconFormat::Png,
    },
    DerivedIcon {
        file_name: "icon-512.png",
        size: 512,
        format: IconFormat::Png,
    },
];

/// Every file the icon directory may contain: one source, and the four derived
/// from it.
///
/// A closed set rather than a minimum. Anything else in there is either a stale
/// icon from a previous design or a size somebody expected to be picked up and
/// which never will be — both of which are silent until someone notices the
/// wrong picture in a browser tab.
pub const ALLOWED_FAVICON_FILES: [&str; 6] = [
    "favicon.svg",
    "favicon-512.png",
    "favicon.ico",
    "apple-touch-icon.png",
    "icon-192.png",
    "icon-512.png",
];

/// Which source a project authored.
pub enum IconSource {
    /// Vector. Also earns the `<link rel="icon" type="image/svg+xml">` and the
    /// `/favicon.svg` route.
    Svg(PathBuf),
    /// Raster, exactly [`SOURCE_PNG_SIZE`] square.
    Png(PathBuf),
}

impl IconSource {
    /// Whether the layout should link an SVG icon.
    #[must_use]
    pub const fn is_vector(&self) -> bool {
        matches!(self, Self::Svg(_))
    }
}

/// Finds the project's single icon source.
///
/// # Errors
///
/// [`CacheBusterError::MissingFavicon`] if neither exists,
/// [`CacheBusterError::AmbiguousFavicon`] if both do, or
/// [`CacheBusterError::FaviconDimensions`] if a PNG source is the wrong size.
pub fn resolve_source(manifest: &Manifest) -> Result<IconSource, CacheBusterError> {
    let svg: Option<PathBuf> = existing(manifest, FAVICON_SVG_SOURCE);
    let png: Option<PathBuf> = existing(manifest, FAVICON_PNG_SOURCE);

    match (svg, png) {
        (Some(_), Some(_)) => Err(CacheBusterError::AmbiguousFavicon),
        (Some(svg), None) => Ok(IconSource::Svg(svg)),
        (None, Some(png)) => {
            let (width, height) = dimensions(&png)?;
            if width != SOURCE_PNG_SIZE || height != SOURCE_PNG_SIZE {
                return Err(CacheBusterError::FaviconDimensions {
                    path: PathBuf::from(FAVICON_PNG_SOURCE),
                    expected: SOURCE_PNG_SIZE,
                    width,
                    height,
                });
            }
            Ok(IconSource::Png(png))
        }
        (None, None) => Err(CacheBusterError::MissingFavicon),
    }
}

/// Renders every derived icon that does not already exist.
///
/// A file already present is left alone, so a hand-tuned small icon — the one
/// size where a naive downscale of a detailed mark really does look muddy —
/// still wins.
///
/// # Errors
///
/// [`CacheBusterError`] if the source cannot be resolved, parsed, rasterised, or
/// an output cannot be written.
pub fn generate_missing_icons(manifest: &Manifest) -> Result<(), CacheBusterError> {
    let source: IconSource = resolve_source(manifest)?;

    let outstanding: Vec<&DerivedIcon> = DERIVED
        .iter()
        .filter(|icon| {
            existing(manifest, &format!("{FAVICON_DIRECTORY}/{}", icon.file_name)).is_none()
        })
        .collect();
    if outstanding.is_empty() {
        return Ok(());
    }

    let renderer: Renderer = Renderer::open(&source)?;
    for icon in outstanding {
        let png: Vec<u8> = renderer.rasterize(icon.size)?;
        let bytes: Vec<u8> = match icon.format {
            IconFormat::Png => png,
            IconFormat::Ico => wrap_png_in_ico(&png, icon.size),
        };

        let path: PathBuf = Path::new(FAVICON_DIRECTORY).join(icon.file_name);
        std::fs::write(&path, bytes)
            .map_err(|source| CacheBusterError::WriteIcon { path, source })?;
        info!("generated `{FAVICON_DIRECTORY}/{}`", icon.file_name);
    }

    Ok(())
}

/// Whatever the source is, reduced to "give me a square PNG of size N".
enum Renderer {
    Svg(Box<Tree>),
    Png(Box<Pixmap>),
}

impl Renderer {
    fn open(source: &IconSource) -> Result<Self, CacheBusterError> {
        match source {
            IconSource::Svg(path) => {
                let svg: String =
                    std::fs::read_to_string(path).map_err(|error| CacheBusterError::ReadFile {
                        path: path.clone(),
                        source: error,
                    })?;
                let tree: Tree = Tree::from_str(&svg, &Options::default())
                    .map_err(|source| CacheBusterError::RenderIcon { source })?;
                Ok(Self::Svg(Box::new(tree)))
            }
            IconSource::Png(path) => {
                let bytes: Vec<u8> =
                    std::fs::read(path).map_err(|error| CacheBusterError::ReadFile {
                        path: path.clone(),
                        source: error,
                    })?;
                let pixmap: Pixmap =
                    Pixmap::decode_png(&bytes).map_err(|error| CacheBusterError::EncodeIcon {
                        reason: error.to_string(),
                    })?;
                Ok(Self::Png(Box::new(pixmap)))
            }
        }
    }

    fn rasterize(&self, size: u32) -> Result<Vec<u8>, CacheBusterError> {
        let mut target: Pixmap =
            Pixmap::new(size, size).ok_or(CacheBusterError::IconDimensions { size })?;
        let requested: f32 = f32::from(u16::try_from(size).unwrap_or(u16::MAX));

        match self {
            Self::Svg(tree) => {
                let source = tree.size();
                let transform: Transform =
                    Transform::from_scale(requested / source.width(), requested / source.height());
                resvg::render(tree, transform, &mut target.as_mut());
            }
            Self::Png(pixmap) => {
                let source: f32 = f32::from(u16::try_from(pixmap.width()).unwrap_or(1));
                target.draw_pixmap(
                    0,
                    0,
                    pixmap.as_ref().as_ref(),
                    &PixmapPaint {
                        // Every derived size is a downscale from 512, which is
                        // where a good filter earns its keep.
                        quality: FilterQuality::Bicubic,
                        ..PixmapPaint::default()
                    },
                    Transform::from_scale(requested / source, requested / source),
                    None,
                );
            }
        }

        target
            .encode_png()
            .map_err(|error| CacheBusterError::EncodeIcon {
                reason: error.to_string(),
            })
    }
}

/// Finds an asset by its hashed name, falling back to its logical one.
///
/// The fallback covers only the pre-hash state: during the build the source is
/// still `favicon.svg`, and after it the manifest knows it as
/// `favicon.<hash>.svg`. It is not a fallback for a *missing* asset — that is
/// caught at boot, loudly.
fn existing(manifest: &Manifest, logical: &str) -> Option<PathBuf> {
    let hashed: PathBuf = PathBuf::from(manifest.resolve(logical));
    if hashed.is_file() {
        return Some(hashed);
    }
    let plain: PathBuf = PathBuf::from(logical);
    plain.is_file().then_some(plain)
}

/// Reads an image's real dimensions.
///
/// # Errors
///
/// [`CacheBusterError::ReadImage`] if the file is absent or not an image.
pub fn dimensions(path: &Path) -> Result<(u32, u32), CacheBusterError> {
    let size = imagesize::size(path).map_err(|error| CacheBusterError::ReadImage {
        path: path.to_path_buf(),
        reason: error.to_string(),
    })?;
    Ok((
        u32::try_from(size.width).unwrap_or(0),
        u32::try_from(size.height).unwrap_or(0),
    ))
}

/// Wraps PNG bytes in a single-image ICO container.
///
/// The format is a 6-byte directory header plus one 16-byte entry, and modern
/// ICOs may carry PNG payloads verbatim — so this is a header, not an encoder.
fn wrap_png_in_ico(png: &[u8], size: u32) -> Vec<u8> {
    // 0 means 256 in this field, which is exactly what we want at that size.
    let dimension: u8 = u8::try_from(size).unwrap_or(0);
    let length: u32 = u32::try_from(png.len()).unwrap_or(u32::MAX);

    let mut ico: Vec<u8> = Vec::with_capacity(22 + png.len());
    ico.extend_from_slice(&0u16.to_le_bytes()); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // type: icon
    ico.extend_from_slice(&1u16.to_le_bytes()); // image count
    ico.push(dimension); // width
    ico.push(dimension); // height
    ico.push(0); // palette size
    ico.push(0); // reserved
    ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
    ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    ico.extend_from_slice(&length.to_le_bytes()); // payload size
    ico.extend_from_slice(&22u32.to_le_bytes()); // payload offset
    ico.extend_from_slice(png);
    ico
}

#[cfg(test)]
mod tests {
    use super::{DERIVED, SOURCE_PNG_SIZE, wrap_png_in_ico};

    #[test]
    fn an_ico_carries_the_png_verbatim_after_a_22_byte_header() {
        let png: [u8; 8] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];

        let ico: Vec<u8> = wrap_png_in_ico(&png, 32);

        let expected_len: usize = 22 + png.len();
        let actual_len: usize = ico.len();
        assert_eq!(expected_len, actual_len);

        assert_eq!(&png[..], &ico[22..]);
        assert_eq!(1u16, u16::from_le_bytes([ico[2], ico[3]]));
        assert_eq!(1u16, u16::from_le_bytes([ico[4], ico[5]]));
        assert_eq!(32u8, ico[6]);
        assert_eq!(32u8, ico[7]);
    }

    #[test]
    fn a_256_pixel_icon_records_its_dimension_as_zero_per_the_format() {
        let ico: Vec<u8> = wrap_png_in_ico(&[0u8; 4], 256);

        let expected: u8 = 0;
        let actual: u8 = ico[6];
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_derived_size_is_a_downscale_from_the_source() {
        // Upscaling is what makes a generated icon look soft, so the source
        // must be at least as large as everything derived from it.
        for icon in &DERIVED {
            assert!(
                icon.size <= SOURCE_PNG_SIZE,
                "`{}` is {}px, larger than the {SOURCE_PNG_SIZE}px source",
                icon.file_name,
                icon.size
            );
        }
    }
}
