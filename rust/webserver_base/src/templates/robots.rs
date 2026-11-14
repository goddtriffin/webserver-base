//! `robots` directives for the situations that actually come up.

/// Maximum indexability, with no snippet or preview limits.
///
/// What a page gets when it does not name a directive.
pub const DEFAULT: &str =
    "index, follow, max-snippet:-1, max-image-preview:large, max-video-preview:-1";

/// Keep this page out of the index, but keep crawling its links.
///
/// For 404s, search results, thank-you pages, and paginated duplicates — pages
/// with no business being a search result but whose links still matter.
pub const NOINDEX_FOLLOW: &str = "noindex, follow";

/// Keep this page out of the index and do not crawl its links.
///
/// For admin panels and anything behind auth.
pub const NOINDEX_NOFOLLOW: &str = "noindex, nofollow";

/// Index this page, but do not vouch for its links.
///
/// For pages carrying user-submitted URLs.
pub const INDEX_NOFOLLOW: &str = "index, nofollow";

/// Index normally, but serve no cached copy.
pub const NOARCHIVE: &str = "index, follow, noarchive";
