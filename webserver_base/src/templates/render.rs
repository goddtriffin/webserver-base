//! The struct Handlebars actually sees.

use std::collections::BTreeMap;

use chrono::{Datelike, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::Environment;

use super::base::BaseTemplateData;
use super::error::TemplateError;
use super::frontend::FrontendRuntime;
use super::page::{Article, PageTemplateData};
use super::theme::ThemeColorTag;

/// What Handlebars sees: base data, page data, what the server computed, and
/// the caller's own.
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
    pub language_code: &'a str,
    pub country_code: &'a str,
    pub see_also: &'a [String],
    pub copyright_start: &'a str,

    // ── merged: page wins over base ───────────────────────────────────────
    pub description: &'a str,
    /// The path as declared, for a page that wants the asset itself — a CSS
    /// background, say. Use `social_image_url` for anything a crawler reads.
    pub social_image: &'a str,
    pub social_image_alt: &'a str,
    pub style_sheets: Vec<&'a str>,
    pub scripts: Vec<&'a str>,

    // ── page identity ─────────────────────────────────────────────────────
    pub page_name: &'a str,
    pub page_url: &'a str,
    pub robots: &'a str,
    /// `website`, or `article` when the page carries article metadata.
    pub og_type: &'static str,
    pub article: Option<&'a Article>,
    /// Escaped for direct embedding in a `<script>` block.
    pub jsonld: Option<String>,

    // ── computed per render ───────────────────────────────────────────────
    /// `"{page_name} | {project}"` — the `<title>`, always.
    pub display_name: String,
    /// `"{base_url}{page_url}"`.
    pub canonical_url: String,
    /// `"{language_code}_{country_code}"`, for `og:locale`.
    pub locale: String,
    /// Absolute and content-hashed. Open Graph requires an absolute URL: a
    /// crawler fetching the page in isolation cannot resolve a relative one, so
    /// a relative `og:image` silently yields no share card at all.
    pub social_image_url: String,
    pub social_image_width: Option<u32>,
    pub social_image_height: Option<u32>,
    pub social_image_type: Option<&'static str>,
    /// Whether to link `favicon.svg`.
    pub has_svg_icon: bool,
    /// Distinct cross-origin hosts referenced by the stylesheets and scripts
    /// below, so the connection is warm before the request that needs it.
    pub preconnect: Vec<String>,
    /// Computed per render, never cached — a long-running server would
    /// otherwise keep claiming last year after New Year's.
    pub copyright_end: i32,

    // ── theme ─────────────────────────────────────────────────────────────
    pub color_scheme: &'static str,
    pub theme_colors: Vec<ThemeColorTag>,

    // ── per-server, from FrontendRuntime ──────────────────────────────────
    pub theme_script: &'a str,
    pub analytics: Option<&'a super::frontend::AnalyticsPaths>,
    pub sentry_browser: Option<&'a super::frontend::SentryBrowser>,

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
        frontend: Option<&'a FrontendRuntime>,
        environment: Environment,
        cache_buster: &'a BTreeMap<String, String>,
        app: A,
    ) -> Result<Self, TemplateError> {
        let canonical_url: String = format!("{}{}", base.base_url(), page.page_url());
        let social_image: &str = page.social_image().unwrap_or_else(|| base.social_image());
        let social_image_url: String = absolute_asset(base.base_url(), social_image, cache_buster);

        let document: Option<Value> =
            merge_jsonld(default_jsonld(base, page, &social_image_url), page.jsonld());
        let jsonld: Option<String> = match document {
            None => None,
            Some(document) => Some(jsonld_script(&document, page.page_url())?),
        };

        let style_sheets: Vec<&str> = page.resolve_style_sheets(base.style_sheets());
        let scripts: Vec<&str> = page.resolve_scripts(base.scripts());
        let preconnect: Vec<String> = preconnect_origins(&style_sheets, &scripts);

        Ok(Self {
            project: base.project(),
            author: base.author(),
            base_url: base.base_url(),
            twitter_username: base.twitter_username(),
            language_code: base.language_code(),
            country_code: base.country_code(),
            see_also: base.see_also(),
            copyright_start: base.copyright_start(),

            description: page.description().unwrap_or_else(|| base.description()),
            social_image,
            social_image_alt: page
                .social_image_alt()
                .unwrap_or_else(|| base.social_image_alt()),
            style_sheets,
            scripts,

            page_name: page.page_name(),
            page_url: page.page_url(),
            robots: page.robots(),
            og_type: page.og_type(),
            article: page.article(),
            jsonld,

            display_name: format!("{} | {}", page.page_name(), base.project()),
            canonical_url,
            locale: format!("{}_{}", base.language_code(), base.country_code()),
            social_image_url,
            social_image_width: frontend.and_then(|f| f.social_image.width),
            social_image_height: frontend.and_then(|f| f.social_image.height),
            social_image_type: frontend.and_then(|f| f.social_image.mime_type),
            has_svg_icon: frontend.is_some_and(|f| f.has_svg_icon),
            preconnect,
            copyright_end: Utc::now().year(),

            color_scheme: base.theme_color().color_scheme(),
            theme_colors: base.theme_color().tags(),

            theme_script: frontend.map_or("", |f| f.theme_script.as_str()),
            analytics: frontend.map(|f| &f.analytics),
            sentry_browser: frontend.map(|f| &f.sentry_browser),

            environment,
            cache_buster,
            app,
        })
    }
}

/// Resolves an asset path to an absolute, content-hashed URL.
///
/// Falls back to the unhashed path when the asset is not in the manifest, which
/// is what happens for an absolute URL a project supplied itself.
fn absolute_asset(base_url: &str, path: &str, cache_buster: &BTreeMap<String, String>) -> String {
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    let key: &str = path.trim_start_matches('/');
    let resolved: &str = cache_buster.get(key).map_or(key, String::as_str);
    format!("{base_url}/{resolved}")
}

/// The distinct origins of every absolute URL in `style_sheets` and `scripts`,
/// in first-seen order.
///
/// Derived rather than configured: a project that adds a CDN stylesheet gets
/// the resource hint without knowing resource hints exist.
fn preconnect_origins(style_sheets: &[&str], scripts: &[&str]) -> Vec<String> {
    let mut origins: Vec<String> = Vec::new();
    for url in style_sheets.iter().chain(scripts.iter()) {
        let Some(origin) = origin_of(url) else {
            continue;
        };
        if !origins.contains(&origin) {
            origins.push(origin);
        }
    }
    origins
}

/// `https://cdn.example.com/a/b.css` → `https://cdn.example.com`.
fn origin_of(url: &str) -> Option<String> {
    let scheme_end: usize = url.find("://")? + 3;
    let authority: &str = &url[scheme_end..];
    let end: usize = authority.find('/').unwrap_or(authority.len());
    Some(format!("{}{}", &url[..scheme_end], &authority[..end]))
}

/// The site's own structured data, emitted on the home page only.
///
/// Google requires `WebSite` markup to live at the domain root and recommends
/// the same for the entity node, so putting either on every page would be
/// wrong rather than merely redundant.
fn default_jsonld(
    base: &BaseTemplateData,
    page: &PageTemplateData,
    social_image_url: &str,
) -> Option<Value> {
    if page.page_url() != "/" {
        return None;
    }

    let website_id: String = format!("{}/#website", base.base_url());
    let entity_id: String = format!("{}/#entity", base.base_url());

    Some(serde_json::json!({
        "@context": "https://schema.org",
        "@graph": [
            {
                "@id": website_id,
                "@type": "WebSite",
                "name": base.project(),
                "url": format!("{}/", base.base_url()),
                "description": base.description(),
                "publisher": { "@id": entity_id },
            },
            {
                "@id": entity_id,
                "@type": base.site_entity().schema_type(),
                "name": base.site_entity().name(),
                "url": format!("{}/", base.base_url()),
                "logo": social_image_url,
                // `sameAs` is how Google reconciles this entity with the
                // profiles it already knows about.
                "sameAs": base.see_also(),
            },
        ],
    }))
}

/// Combines the library's default graph with whatever a page supplied.
///
/// Nodes are matched by `@id`, falling back to `@type`. A supplied node's
/// properties are shallow-merged over the default's, so naming one property
/// does not silently discard the rest; a supplied node matching nothing is
/// appended.
fn merge_jsonld(default: Option<Value>, supplied: Option<&Value>) -> Option<Value> {
    match (default, supplied) {
        (None, None) => None,
        (None, Some(supplied)) => Some(supplied.clone()),
        (Some(default), None) => Some(default),
        (Some(default), Some(supplied)) => {
            let mut nodes: Vec<Value> = graph_nodes(&default);
            for node in graph_nodes(supplied) {
                match nodes.iter_mut().find(|existing| same_node(existing, &node)) {
                    Some(existing) => shallow_merge(existing, &node),
                    None => nodes.push(node),
                }
            }
            Some(serde_json::json!({
                "@context": "https://schema.org",
                "@graph": nodes,
            }))
        }
    }
}

/// Flattens a document into its nodes, whether or not it uses `@graph`.
fn graph_nodes(document: &Value) -> Vec<Value> {
    match document.get("@graph") {
        Some(Value::Array(nodes)) => nodes.clone(),
        _ => match document {
            Value::Array(nodes) => nodes.clone(),
            other => vec![other.clone()],
        },
    }
}

/// Two nodes are the same when their `@id`s match, or — absent an `@id` — their
/// `@type`s do.
fn same_node(left: &Value, right: &Value) -> bool {
    match (left.get("@id"), right.get("@id")) {
        (Some(left), Some(right)) => left == right,
        _ => match (left.get("@type"), right.get("@type")) {
            (Some(left), Some(right)) => left == right,
            _ => false,
        },
    }
}

/// Copies `source`'s properties over `target`'s, one level deep.
fn shallow_merge(target: &mut Value, source: &Value) {
    let (Some(target), Some(source)) = (target.as_object_mut(), source.as_object()) else {
        return;
    };
    for (key, value) in source {
        target.insert(key.clone(), value.clone());
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

    use serde_json::{Value, json};

    use crate::Environment;
    use crate::templates::base::{BaseTemplateData, BaseTemplateDataParams};
    use crate::templates::page::PageTemplateData;
    use crate::templates::site_entity::SiteEntity;
    use crate::templates::theme::ThemeColor;

    use super::{TemplateData, absolute_asset, jsonld_script, origin_of, preconnect_origins};

    fn base() -> BaseTemplateData {
        BaseTemplateData::new(BaseTemplateDataParams {
            project: String::from("Todd Griffin"),
            description: String::from("Base description."),
            author: String::from("Todd Everett Griffin"),
            base_url: String::from("https://www.toddgriffin.me"),
            twitter_username: String::from("goddtriffin"),
            social_image: String::from("static/image/social/card.webp"),
            social_image_alt: String::from("Base card"),
            theme_color: ThemeColor::light_dark("#fafafa", "#121212"),
            theme_fallback: crate::templates::Fallback::Dark,
            site_entity: SiteEntity::person("Todd Everett Griffin"),
            language_code: String::from("en"),
            country_code: String::from("US"),
            see_also: vec![String::from("https://x.com/goddtriffin")],
            copyright_start: String::from("1998"),
            style_sheets: vec![String::from("static/stylesheet/main.css")],
            scripts: vec![String::from("static/script/main.js")],
        })
    }

    fn hashed() -> BTreeMap<String, String> {
        let mut cache: BTreeMap<String, String> = BTreeMap::new();
        cache.insert(
            String::from("static/image/social/card.webp"),
            String::from("static/image/social/card.abc123.webp"),
        );
        cache
    }

    #[test]
    fn the_social_image_url_is_absolute_and_content_hashed() {
        let base: BaseTemplateData = base();
        let page: PageTemplateData = PageTemplateData::new("home", "Home", "/");
        let cache: BTreeMap<String, String> = hashed();

        let actual: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &page, None, Environment::Local, &cache, ())
                .expect("assembles");

        // Open Graph cannot resolve a relative path, and the hash is what lets
        // the card be cached forever.
        let expected: String =
            String::from("https://www.toddgriffin.me/static/image/social/card.abc123.webp");
        assert_eq!(expected, actual.social_image_url);
    }

    #[test]
    fn an_unhashed_social_image_still_resolves_to_an_absolute_url() {
        let expected: String = String::from("https://a.com/static/card.webp");
        let actual: String = absolute_asset("https://a.com", "/static/card.webp", &BTreeMap::new());
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_absolute_asset_url_is_left_alone() {
        let expected: String = String::from("https://cdn.example.com/card.png");
        let actual: String = absolute_asset(
            "https://a.com",
            "https://cdn.example.com/card.png",
            &BTreeMap::new(),
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn preconnect_names_each_cross_origin_host_once() {
        let expected: Vec<String> = vec![String::from("https://cdnjs.cloudflare.com")];
        let actual: Vec<String> = preconnect_origins(
            &[
                "static/stylesheet/main.css",
                "https://cdnjs.cloudflare.com/ajax/libs/highlight.js/styles/a.css",
            ],
            &["https://cdnjs.cloudflare.com/ajax/libs/highlight.js/hl.js"],
        );
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_site_with_no_cross_origin_assets_gets_no_hints() {
        let expected: Vec<String> = Vec::new();
        let actual: Vec<String> =
            preconnect_origins(&["static/stylesheet/main.css"], &["static/script/main.js"]);
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_origin_stops_at_the_first_path_segment() {
        let expected: Option<String> = Some(String::from("https://cdn.example.com:8443"));
        let actual: Option<String> = origin_of("https://cdn.example.com:8443/a/b.css");
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_computed_fields_are_what_the_layout_expects() {
        let base: BaseTemplateData = base();
        let page: PageTemplateData = PageTemplateData::new("404", "404", "/404");
        let cache: BTreeMap<String, String> = BTreeMap::new();

        let actual: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &page, None, Environment::Local, &cache, ())
                .expect("assembles");

        assert_eq!(String::from("404 | Todd Griffin"), actual.display_name);
        assert_eq!(
            String::from("https://www.toddgriffin.me/404"),
            actual.canonical_url
        );
        assert_eq!(String::from("en_US"), actual.locale);
        assert_eq!("light dark", actual.color_scheme);
        assert_eq!(2, actual.theme_colors.len());
        assert_eq!("website", actual.og_type);
    }

    #[test]
    fn the_default_graph_is_emitted_on_the_home_page_only() {
        let base: BaseTemplateData = base();
        let cache: BTreeMap<String, String> = BTreeMap::new();

        let home: PageTemplateData = PageTemplateData::new("home", "Home", "/");
        let rendered: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &home, None, Environment::Local, &cache, ())
                .expect("assembles");
        let jsonld: String = rendered.jsonld.expect("the home page gets a default graph");
        assert!(jsonld.contains("WebSite"));
        assert!(jsonld.contains("Person"));
        // `sameAs` is what Google reconciles the entity against.
        assert!(jsonld.contains("sameAs"));
        assert!(jsonld.contains("https://x.com/goddtriffin"));

        // Google requires WebSite markup at the domain root, so anywhere else
        // it would be wrong rather than merely redundant.
        let inner: PageTemplateData = PageTemplateData::new("blog", "Blog", "/blog");
        let rendered: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &inner, None, Environment::Local, &cache, ())
                .expect("assembles");
        assert_eq!(None, rendered.jsonld);
    }

    #[test]
    fn a_page_supplied_node_merges_over_the_default_without_discarding_it() {
        let base: BaseTemplateData = base();
        let cache: BTreeMap<String, String> = BTreeMap::new();
        let home: PageTemplateData =
            PageTemplateData::new("home", "Home", "/").with_jsonld(json!({
                "@id": "https://www.toddgriffin.me/#website",
                "name": "Overridden",
                "alternateName": "TG",
            }));

        let rendered: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &home, None, Environment::Local, &cache, ())
                .expect("assembles");
        let jsonld: String = rendered.jsonld.expect("a graph");

        // supplied wins
        assert!(jsonld.contains("Overridden"));
        assert!(!jsonld.contains("Todd Griffin\","));
        // supplied adds
        assert!(jsonld.contains("alternateName"));
        // unmentioned defaults survive
        assert!(jsonld.contains("WebSite"));
        assert!(jsonld.contains("Person"));
    }

    #[test]
    fn a_page_supplied_node_matching_nothing_is_appended() {
        let base: BaseTemplateData = base();
        let cache: BTreeMap<String, String> = BTreeMap::new();
        let home: PageTemplateData = PageTemplateData::new("home", "Home", "/")
            .with_jsonld(json!({ "@type": "WebApplication", "name": "Finder" }));

        let rendered: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &home, None, Environment::Local, &cache, ())
                .expect("assembles");
        let jsonld: String = rendered.jsonld.expect("a graph");

        assert!(jsonld.contains("WebApplication"));
        assert!(jsonld.contains("WebSite"));
        assert!(jsonld.contains("Person"));
    }

    #[test]
    fn a_non_home_page_may_still_supply_its_own_document() {
        let base: BaseTemplateData = base();
        let cache: BTreeMap<String, String> = BTreeMap::new();
        let post: PageTemplateData = PageTemplateData::new("blog-post", "A Post", "/blog/a")
            .with_jsonld(json!({ "@type": "BlogPosting", "headline": "A Post" }));

        let rendered: TemplateData<'_, ()> =
            TemplateData::assemble(&base, &post, None, Environment::Local, &cache, ())
                .expect("assembles");
        let jsonld: String = rendered.jsonld.expect("the page supplied one");

        assert!(jsonld.contains("BlogPosting"));
        assert!(!jsonld.contains("WebSite"));
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
            None,
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
}
