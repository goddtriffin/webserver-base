//! The struct Handlebars actually sees.

use std::collections::BTreeMap;

use chrono::{Datelike, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::Environment;

use super::base::BaseTemplateData;
use super::error::TemplateError;
use super::page::PageTemplateData;

/// What Handlebars sees: base data, page data, and the caller's own.
///
/// Borrowed throughout — nothing is copied that was not computed. `A` is the
/// caller's data, reachable in templates as `{{app.…}}`; pass `()` for none.
#[derive(Debug, Serialize)]
pub struct TemplateData<'a, A>
where
    A: Serialize,
{
    // ── site identity, from BaseTemplateData ──────────────────────────────
    pub project: &'a str,
    pub author: &'a str,
    pub base_url: &'a str,
    pub twitter_username: &'a str,
    pub theme_color: &'a str,
    pub language_code: &'a str,
    pub country_code: &'a str,
    pub charset: &'a str,
    pub see_also: &'a [String],
    pub copyright_start: &'a str,

    // ── merged: page wins over base ───────────────────────────────────────
    pub description: &'a str,
    pub keywords: &'a [String],
    pub social_image: &'a str,
    pub social_image_alt: &'a str,
    pub style_sheets: Vec<&'a str>,
    pub scripts: Vec<&'a str>,

    // ── page identity ─────────────────────────────────────────────────────
    pub page_name: &'a str,
    pub page_url: &'a str,
    pub robots: &'a str,
    /// Escaped for direct embedding in a `<script>` block.
    pub jsonld: Option<String>,

    // ── computed per render ───────────────────────────────────────────────
    /// `"{page_name} | {project}"` — the `<title>`, always.
    pub display_name: String,
    /// `"{base_url}{page_url}"`.
    pub canonical_url: String,
    /// `"{language_code}_{country_code}"`, for `og:locale`.
    pub locale: String,
    /// Computed per render, never cached — a long-running server would
    /// otherwise keep claiming last year after New Year's.
    pub copyright_end: i32,

    // ── context ───────────────────────────────────────────────────────────
    pub environment: Environment,
    /// Original asset path to content-hashed path.
    pub cache_buster: &'a BTreeMap<String, String>,
    pub app: A,
}

impl<'a, A> TemplateData<'a, A>
where
    A: Serialize,
{
    /// # Errors
    ///
    /// [`TemplateError::JsonLd`] if the page's JSON-LD document cannot be
    /// serialized.
    pub fn assemble(
        base: &'a BaseTemplateData,
        page: &'a PageTemplateData,
        environment: Environment,
        cache_buster: &'a BTreeMap<String, String>,
        app: A,
    ) -> Result<Self, TemplateError> {
        let jsonld: Option<String> = match page.jsonld() {
            None => None,
            Some(document) => Some(jsonld_script(document, page.page_url())?),
        };

        Ok(Self {
            project: base.project(),
            author: base.author(),
            base_url: base.base_url(),
            twitter_username: base.twitter_username(),
            theme_color: base.theme_color(),
            language_code: base.language_code(),
            country_code: base.country_code(),
            charset: base.charset(),
            see_also: base.see_also(),
            copyright_start: base.copyright_start(),

            description: page.description().unwrap_or_else(|| base.description()),
            keywords: page.keywords().unwrap_or_else(|| base.keywords()),
            social_image: page.social_image().unwrap_or_else(|| base.social_image()),
            social_image_alt: page
                .social_image_alt()
                .unwrap_or_else(|| base.social_image_alt()),
            style_sheets: page.resolve_style_sheets(base.style_sheets()),
            scripts: page.resolve_scripts(base.scripts()),

            page_name: page.page_name(),
            page_url: page.page_url(),
            robots: page.robots(),
            jsonld,

            display_name: format!("{} | {}", page.page_name(), base.project()),
            canonical_url: format!("{}{}", base.base_url(), page.page_url()),
            locale: format!("{}_{}", base.language_code(), base.country_code()),
            copyright_end: Utc::now().year(),

            environment,
            cache_buster,
            app,
        })
    }
}

/// Serializes JSON-LD safely for a `<script type="application/ld+json">` block.
///
/// `<`, `>` and `&` are hex-escaped: without it a `</script>` inside a string
/// value closes the element early and the rest is parsed as markup. They never
/// appear as JSON structural tokens, so the document stays valid.
fn jsonld_script(document: &Value, page_url: &str) -> Result<String, TemplateError> {
    let mut document: Value = document.clone();
    if let Some(object) = document.as_object_mut()
        && !object.contains_key("@context")
    {
        object.insert(
            String::from("@context"),
            Value::String(String::from("https://schema.org")),
        );
    }

    let serialized: String =
        serde_json::to_string(&document).map_err(|source| TemplateError::JsonLd {
            page: page_url.to_string(),
            source,
        })?;

    Ok(serialized
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026"))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use chrono::{Datelike, Utc};
    use serde_json::{Value, json};

    use crate::Environment;
    use crate::templates::base::{BaseTemplateData, BaseTemplateDataParams};
    use crate::templates::page::PageTemplateData;

    use super::{TemplateData, jsonld_script};

    fn base() -> BaseTemplateData {
        BaseTemplateData::new(BaseTemplateDataParams {
            project: String::from("Todd Griffin"),
            description: String::from("Base description."),
            keywords: vec![String::from("base")],
            author: String::from("Todd Everett Griffin"),
            base_url: String::from("https://www.toddgriffin.me"),
            twitter_username: String::from("goddtriffin"),
            social_image: String::from("/static/base.webp"),
            social_image_alt: String::from("Base card"),
            theme_color: String::from("#f7cb64"),
            language_code: String::from("en"),
            country_code: String::from("US"),
            charset: String::from("utf-8"),
            see_also: vec![String::from("https://x.com/goddtriffin")],
            copyright_start: String::from("1998"),
            style_sheets: vec![String::from("static/stylesheet/main.css")],
            scripts: vec![String::from("static/script/main.js")],
        })
    }

    #[test]
    fn the_computed_fields_are_what_the_templates_expect() {
        let base: BaseTemplateData = base();
        let page: PageTemplateData = PageTemplateData::new("404", "404", "/404");
        let cache: BTreeMap<String, String> = BTreeMap::new();

        let actual: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &page, Environment::Local, &cache, ())
                .expect("no jsonld to fail on");

        let expected_display_name: String = String::from("404 | Todd Griffin");
        assert_eq!(expected_display_name, actual.display_name);

        let expected_canonical: String = String::from("https://www.toddgriffin.me/404");
        assert_eq!(expected_canonical, actual.canonical_url);

        let expected_locale: String = String::from("en_US");
        assert_eq!(expected_locale, actual.locale);

        let expected_year: i32 = Utc::now().year();
        assert_eq!(expected_year, actual.copyright_end);
    }

    #[test]
    fn the_home_page_canonical_url_keeps_exactly_one_slash() {
        let base: BaseTemplateData = base();
        let page: PageTemplateData = PageTemplateData::new("home", "Home", "/");
        let cache: BTreeMap<String, String> = BTreeMap::new();

        let actual: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &page, Environment::Production, &cache, ())
                .expect("assembles");

        let expected: String = String::from("https://www.toddgriffin.me/");
        assert_eq!(expected, actual.canonical_url);
    }

    #[test]
    fn a_page_override_beats_the_base_and_absence_falls_through() {
        let base: BaseTemplateData = base();
        let page: PageTemplateData = PageTemplateData::new("blog", "Blog", "/blog")
            .with_description("A page description.")
            .with_social_image("/static/blog.webp");
        let cache: BTreeMap<String, String> = BTreeMap::new();

        let actual: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &page, Environment::Local, &cache, ())
                .expect("assembles");

        assert_eq!("A page description.", actual.description);
        assert_eq!("/static/blog.webp", actual.social_image);
        // not overridden, so the base value survives
        assert_eq!("Base card", actual.social_image_alt);
        assert_eq!(&[String::from("base")], actual.keywords);
    }

    #[test]
    fn app_data_is_namespaced_rather_than_flattened() {
        #[derive(serde::Serialize)]
        struct BlogView {
            posts: Vec<&'static str>,
        }

        let base: BaseTemplateData = base();
        let page: PageTemplateData = PageTemplateData::new("blog", "Blog", "/blog");
        let cache: BTreeMap<String, String> = BTreeMap::new();

        let data: TemplateData<'_, BlogView> = TemplateData::assemble(
            &base,
            &page,
            Environment::Local,
            &cache,
            BlogView {
                posts: vec!["first"],
            },
        )
        .expect("assembles");

        let serialized: Value = serde_json::to_value(&data).expect("serializes");
        let expected: Value = json!(["first"]);
        let actual: Value = serialized["app"]["posts"].clone();
        assert_eq!(expected, actual);
    }

    #[test]
    fn jsonld_cannot_break_out_of_its_script_block() {
        let hostile: Value = json!({
            "@type": "DefinedTerm",
            "name": "</script><img src=x onerror=alert(1)>",
        });

        let actual: String = jsonld_script(&hostile, "/finder/word").expect("serializes");

        assert!(!actual.contains("</script>"), "the closing tag survived");
        assert!(!actual.contains('<'), "a raw < survived");
        assert!(!actual.contains('>'), "a raw > survived");
        assert!(actual.contains("\\u003c"), "escaping did not happen");
    }

    #[test]
    fn jsonld_gets_a_context_when_one_was_not_supplied() {
        let actual: String =
            jsonld_script(&json!({ "@type": "WebSite" }), "/").expect("serializes");
        assert!(actual.contains(r#""@context":"https://schema.org""#));

        let supplied: String = jsonld_script(
            &json!({ "@context": "https://example.com", "@type": "WebSite" }),
            "/",
        )
        .expect("serializes");
        assert!(supplied.contains("https://example.com"));
        assert!(!supplied.contains("schema.org"));
    }
}
