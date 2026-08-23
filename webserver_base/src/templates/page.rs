//! Per-render template data: what this page is, this time.

use serde_json::Value;

use super::robots;

/// Per-render template data.
///
/// Non-generic on purpose: the data a page displays is passed alongside this to
/// the render call rather than stored on it, so every builder method below is
/// type-preserving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageTemplateData {
    template: String,
    page_name: String,
    page_url: String,

    description: Option<String>,
    keywords: Option<Vec<String>>,
    social_image: Option<String>,
    social_image_alt: Option<String>,
    robots: Option<String>,
    jsonld: Option<Value>,

    style_sheets: Option<Vec<String>>,
    scripts: Option<Vec<String>>,
    extra_style_sheets: Vec<String>,
    extra_scripts: Vec<String>,
}

impl PageTemplateData {
    /// Names a page.
    ///
    /// - `template` is the registered Handlebars name, e.g. `home`.
    /// - `page_name` is the human name, e.g. `Blog`. It becomes the `<title>`
    ///   via the computed `display_name`, which is the *only* way to influence
    ///   the title.
    /// - `page_url` is this page's path from the site root, with a leading
    ///   slash, e.g. `/blog`. It becomes the canonical URL.
    #[must_use]
    pub fn new(
        template: impl Into<String>,
        page_name: impl Into<String>,
        page_url: impl Into<String>,
    ) -> Self {
        Self {
            template: template.into(),
            page_name: page_name.into(),
            page_url: normalize_page_url(&page_url.into()),
            description: None,
            keywords: None,
            social_image: None,
            social_image_alt: None,
            robots: None,
            jsonld: None,
            style_sheets: None,
            scripts: None,
            extra_style_sheets: Vec::new(),
            extra_scripts: Vec::new(),
        }
    }

    /// Overrides the base description for this page.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Overrides the base keywords for this page.
    #[must_use]
    pub fn with_keywords<I, S>(mut self, keywords: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.keywords = Some(keywords.into_iter().map(Into::into).collect());
        self
    }

    /// Overrides the base social card image for this page.
    #[must_use]
    pub fn with_social_image(mut self, social_image: impl Into<String>) -> Self {
        self.social_image = Some(social_image.into());
        self
    }

    /// Overrides the base social card alt text for this page.
    #[must_use]
    pub fn with_social_image_alt(mut self, social_image_alt: impl Into<String>) -> Self {
        self.social_image_alt = Some(social_image_alt.into());
        self
    }

    /// Overrides [`robots::DEFAULT`] for this page. See [`robots`] for the
    /// common directives.
    #[must_use]
    pub fn with_robots(mut self, robots: impl Into<String>) -> Self {
        self.robots = Some(robots.into());
        self
    }

    /// Attaches a schema.org JSON-LD document.
    ///
    /// Takes a [`Value`] rather than a pre-escaped string so the escaping cannot
    /// be skipped — a `</script>` inside a string value would otherwise break
    /// out of the block. `@context` is filled in when absent.
    #[must_use]
    pub fn with_jsonld(mut self, jsonld: Value) -> Self {
        self.jsonld = Some(jsonld);
        self
    }

    /// Replaces the base stylesheet list wholesale for this page.
    #[must_use]
    pub fn replace_style_sheets<I, S>(mut self, style_sheets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.style_sheets = Some(style_sheets.into_iter().map(Into::into).collect());
        self
    }

    /// Appends to whatever stylesheet list this page ends up with.
    #[must_use]
    pub fn extend_style_sheets<I, S>(mut self, style_sheets: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_style_sheets
            .extend(style_sheets.into_iter().map(Into::into));
        self
    }

    /// Replaces the base script list wholesale for this page.
    #[must_use]
    pub fn replace_scripts<I, S>(mut self, scripts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.scripts = Some(scripts.into_iter().map(Into::into).collect());
        self
    }

    /// Appends to whatever script list this page ends up with.
    #[must_use]
    pub fn extend_scripts<I, S>(mut self, scripts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.extra_scripts
            .extend(scripts.into_iter().map(Into::into));
        self
    }

    /// The registered Handlebars template name.
    #[must_use]
    pub fn template(&self) -> &str {
        &self.template
    }
    /// The human page name.
    #[must_use]
    pub fn page_name(&self) -> &str {
        &self.page_name
    }
    /// This page's path from the site root.
    #[must_use]
    pub fn page_url(&self) -> &str {
        &self.page_url
    }
    /// This page's description override, if any.
    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }
    /// This page's keywords override, if any.
    #[must_use]
    pub fn keywords(&self) -> Option<&[String]> {
        self.keywords.as_deref()
    }
    /// This page's social image override, if any.
    #[must_use]
    pub fn social_image(&self) -> Option<&str> {
        self.social_image.as_deref()
    }
    /// This page's social image alt override, if any.
    #[must_use]
    pub fn social_image_alt(&self) -> Option<&str> {
        self.social_image_alt.as_deref()
    }
    /// This page's robots directive, defaulting to [`robots::DEFAULT`].
    #[must_use]
    pub fn robots(&self) -> &str {
        self.robots.as_deref().unwrap_or(robots::DEFAULT)
    }
    /// This page's JSON-LD document, if any.
    #[must_use]
    pub const fn jsonld(&self) -> Option<&Value> {
        self.jsonld.as_ref()
    }

    /// Resolves the final stylesheet list: base, optionally replaced, then
    /// extended.
    #[must_use]
    pub fn resolve_style_sheets<'a>(&'a self, base: &'a [String]) -> Vec<&'a str> {
        resolve(self.style_sheets.as_deref(), base, &self.extra_style_sheets)
    }

    /// Resolves the final script list: base, optionally replaced, then
    /// extended.
    #[must_use]
    pub fn resolve_scripts<'a>(&'a self, base: &'a [String]) -> Vec<&'a str> {
        resolve(self.scripts.as_deref(), base, &self.extra_scripts)
    }
}

/// base → wholesale replacement (if any) → additions.
fn resolve<'a>(
    replacement: Option<&'a [String]>,
    base: &'a [String],
    additions: &'a [String],
) -> Vec<&'a str> {
    replacement
        .unwrap_or(base)
        .iter()
        .chain(additions.iter())
        .map(String::as_str)
        .collect()
}

/// Exactly one leading slash and no trailing one, so `base_url + page_url` is
/// always well-formed. The site root stays a bare `/`.
fn normalize_page_url(page_url: &str) -> String {
    let trimmed: &str = page_url.trim();
    if trimmed.is_empty() || trimmed == "/" {
        return String::from("/");
    }
    let without_trailing: &str = trimmed.trim_end_matches('/');
    if without_trailing.starts_with('/') {
        without_trailing.to_string()
    } else {
        format!("/{without_trailing}")
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{PageTemplateData, normalize_page_url};
    use crate::templates::robots;

    fn base_sheets() -> Vec<String> {
        vec![
            String::from("static/stylesheet/main.css"),
            String::from("static/stylesheet/theme.css"),
        ]
    }

    #[test]
    fn a_page_defaults_to_maximum_indexability() {
        let page: PageTemplateData = PageTemplateData::new("home", "Home", "/");

        let expected: String = String::from(robots::DEFAULT);
        let actual: String = page.robots().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_page_can_opt_out_of_indexing() {
        let page: PageTemplateData =
            PageTemplateData::new("404", "404", "/404").with_robots(robots::NOINDEX_FOLLOW);

        let expected: String = String::from("noindex, follow");
        let actual: String = page.robots().to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn page_urls_are_normalized_to_one_leading_and_no_trailing_slash() {
        let cases: [(&str, &str); 6] = [
            ("/", "/"),
            ("", "/"),
            ("blog", "/blog"),
            ("/blog", "/blog"),
            ("/blog/", "/blog"),
            ("  /blog/tips/  ", "/blog/tips"),
        ];
        for (input, expected) in cases {
            let actual: String = normalize_page_url(input);
            assert_eq!(expected, actual, "input was {input:?}");
        }
    }

    #[test]
    fn assets_default_to_the_base_list() {
        let page: PageTemplateData = PageTemplateData::new("home", "Home", "/");

        let base: Vec<String> = base_sheets();
        let expected: Vec<&str> = vec!["static/stylesheet/main.css", "static/stylesheet/theme.css"];
        let actual: Vec<&str> = page.resolve_style_sheets(&base);
        assert_eq!(expected, actual);
    }

    #[test]
    fn extend_appends_to_the_base_list() {
        let page: PageTemplateData = PageTemplateData::new("home", "Home", "/")
            .extend_style_sheets(["static/stylesheet/home.css"]);

        let expected: Vec<&str> = vec![
            "static/stylesheet/main.css",
            "static/stylesheet/theme.css",
            "static/stylesheet/home.css",
        ];
        let base: Vec<String> = base_sheets();
        let actual: Vec<&str> = page.resolve_style_sheets(&base);
        assert_eq!(expected, actual);
    }

    #[test]
    fn replace_discards_the_base_list_then_extend_still_appends() {
        let page: PageTemplateData = PageTemplateData::new("play", "Play", "/play")
            .replace_style_sheets(["static/stylesheet/play.css"])
            .extend_style_sheets(["static/stylesheet/tiles.css"]);

        let base: Vec<String> = base_sheets();
        let expected: Vec<&str> = vec!["static/stylesheet/play.css", "static/stylesheet/tiles.css"];
        let actual: Vec<&str> = page.resolve_style_sheets(&base);
        assert_eq!(expected, actual);
    }

    #[test]
    fn overrides_are_absent_until_set() {
        let bare: PageTemplateData = PageTemplateData::new("home", "Home", "/");

        assert_eq!(None, bare.description());
        assert_eq!(None, bare.social_image());
        assert_eq!(None, bare.social_image_alt());
        assert_eq!(None, bare.keywords());
        assert_eq!(None, bare.jsonld());

        let overridden: PageTemplateData = bare
            .with_description("A specific page.")
            .with_social_image("/static/image/social/blog.webp")
            .with_social_image_alt("The blog card")
            .with_keywords(["blog", "rust"])
            .with_jsonld(json!({ "@type": "BlogPosting" }));

        assert_eq!(Some("A specific page."), overridden.description());
        assert_eq!(
            Some("/static/image/social/blog.webp"),
            overridden.social_image()
        );
        assert_eq!(Some("The blog card"), overridden.social_image_alt());
        assert_eq!(
            Some(vec![String::from("blog"), String::from("rust")].as_slice()),
            overridden.keywords()
        );
        assert!(overridden.jsonld().is_some());
    }
}
