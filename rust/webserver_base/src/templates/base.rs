//! Per-server template data: what is true of every page on the site.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::site_entity::SiteEntity;
use super::theme::{Fallback, ThemeColor, ThemeScript};

/// Everything [`BaseTemplateData::new`] requires.
///
/// A named-field struct rather than sixteen positional arguments: every field
/// is mandatory either way, but this one cannot be built with two of them
/// transposed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BaseTemplateDataParams {
    /// The site's name, e.g. `Boggledygook`.
    pub project: String,
    /// The default `<meta name="description">`.
    pub description: String,
    /// Who wrote it.
    pub author: String,
    /// The site's origin, without a trailing slash — `https://www.example.com`.
    pub base_url: String,
    /// The Twitter/X handle, without the leading `@`.
    pub twitter_username: String,
    /// The default Open Graph / Twitter card image.
    pub social_image: String,
    /// The default alt text for that image.
    pub social_image_alt: String,
    /// The browser-chrome colour, and the schemes this site supports.
    pub theme_color: ThemeColor,
    /// Which theme to stamp when neither the reader nor their OS states a
    /// preference. Design data, so it lives beside the colours it chooses
    /// between.
    pub theme_fallback: Fallback,
    /// Who or what the site represents, for schema.org.
    pub site_entity: SiteEntity,
    /// The `<html lang>` language subtag, e.g. `en`.
    pub language_code: String,
    /// The region subtag, e.g. `US`.
    pub country_code: String,
    /// Other places this author exists, emitted as `og:see_also`.
    pub see_also: Vec<String>,
    /// The first year of the copyright range. The last is computed per render.
    pub copyright_start: String,
    /// Stylesheets every page loads.
    pub style_sheets: Vec<String>,
    /// Scripts every page loads.
    pub scripts: Vec<String>,
}

/// Per-server template data.
///
/// Built once at boot and held in the server's state. Everything on it can be
/// overridden for a single page by [`PageTemplateData`](super::PageTemplateData),
/// except the fields that identify the site itself.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BaseTemplateData {
    project: String,
    description: String,
    author: String,
    base_url: String,
    twitter_username: String,
    social_image: String,
    social_image_alt: String,
    theme_color: ThemeColor,
    theme_fallback: Fallback,
    site_entity: SiteEntity,
    language_code: String,
    country_code: String,
    see_also: Vec<String>,
    copyright_start: String,
    style_sheets: Vec<String>,
    scripts: Vec<String>,
    #[serde(default)]
    site: Value,
}

impl BaseTemplateData {
    /// Builds base data with every field supplied.
    ///
    /// Two paths are normalised on the way in. A trailing slash on `base_url`
    /// is stripped, so joining it with a page URL cannot produce
    /// `https://example.com//blog`. A *leading* slash on `social_image` is
    /// stripped, so it keys into the cache-buster map exactly like the
    /// stylesheet and script paths beside it — without that, the social card is
    /// the one asset on the site that can never be content-hashed.
    #[must_use]
    pub fn new(params: BaseTemplateDataParams) -> Self {
        Self {
            project: params.project,
            description: params.description,
            author: params.author,
            base_url: normalize_base_url(&params.base_url),
            twitter_username: params.twitter_username,
            social_image: normalize_asset_path(&params.social_image),
            social_image_alt: params.social_image_alt,
            theme_color: params.theme_color,
            theme_fallback: params.theme_fallback,
            site_entity: params.site_entity,
            language_code: params.language_code,
            country_code: params.country_code,
            see_also: params.see_also,
            copyright_start: params.copyright_start,
            style_sheets: params.style_sheets,
            scripts: params.scripts,
            site: Value::Null,
        }
    }

    /// The project's own site-wide data, reachable on every page — the 404
    /// included — as `{{site.…}}`.
    ///
    /// For what every page shares but the library has no field for: a nav
    /// tree, a phone number, opening hours. Supplying it once here is what
    /// lets a partial read it on a page whose `{{app}}` knows nothing about it.
    ///
    /// # Panics
    ///
    /// If `site` cannot be serialized — at boot, never per request.
    #[must_use]
    pub fn with_site<A>(mut self, site: A) -> Self
    where
        A: Serialize,
    {
        self.site = serde_json::to_value(site)
            .unwrap_or_else(|error| panic!("the site data is unserializable: {error}"));
        self
    }

    /// Overrides the author.
    #[must_use]
    pub fn with_author(mut self, author: impl Into<String>) -> Self {
        self.author = author.into();
        self
    }

    /// Overrides the Twitter/X handle.
    #[must_use]
    pub fn with_twitter_username(mut self, twitter_username: impl Into<String>) -> Self {
        self.twitter_username = twitter_username.into();
        self
    }

    /// Overrides who or what the site represents.
    #[must_use]
    pub fn with_site_entity(mut self, site_entity: SiteEntity) -> Self {
        self.site_entity = site_entity;
        self
    }

    /// Overrides the language subtag.
    #[must_use]
    pub fn with_language_code(mut self, language_code: impl Into<String>) -> Self {
        self.language_code = language_code.into();
        self
    }

    /// Overrides the region subtag.
    #[must_use]
    pub fn with_country_code(mut self, country_code: impl Into<String>) -> Self {
        self.country_code = country_code.into();
        self
    }

    /// Replaces the `og:see_also` list wholesale.
    ///
    /// For a project that should not carry the author's personal links at all.
    #[must_use]
    pub fn replace_see_also<I, S>(mut self, see_also: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.see_also = see_also.into_iter().map(Into::into).collect();
        self
    }

    /// Appends to the `og:see_also` list.
    ///
    /// For a project with its own presence alongside the author's — a band's
    /// `YouTube` channel next to a personal one.
    #[must_use]
    pub fn extend_see_also<I, S>(mut self, see_also: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.see_also.extend(see_also.into_iter().map(Into::into));
        self
    }

    /// Appends to the site-wide stylesheet list.
    #[must_use]
    pub fn extend_style_sheets<I, S>(mut self, style_sheets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.style_sheets
            .extend(style_sheets.into_iter().map(Into::into));
        self
    }

    /// Appends to the site-wide script list.
    #[must_use]
    pub fn extend_scripts<I, S>(mut self, scripts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scripts.extend(scripts.into_iter().map(Into::into));
        self
    }

    /// The project's site-wide data; `null` unless
    /// [`with_site`](Self::with_site) was called.
    #[must_use]
    pub const fn site(&self) -> &Value {
        &self.site
    }

    /// The site's name.
    #[must_use]
    pub fn project(&self) -> &str {
        &self.project
    }
    /// The default description.
    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }
    /// The author.
    #[must_use]
    pub fn author(&self) -> &str {
        &self.author
    }
    /// The site origin, with no trailing slash.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }
    /// The Twitter/X handle.
    #[must_use]
    pub fn twitter_username(&self) -> &str {
        &self.twitter_username
    }
    /// The default social image.
    #[must_use]
    pub fn social_image(&self) -> &str {
        &self.social_image
    }
    /// The default social image alt text.
    #[must_use]
    pub fn social_image_alt(&self) -> &str {
        &self.social_image_alt
    }
    /// The browser-chrome colour and supported schemes.
    #[must_use]
    pub const fn theme_color(&self) -> &ThemeColor {
        &self.theme_color
    }
    /// Who or what the site represents.
    #[must_use]
    pub const fn site_entity(&self) -> &SiteEntity {
        &self.site_entity
    }
    /// The pre-paint theme script this site's design implies.
    #[must_use]
    pub const fn theme_script(&self) -> ThemeScript {
        ThemeScript::new(self.theme_fallback)
    }
    /// The language subtag.
    #[must_use]
    pub fn language_code(&self) -> &str {
        &self.language_code
    }
    /// The region subtag.
    #[must_use]
    pub fn country_code(&self) -> &str {
        &self.country_code
    }
    /// The `og:see_also` list.
    #[must_use]
    pub fn see_also(&self) -> &[String] {
        &self.see_also
    }
    /// The first year of the copyright range.
    #[must_use]
    pub fn copyright_start(&self) -> &str {
        &self.copyright_start
    }
    /// The site-wide stylesheets.
    #[must_use]
    pub fn style_sheets(&self) -> &[String] {
        &self.style_sheets
    }
    /// The site-wide scripts.
    #[must_use]
    pub fn scripts(&self) -> &[String] {
        &self.scripts
    }
}

/// Strips any trailing slashes from a site origin.
fn normalize_base_url(base_url: &str) -> String {
    base_url.trim().trim_end_matches('/').to_string()
}

/// Strips a leading slash, so an asset path matches its cache-buster key.
fn normalize_asset_path(path: &str) -> String {
    path.trim().trim_start_matches('/').to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// preset
// ─────────────────────────────────────────────────────────────────────────────

/// One of Todd Griffin's social profiles: what a project's icon row renders.
#[cfg(feature = "preset")]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Social {
    /// The network's name, for `aria-label` and `title`.
    pub label: &'static str,
    /// The profile URL.
    pub href: &'static str,
    /// SVG path data for a `0 0 24 24` view box, drawn in `currentColor` (Simple Icons 16.36.0, CC0).
    pub icon: &'static str,
}

/// Todd Griffin's social profiles, in the order every project shows them. The single source for both
/// [`GODDTRIFFIN_SEE_ALSO`] and a site's icon row: add, drop or reorder a network here only.
#[cfg(feature = "preset")]
pub const GODDTRIFFIN_SOCIALS: [Social; 8] = [
    Social {
        label: "X",
        href: "https://x.com/goddtriffin",
        icon: "M14.234 10.162 22.977 0h-2.072l-7.591 8.824L7.251 0H.258l9.168 13.343L.258 24H2.33l8.016-9.318L16.749 24h6.993zm-2.837 3.299-.929-1.329L3.076 1.56h3.182l5.965 8.532.929 1.329 7.754 11.09h-3.182z",
    },
    Social {
        label: "LinkedIn",
        href: "https://www.linkedin.com/in/todd-griffin/",
        icon: "M20.447 20.452h-3.554v-5.569c0-1.328-.027-3.037-1.852-3.037-1.853 0-2.136 1.445-2.136 2.939v5.667H9.351V9h3.414v1.561h.046c.477-.9 1.637-1.85 3.37-1.85 3.601 0 4.267 2.37 4.267 5.455v6.286zM5.337 7.433c-1.144 0-2.063-.926-2.063-2.065 0-1.138.92-2.063 2.063-2.063 1.14 0 2.064.925 2.064 2.063 0 1.139-.925 2.065-2.064 2.065zm1.782 13.019H3.555V9h3.564v11.452zM22.225 0H1.771C.792 0 0 .774 0 1.729v20.542C0 23.227.792 24 1.771 24h20.451C23.2 24 24 23.227 24 22.271V1.729C24 .774 23.2 0 22.222 0h.003z",
    },
    Social {
        label: "GitHub",
        href: "https://github.com/goddtriffin",
        icon: "M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.896-.015 3.286 0 .315.21.69.825.57C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12",
    },
    Social {
        label: "YouTube",
        href: "https://www.youtube.com/@goddtriffin",
        icon: "M23.498 6.186a3.016 3.016 0 0 0-2.122-2.136C19.505 3.545 12 3.545 12 3.545s-7.505 0-9.377.505A3.017 3.017 0 0 0 .502 6.186C0 8.07 0 12 0 12s0 3.93.502 5.814a3.016 3.016 0 0 0 2.122 2.136c1.871.505 9.376.505 9.376.505s7.505 0 9.377-.505a3.015 3.015 0 0 0 2.122-2.136C24 15.93 24 12 24 12s0-3.93-.502-5.814zM9.545 15.568V8.432L15.818 12l-6.273 3.568z",
    },
    Social {
        label: "Instagram",
        href: "https://www.instagram.com/goddtriffin/",
        icon: "M7.0301.084c-1.2768.0602-2.1487.264-2.911.5634-.7888.3075-1.4575.72-2.1228 1.3877-.6652.6677-1.075 1.3368-1.3802 2.127-.2954.7638-.4956 1.6365-.552 2.914-.0564 1.2775-.0689 1.6882-.0626 4.947.0062 3.2586.0206 3.6671.0825 4.9473.061 1.2765.264 2.1482.5635 2.9107.308.7889.72 1.4573 1.388 2.1228.6679.6655 1.3365 1.0743 2.1285 1.38.7632.295 1.6361.4961 2.9134.552 1.2773.056 1.6884.069 4.9462.0627 3.2578-.0062 3.668-.0207 4.9478-.0814 1.28-.0607 2.147-.2652 2.9098-.5633.7889-.3086 1.4578-.72 2.1228-1.3881.665-.6682 1.0745-1.3378 1.3795-2.1284.2957-.7632.4966-1.636.552-2.9124.056-1.2809.0692-1.6898.063-4.948-.0063-3.2583-.021-3.6668-.0817-4.9465-.0607-1.2797-.264-2.1487-.5633-2.9117-.3084-.7889-.72-1.4568-1.3876-2.1228C21.2982 1.33 20.628.9208 19.8378.6165 19.074.321 18.2017.1197 16.9244.0645 15.6471.0093 15.236-.005 11.977.0014 8.718.0076 8.31.0215 7.0301.0839m.1402 21.6932c-1.17-.0509-1.8053-.2453-2.2287-.408-.5606-.216-.96-.4771-1.3819-.895-.422-.4178-.6811-.8186-.9-1.378-.1644-.4234-.3624-1.058-.4171-2.228-.0595-1.2645-.072-1.6442-.079-4.848-.007-3.2037.0053-3.583.0607-4.848.05-1.169.2456-1.805.408-2.2282.216-.5613.4762-.96.895-1.3816.4188-.4217.8184-.6814 1.3783-.9003.423-.1651 1.0575-.3614 2.227-.4171 1.2655-.06 1.6447-.072 4.848-.079 3.2033-.007 3.5835.005 4.8495.0608 1.169.0508 1.8053.2445 2.228.408.5608.216.96.4754 1.3816.895.4217.4194.6816.8176.9005 1.3787.1653.4217.3617 1.056.4169 2.2263.0602 1.2655.0739 1.645.0796 4.848.0058 3.203-.0055 3.5834-.061 4.848-.051 1.17-.245 1.8055-.408 2.2294-.216.5604-.4763.96-.8954 1.3814-.419.4215-.8181.6811-1.3783.9-.4224.1649-1.0577.3617-2.2262.4174-1.2656.0595-1.6448.072-4.8493.079-3.2045.007-3.5825-.006-4.848-.0608M16.953 5.5864A1.44 1.44 0 1 0 18.39 4.144a1.44 1.44 0 0 0-1.437 1.4424M5.8385 12.012c.0067 3.4032 2.7706 6.1557 6.173 6.1493 3.4026-.0065 6.157-2.7701 6.1506-6.1733-.0065-3.4032-2.771-6.1565-6.174-6.1498-3.403.0067-6.156 2.771-6.1496 6.1738M8 12.0077a4 4 0 1 1 4.008 3.9921A3.9996 3.9996 0 0 1 8 12.0077",
    },
    Social {
        label: "Facebook",
        href: "https://www.facebook.com/goddtriffin/",
        icon: "M9.101 23.691v-7.98H6.627v-3.667h2.474v-1.58c0-4.085 1.848-5.978 5.858-5.978.401 0 .955.042 1.468.103a8.68 8.68 0 0 1 1.141.195v3.325a8.623 8.623 0 0 0-.653-.036 26.805 26.805 0 0 0-.733-.009c-.707 0-1.259.096-1.675.309a1.686 1.686 0 0 0-.679.622c-.258.42-.374.995-.374 1.752v1.297h3.919l-.386 2.103-.287 1.564h-3.246v8.245C19.396 23.238 24 18.179 24 12.044c0-6.627-5.373-12-12-12s-12 5.373-12 12c0 5.628 3.874 10.35 9.101 11.647Z",
    },
    Social {
        label: "Reddit",
        href: "https://www.reddit.com/user/goddtriffin",
        icon: "M12 0C5.373 0 0 5.373 0 12c0 3.314 1.343 6.314 3.515 8.485l-2.286 2.286C.775 23.225 1.097 24 1.738 24H12c6.627 0 12-5.373 12-12S18.627 0 12 0Zm4.388 3.199c1.104 0 1.999.895 1.999 1.999 0 1.105-.895 2-1.999 2-.946 0-1.739-.657-1.947-1.539v.002c-1.147.162-2.032 1.15-2.032 2.341v.007c1.776.067 3.4.567 4.686 1.363.473-.363 1.064-.58 1.707-.58 1.547 0 2.802 1.254 2.802 2.802 0 1.117-.655 2.081-1.601 2.531-.088 3.256-3.637 5.876-7.997 5.876-4.361 0-7.905-2.617-7.998-5.87-.954-.447-1.614-1.415-1.614-2.538 0-1.548 1.255-2.802 2.803-2.802.645 0 1.239.218 1.712.585 1.275-.79 2.881-1.291 4.64-1.365v-.01c0-1.663 1.263-3.034 2.88-3.207.188-.911.993-1.595 1.959-1.595Zm-8.085 8.376c-.784 0-1.459.78-1.506 1.797-.047 1.016.64 1.429 1.426 1.429.786 0 1.371-.369 1.418-1.385.047-1.017-.553-1.841-1.338-1.841Zm7.406 0c-.786 0-1.385.824-1.338 1.841.047 1.017.634 1.385 1.418 1.385.785 0 1.473-.413 1.426-1.429-.046-1.017-.721-1.797-1.506-1.797Zm-3.703 4.013c-.974 0-1.907.048-2.77.135-.147.015-.241.168-.183.305.483 1.154 1.622 1.964 2.953 1.964 1.33 0 2.47-.81 2.953-1.964.057-.137-.037-.29-.184-.305-.863-.087-1.795-.135-2.769-.135Z",
    },
    Social {
        label: "itch.io",
        href: "https://goddtriffin.itch.io/",
        icon: "M3.13 1.338C2.08 1.96.02 4.328 0 4.95v1.03c0 1.303 1.22 2.45 2.325 2.45 1.33 0 2.436-1.102 2.436-2.41 0 1.308 1.07 2.41 2.4 2.41 1.328 0 2.362-1.102 2.362-2.41 0 1.308 1.137 2.41 2.466 2.41h.024c1.33 0 2.466-1.102 2.466-2.41 0 1.308 1.034 2.41 2.363 2.41 1.33 0 2.4-1.102 2.4-2.41 0 1.308 1.106 2.41 2.435 2.41C22.78 8.43 24 7.282 24 5.98V4.95c-.02-.62-2.082-2.99-3.13-3.612-3.253-.114-5.508-.134-8.87-.133-3.362 0-7.945.053-8.87.133zm6.376 6.477a2.74 2.74 0 0 1-.468.602c-.5.49-1.19.795-1.947.795a2.786 2.786 0 0 1-1.95-.795c-.182-.178-.32-.37-.446-.59-.127.222-.303.412-.486.59a2.788 2.788 0 0 1-1.95.795c-.092 0-.187-.025-.264-.052-.107 1.113-.152 2.176-.168 2.95v.005l-.006 1.167c.02 2.334-.23 7.564 1.03 8.85 1.952.454 5.545.662 9.15.663 3.605 0 7.198-.21 9.15-.664 1.26-1.284 1.01-6.514 1.03-8.848l-.006-1.167v-.004c-.016-.775-.06-1.838-.168-2.95-.077.026-.172.052-.263.052a2.788 2.788 0 0 1-1.95-.795c-.184-.178-.36-.368-.486-.59-.127.22-.265.412-.447.59a2.786 2.786 0 0 1-1.95.794c-.76 0-1.446-.303-1.948-.793a2.74 2.74 0 0 1-.468-.602 2.738 2.738 0 0 1-.463.602 2.787 2.787 0 0 1-1.95.794h-.16a2.787 2.787 0 0 1-1.95-.793 2.738 2.738 0 0 1-.464-.602zm-2.004 2.59v.002c.795.002 1.5 0 2.373.953.687-.072 1.406-.108 2.125-.107.72 0 1.438.035 2.125.107.873-.953 1.578-.95 2.372-.953.376 0 1.876 0 2.92 2.934l1.123 4.028c.832 2.995-.266 3.068-1.636 3.07-2.03-.075-3.156-1.55-3.156-3.025-1.124.184-2.436.276-3.748.277-1.312 0-2.624-.093-3.748-.277 0 1.475-1.125 2.95-3.156 3.026-1.37-.004-2.468-.077-1.636-3.072l1.122-4.027c1.045-2.934 2.545-2.934 2.92-2.934zM12 12.714c-.002.002-2.14 1.964-2.523 2.662l1.4-.056v1.22c0 .056.56.033 1.123.007.562.026 1.124.05 1.124-.008v-1.22l1.4.055C14.138 14.677 12 12.713 12 12.713z",
    },
];

/// Todd Griffin's web presences, emitted as `og:see_also`: his site, then [`GODDTRIFFIN_SOCIALS`] in order.
#[cfg(feature = "preset")]
pub const GODDTRIFFIN_SEE_ALSO: [&str; GODDTRIFFIN_SOCIALS.len() + 1] = {
    let mut see_also: [&str; GODDTRIFFIN_SOCIALS.len() + 1] =
        ["https://www.toddgriffin.me/"; GODDTRIFFIN_SOCIALS.len() + 1];
    let mut index: usize = 0;
    while index < GODDTRIFFIN_SOCIALS.len() {
        see_also[index + 1] = GODDTRIFFIN_SOCIALS[index].href;
        index += 1;
    }
    see_also
};

/// Everything [`BaseTemplateData::goddtriffin`] cannot know for you.
///
/// Absent on purpose: the fields the preset fills in — author, language,
/// region, handle, `site_entity` and `see_also` — and `social_image_alt`, which
/// is derived as `"{project}: {description}"`. Every one of them can still be
/// changed on the returned value with a `with_*`, `replace_*` or `extend_*`
/// method.
#[cfg(feature = "preset")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoddtriffinParams {
    /// The site's name.
    pub project: String,
    /// The default description.
    pub description: String,
    /// The site's origin, without a trailing slash.
    pub base_url: String,
    /// The default social card image.
    pub social_image: String,
    /// The browser-chrome colour, and the schemes this site supports.
    pub theme_color: ThemeColor,
    /// Which theme to stamp when nothing states a preference.
    pub theme_fallback: Fallback,
    /// The first year of the copyright range.
    pub copyright_start: String,
    /// Stylesheets every page loads.
    pub style_sheets: Vec<String>,
    /// Scripts every page loads.
    pub scripts: Vec<String>,
}

#[cfg(feature = "preset")]
impl BaseTemplateData {
    /// Base data pre-filled with Todd Everett Griffin's defaults.
    ///
    /// Fills in author, language, region, Twitter/X handle,
    /// [`GODDTRIFFIN_SEE_ALSO`] and a `Person` site entity, and derives
    /// `social_image_alt` as `"{project}: {description}"`. Everything else is
    /// required, because it differs per project.
    ///
    /// A project that is not *him* — a band, a business — should call
    /// [`with_site_entity`](Self::with_site_entity) with an
    /// [`Organization`](SiteEntity::organization), because the entity node is a
    /// factual claim search engines reconcile against.
    ///
    /// To change what the preset filled in, call
    /// [`replace_see_also`](Self::replace_see_also),
    /// [`extend_see_also`](Self::extend_see_also), or one of the `with_*`
    /// methods on the returned value.
    #[must_use]
    pub fn goddtriffin(params: GoddtriffinParams) -> Self {
        let social_image_alt: String = format!("{}: {}", params.project, params.description);

        Self::new(BaseTemplateDataParams {
            project: params.project,
            description: params.description,
            author: String::from("Todd Everett Griffin"),
            base_url: params.base_url,
            twitter_username: String::from("goddtriffin"),
            social_image: params.social_image,
            social_image_alt,
            theme_color: params.theme_color,
            theme_fallback: params.theme_fallback,
            site_entity: SiteEntity::person("Todd Everett Griffin"),
            language_code: String::from("en"),
            country_code: String::from("US"),
            see_also: GODDTRIFFIN_SEE_ALSO
                .iter()
                .map(|url| (*url).to_string())
                .collect(),
            copyright_start: params.copyright_start,
            style_sheets: params.style_sheets,
            scripts: params.scripts,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{BaseTemplateData, BaseTemplateDataParams, Fallback, SiteEntity, ThemeColor};

    fn params() -> BaseTemplateDataParams {
        BaseTemplateDataParams {
            project: String::from("Test Project"),
            description: String::from("A test."),
            author: String::from("Someone"),
            base_url: String::from("https://www.example.com"),
            twitter_username: String::from("someone"),
            social_image: String::from("static/social.webp"),
            social_image_alt: String::from("A test image."),
            theme_color: ThemeColor::light_dark("#fafafa", "#121212"),
            theme_fallback: Fallback::Dark,
            site_entity: SiteEntity::person("Someone"),
            language_code: String::from("en"),
            country_code: String::from("US"),
            see_also: vec![String::from("https://www.example.com/")],
            copyright_start: String::from("1998"),
            style_sheets: vec![String::from("static/stylesheet/main.css")],
            scripts: vec![String::from("static/script/main.js")],
        }
    }

    #[test]
    fn a_trailing_slash_is_stripped_so_urls_never_double_up() {
        let mut with_slash: BaseTemplateDataParams = params();
        with_slash.base_url = String::from("https://www.example.com/");

        let expected: String = String::from("https://www.example.com");
        let actual: String = BaseTemplateData::new(with_slash).base_url().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_leading_slash_on_the_social_image_is_stripped_so_it_can_be_cache_busted() {
        let mut with_slash: BaseTemplateDataParams = params();
        with_slash.social_image = String::from("/static/image/social/card.webp");

        // The cache-buster map is keyed without a leading slash, exactly like
        // `style_sheets` and `scripts`; a mismatch here is why the social card
        // was historically the one asset that never got hashed.
        let expected: String = String::from("static/image/social/card.webp");
        let actual: String = BaseTemplateData::new(with_slash).social_image().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn every_field_survives_construction() {
        let expected: BaseTemplateDataParams = params();
        let actual: BaseTemplateData = BaseTemplateData::new(params());

        assert_eq!(expected.project, actual.project());
        assert_eq!(expected.description, actual.description());
        assert_eq!(expected.author, actual.author());
        assert_eq!(expected.base_url, actual.base_url());
        assert_eq!(expected.twitter_username, actual.twitter_username());
        assert_eq!(expected.social_image, actual.social_image());
        assert_eq!(expected.social_image_alt, actual.social_image_alt());
        assert_eq!(&expected.theme_color, actual.theme_color());
        assert_eq!(expected.theme_fallback, actual.theme_fallback);
        assert_eq!(&expected.site_entity, actual.site_entity());
        assert_eq!(expected.language_code, actual.language_code());
        assert_eq!(expected.country_code, actual.country_code());
        assert_eq!(expected.see_also, actual.see_also());
        assert_eq!(expected.copyright_start, actual.copyright_start());
        assert_eq!(expected.style_sheets, actual.style_sheets());
        assert_eq!(expected.scripts, actual.scripts());
    }

    #[test]
    fn extend_appends_without_discarding() {
        let base: BaseTemplateData = BaseTemplateData::new(params())
            .extend_see_also(["https://www.youtube.com/@tripleentendreband"])
            .extend_scripts(["static/script/home.js"]);

        let expected_see_also: Vec<String> = vec![
            String::from("https://www.example.com/"),
            String::from("https://www.youtube.com/@tripleentendreband"),
        ];
        let actual_see_also: Vec<String> = base.see_also().to_vec();
        assert_eq!(expected_see_also, actual_see_also);

        let expected_scripts: Vec<String> = vec![
            String::from("static/script/main.js"),
            String::from("static/script/home.js"),
        ];
        let actual_scripts: Vec<String> = base.scripts().to_vec();
        assert_eq!(expected_scripts, actual_scripts);
    }

    #[cfg(feature = "preset")]
    #[test]
    fn the_preset_fills_identity_and_leaves_the_rest_required() {
        use super::{GODDTRIFFIN_SEE_ALSO, GoddtriffinParams};

        let base: BaseTemplateData = BaseTemplateData::goddtriffin(GoddtriffinParams {
            project: String::from("Boggledygook"),
            description: String::from("Boggle, exhaustively."),
            base_url: String::from("https://www.boggledygook.com"),
            social_image: String::from("/og-image.png"),
            theme_color: ThemeColor::light_dark("#fafafa", "#121212"),
            theme_fallback: Fallback::Dark,
            copyright_start: String::from("2025"),
            style_sheets: vec![],
            scripts: vec![],
        });

        let expected_author: String = String::from("Todd Everett Griffin");
        let actual_author: String = base.author().to_string();
        assert_eq!(expected_author, actual_author);

        let expected_handle: String = String::from("goddtriffin");
        let actual_handle: String = base.twitter_username().to_string();
        assert_eq!(expected_handle, actual_handle);

        let expected_entity: SiteEntity = SiteEntity::person("Todd Everett Griffin");
        assert_eq!(&expected_entity, base.site_entity());

        let expected_see_also: Vec<String> = GODDTRIFFIN_SEE_ALSO
            .iter()
            .map(|url| (*url).to_string())
            .collect();
        let actual_see_also: Vec<String> = base.see_also().to_vec();
        assert_eq!(expected_see_also, actual_see_also);
    }

    #[cfg(feature = "preset")]
    #[test]
    fn the_socials_are_in_todds_order_and_each_has_an_icon() {
        use super::GODDTRIFFIN_SOCIALS;

        let expected: [&str; 8] = [
            "X",
            "LinkedIn",
            "GitHub",
            "YouTube",
            "Instagram",
            "Facebook",
            "Reddit",
            "itch.io",
        ];
        let actual: Vec<&str> = GODDTRIFFIN_SOCIALS
            .iter()
            .map(|social| social.label)
            .collect();
        assert_eq!(expected.to_vec(), actual);
        assert!(
            GODDTRIFFIN_SOCIALS
                .iter()
                .all(|social| social.icon.starts_with('M') && social.href.starts_with("https://")),
            "every entry draws an SVG path and links over HTTPS"
        );
    }

    #[cfg(feature = "preset")]
    #[test]
    fn the_preset_see_also_is_the_site_then_the_socials_in_order() {
        use super::GODDTRIFFIN_SEE_ALSO;

        let expected: [&str; 9] = [
            "https://www.toddgriffin.me/",
            "https://x.com/goddtriffin",
            "https://www.linkedin.com/in/todd-griffin/",
            "https://github.com/goddtriffin",
            "https://www.youtube.com/@goddtriffin",
            "https://www.instagram.com/goddtriffin/",
            "https://www.facebook.com/goddtriffin/",
            "https://www.reddit.com/user/goddtriffin",
            "https://goddtriffin.itch.io/",
        ];
        assert_eq!(expected, GODDTRIFFIN_SEE_ALSO);
        assert!(
            !GODDTRIFFIN_SEE_ALSO
                .iter()
                .any(|url| url.contains("twitter.com") || url.contains("stackoverflow.com")),
            "twitter.com was replaced by x.com; Stack Overflow is dropped"
        );
    }

    #[cfg(feature = "preset")]
    #[test]
    fn a_preset_field_can_still_be_overridden() {
        use super::{GODDTRIFFIN_SEE_ALSO, GoddtriffinParams};

        let base: BaseTemplateData = BaseTemplateData::goddtriffin(GoddtriffinParams {
            project: String::from("Triple Entendre"),
            description: String::from("A band."),
            base_url: String::from("https://www.tripleentendreband.com"),
            social_image: String::from("/static/social.webp"),
            theme_color: ThemeColor::dark("#000000"),
            theme_fallback: Fallback::Dark,
            copyright_start: String::from("2019"),
            style_sheets: vec![],
            scripts: vec![],
        })
        .with_twitter_username("tripleentendre")
        .extend_see_also(["https://www.youtube.com/@tripleentendreband"]);

        let expected_handle: String = String::from("tripleentendre");
        let actual_handle: String = base.twitter_username().to_string();
        assert_eq!(expected_handle, actual_handle);

        let expected_len: usize = GODDTRIFFIN_SEE_ALSO.len() + 1;
        let actual_len: usize = base.see_also().len();
        assert_eq!(expected_len, actual_len);
    }

    #[cfg(feature = "preset")]
    #[test]
    fn the_preset_derives_social_image_alt_from_project_and_description() {
        use super::GoddtriffinParams;

        let base: BaseTemplateData = BaseTemplateData::goddtriffin(GoddtriffinParams {
            project: String::from("Scannable Codes"),
            description: String::from("Every scannable code, explained."),
            base_url: String::from("https://www.scannablecodes.com"),
            social_image: String::from("/static/social.webp"),
            theme_color: ThemeColor::light_dark("#fafafa", "#121212"),
            theme_fallback: Fallback::Dark,
            copyright_start: String::from("1998"),
            style_sheets: vec![],
            scripts: vec![],
        });

        let expected: String = String::from("Scannable Codes: Every scannable code, explained.");
        let actual: String = base.social_image_alt().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn replace_see_also_discards_whatever_was_there() {
        let base: BaseTemplateData = BaseTemplateData::new(params())
            .replace_see_also(["https://www.youtube.com/@tripleentendreband"]);

        let expected: Vec<String> =
            vec![String::from("https://www.youtube.com/@tripleentendreband")];
        let actual: Vec<String> = base.see_also().to_vec();
        assert_eq!(expected, actual);
    }

    #[test]
    fn replace_then_extend_composes_in_that_order() {
        let base: BaseTemplateData = BaseTemplateData::new(params())
            .replace_see_also(["https://a.example.com"])
            .extend_see_also(["https://b.example.com"]);

        let expected: Vec<String> = vec![
            String::from("https://a.example.com"),
            String::from("https://b.example.com"),
        ];
        let actual: Vec<String> = base.see_also().to_vec();
        assert_eq!(expected, actual);
    }
}
