//! Per-server template data: what is true of every page on the site.

use serde::{Deserialize, Serialize};

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
        }
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

/// Todd Everett Griffin's other web presences, emitted as `og:see_also`.
#[cfg(feature = "preset")]
pub const GODDTRIFFIN_SEE_ALSO: [&str; 8] = [
    "https://www.toddgriffin.me/",
    "https://x.com/goddtriffin",
    "https://github.com/goddtriffin",
    "https://www.instagram.com/goddtriffin/",
    "https://www.youtube.com/@goddtriffin",
    "https://www.facebook.com/goddtriffin/",
    "https://stackoverflow.com/users/11767294/goddtriffin",
    "https://www.reddit.com/user/goddtriffin",
];

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
    fn the_preset_see_also_leads_with_the_personal_site_then_x_then_github() {
        use super::GODDTRIFFIN_SEE_ALSO;

        let expected: [&str; 3] = [
            "https://www.toddgriffin.me/",
            "https://x.com/goddtriffin",
            "https://github.com/goddtriffin",
        ];
        let actual: [&str; 3] = [
            GODDTRIFFIN_SEE_ALSO[0],
            GODDTRIFFIN_SEE_ALSO[1],
            GODDTRIFFIN_SEE_ALSO[2],
        ];
        assert_eq!(expected, actual);

        assert!(
            !GODDTRIFFIN_SEE_ALSO
                .iter()
                .any(|url| url.contains("twitter.com")),
            "twitter.com was replaced by x.com"
        );
    }

    #[cfg(feature = "preset")]
    #[test]
    fn a_preset_field_can_still_be_overridden() {
        use super::GoddtriffinParams;

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

        let expected_len: usize = 9;
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
