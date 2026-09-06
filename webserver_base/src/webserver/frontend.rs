//! What it takes to serve HTML to humans.
//!
//! `WebServer::frontend` is the line between a site and a service. Everything a
//! frontend *must* have is a field here, so omitting one is a compile error
//! rather than a page that quietly ships without a share card. A server that
//! never calls it — a sidecar, a JSON API — needs none of it and boots clean.

use crate::analytics::{AnalyticsConfig, SentryDsn};
use crate::assets::CacheBuster;
use crate::sitemap::{SitemapSet, SitemapUrl, build_sitemaps};
use std::sync::Arc;

use serde_json::Value;

use crate::templates::{
    AnalyticsPaths, BaseTemplateData, FrontendRuntime, NOT_FOUND_TEMPLATE_NAME, PageTemplateData,
    SentryBrowser, SocialImageMetadata, robots,
};
use crate::{Environment, env};

use super::error::WebServerError;
use super::pages::Pages;

/// Everything a frontend requires.
///
/// A named-field struct rather than five positional arguments: two of them are
/// strings, and this one cannot be built with them transposed.
pub struct FrontendParams<S = ()> {
    /// What is true of every page on the site.
    pub base: BaseTemplateData,
    /// The pages themselves, which become both routes and sitemap entries.
    pub pages: Pages<S>,
    /// Which Plausible site this frontend reports to.
    pub analytics: AnalyticsConfig,
    /// The browser Sentry DSN — separate from the server's, because Sentry
    /// recommends a project per language and per deployable, and mixing Rust
    /// panics with JavaScript exceptions in one stream helps nobody.
    pub sentry_browser_dsn: String,
}

impl<S> FrontendParams<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Reads the analytics script id and browser DSN from the environment.
    ///
    /// # Errors
    ///
    /// [`WebServerError::Env`] if either variable is unset or blank.
    pub fn from_env(base: BaseTemplateData, pages: Pages<S>) -> Result<Self, WebServerError> {
        Ok(Self {
            base,
            pages,
            analytics: AnalyticsConfig::from_env().map_err(WebServerError::Env)?,
            sentry_browser_dsn: env::required(crate::analytics::ENV_SENTRY_BROWSER_DSN)
                .map_err(WebServerError::Env)?,
        })
    }
}

/// The well-known documents a frontend serves, held in memory.
///
/// None of these is written to disk. They are generated (robots, sitemaps, the
/// web manifest) or embedded (humans.txt), they are served at fixed never-cached
/// routes where a content hash would mean nothing, and keeping them in memory is
/// what lets a production image be read-only.
#[derive(Debug, Clone)]
pub struct WellKnown {
    pub robots_txt: String,
    pub humans_txt: String,
    pub webmanifest: String,
    pub sitemaps: SitemapSet,
}

/// A frontend, assembled.
pub struct Frontend<S> {
    pub pages: Pages<S>,
    pub base: BaseTemplateData,
    pub runtime: FrontendRuntime,
    pub well_known: WellKnown,
    pub analytics: AnalyticsConfig,
    pub sentry_dsn: SentryDsn,
    /// The 404, declared by the library rather than by each project.
    pub not_found: (Arc<PageTemplateData>, Arc<Value>),
    /// Whether an SVG icon exists to link.
    pub has_svg_icon: bool,
}

/// The bytes of Todd Everett Griffin's humans.txt, shared by every project that
/// enables `preset` so it is written once and never drifts.
#[cfg(feature = "preset")]
const PRESET_HUMANS_TXT: &str = include_str!("../../assets/humans.txt");

impl<S> Frontend<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Builds a frontend: derives the proxy paths, probes the social image,
    /// and renders the well-known documents.
    ///
    /// # Errors
    ///
    /// [`WebServerError::SentryDsn`] if the browser DSN is malformed, or
    /// [`WebServerError::Sitemap`] if the sitemaps cannot be built.
    pub fn build(
        params: FrontendParams<S>,
        cache_buster: &CacheBuster,
        environment: Environment,
    ) -> Result<Self, WebServerError> {
        let sentry_dsn: SentryDsn =
            SentryDsn::parse(&params.sentry_browser_dsn).map_err(WebServerError::SentryDsn)?;

        // Every asset a page will reference is proved to resolve before the
        // server binds. Falling back to an un-hashed path would produce a link
        // that 404s for the visitor and reports nothing to us.
        let mut declared: Vec<String> = Vec::new();
        declared.extend(params.base.style_sheets().iter().cloned());
        declared.extend(params.base.scripts().iter().cloned());
        declared.push(params.base.social_image().to_string());
        declared.extend(params.pages.declared_assets());
        crate::assets::validate_declared(cache_buster, &declared)?;

        let icon_source: crate::assets::IconSource = crate::assets::validate_icons(cache_buster)?;

        let paths: ProxyPaths = ProxyPaths::derive(params.base.project());
        let social_image: SocialImageMetadata = probe_social_image(&params.base, cache_buster);

        if !social_image.is_large_enough() {
            // Boots, but every X share renders a thumbnail instead of the wide
            // card the layout declares — exactly the kind of silent degradation
            // that must reach Sentry, and `warn!` never does.
            tracing::error!(
                "the social image is {}x{}, below the 600x314 floor for a \
                 `summary_large_image` card; shares will degrade to a thumbnail",
                social_image.width.unwrap_or_default(),
                social_image.height.unwrap_or_default(),
            );
        }

        // Images are declared logically but served hashed; the sitemap has to
        // name the URL that actually resolves.
        let sitemap_urls: Vec<SitemapUrl> = params
            .pages
            .sitemap_urls()?
            .into_iter()
            .map(|url| url.map_images(|image| cache_buster.get_file(image)))
            .collect();
        let last_modified: Option<chrono::DateTime<chrono::Utc>> =
            crate::assets::content_modified(&["html", "static"]);
        let sitemaps: SitemapSet =
            build_sitemaps(params.base.base_url(), &sitemap_urls, last_modified)
                .map_err(WebServerError::Sitemap)?;

        let well_known: WellKnown = WellKnown {
            robots_txt: robots_txt(params.base.base_url(), &sitemaps),
            humans_txt: humans_txt(&params.base),
            webmanifest: webmanifest(&params.base),
            sitemaps,
        };

        let runtime: FrontendRuntime = FrontendRuntime {
            theme_script: params.base.theme_script().source(),
            social_image,
            has_svg_icon: icon_source.is_vector(),
            analytics: AnalyticsPaths {
                script_path: paths.analytics_script.clone(),
                event_path: paths.analytics_event.clone(),
            },
            sentry_browser: SentryBrowser {
                script_path: paths.sentry_script.clone(),
                tunnel_path: paths.sentry_tunnel.clone(),
                environment: environment.to_string(),
            },
        };

        let not_found: (Arc<PageTemplateData>, Arc<Value>) = (
            Arc::new(
                PageTemplateData::new(NOT_FOUND_TEMPLATE_NAME, "404", "/404")
                    .with_robots(robots::NOINDEX_FOLLOW),
            ),
            Arc::new(Value::Null),
        );

        Ok(Self {
            pages: params.pages,
            base: params.base,
            runtime,
            well_known,
            analytics: params.analytics,
            sentry_dsn,
            not_found,
            has_svg_icon: icon_source.is_vector(),
        })
    }
}

/// Where the first-party proxies live on this origin.
///
/// Derived from the project name, never configured. Plausible's guidance is to
/// avoid their documented default paths because blocklists target them, and to
/// avoid words like "analytics" or "stats". A per-project name also means no
/// single filter rule can take out every site at once, which a shared
/// library-wide constant would invite. The shape — `name-hash.js` — is what
/// every bundler on the web already emits.
#[derive(Debug, Clone, PartialEq, Eq)]
struct ProxyPaths {
    analytics_script: String,
    analytics_event: String,
    sentry_script: String,
    sentry_tunnel: String,
}

impl ProxyPaths {
    fn derive(project: &str) -> Self {
        let slug: String = slugify(project);
        let analytics: String = short_hash(project);
        // A second, unrelated hash, so the two scripts do not visibly belong to
        // one another.
        let sentry: String = short_hash(&format!("{project}:sentry"));

        Self {
            analytics_script: format!("/script/{slug}-{analytics}.js"),
            analytics_event: format!("{}/{slug}-{analytics}", super::server::API_PREFIX),
            sentry_script: format!("/script/{slug}-{sentry}.js"),
            sentry_tunnel: format!("{}/{slug}-{sentry}", super::server::API_PREFIX),
        }
    }
}

/// `Boggledygook!` → `boggledygook`.
fn slugify(project: &str) -> String {
    let mut slug: String = String::with_capacity(project.len());
    let mut previous_dash: bool = true;
    for character in project.chars() {
        if character.is_ascii_alphanumeric() {
            slug.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            slug.push('-');
            previous_dash = true;
        }
    }
    String::from(slug.trim_matches('-'))
}

/// Eight hex characters: enough to be unguessable, short enough to look like
/// ordinary bundler output.
fn short_hash(input: &str) -> String {
    format!("{:x}", md5::compute(input.as_bytes()))
        .chars()
        .take(8)
        .collect()
}

/// Resolves and measures the social card image on disk.
fn probe_social_image(base: &BaseTemplateData, cache_buster: &CacheBuster) -> SocialImageMetadata {
    let hashed: String = cache_buster.get_file(base.social_image());
    crate::assets::probe_social_image(&hashed)
}

/// `robots.txt`, naming the sitemap index first and then every url set.
///
/// Index first is deliberate: Google discards a robots.txt past 500 KiB, and
/// truncation is positional, so the one line that must survive goes at the top.
fn robots_txt(base_url: &str, sitemaps: &SitemapSet) -> String {
    /// Well under Google's 500 KiB ceiling, with room for the directives above.
    const BUDGET: usize = 400 * 1024;

    let mut robots: String = String::from("User-agent: *\nAllow: /\n\n");
    let mut omitted: usize = 0;
    for path in sitemaps.paths() {
        let line: String = format!("Sitemap: {base_url}{path}\n");
        if robots.len() + line.len() > BUDGET {
            omitted += 1;
            continue;
        }
        robots.push_str(&line);
    }

    if omitted > 0 {
        // Silent truncation is the failure mode this whole design avoids, so
        // the generator must not commit it either.
        tracing::error!(
            "omitted {omitted} sitemap line(s) from robots.txt to stay under \
             Google's 500 KiB limit; the index is still listed first"
        );
    }

    robots
}

/// The site's humans.txt: the author's own, written once and inherited by every
/// project that enables `preset`.
#[cfg(feature = "preset")]
fn humans_txt(_base: &BaseTemplateData) -> String {
    String::from(PRESET_HUMANS_TXT)
}

/// A minimal humans.txt, so the layout's `rel="author"` link never 404s on a
/// project that does not use the author's own.
#[cfg(not(feature = "preset"))]
fn humans_txt(base: &BaseTemplateData) -> String {
    format!(
        "/* TEAM */\n\nName: {}\nSite: {}\n",
        base.author(),
        base.base_url()
    )
}

/// The web app manifest.
///
/// `minimal-ui` rather than `standalone`: an installed multi-page content site
/// with no back button strands the reader. It still satisfies the installability
/// bar, so nothing is given up.
fn webmanifest(base: &BaseTemplateData) -> String {
    const ICONS: [(&str, &str); 2] = [("/icon-192.png", "192x192"), ("/icon-512.png", "512x512")];

    let icons: Vec<String> = ICONS
        .iter()
        .map(|(source, sizes)| {
            format!(
                "{{ \"src\": \"{source}\", \"sizes\": \"{sizes}\", \"type\": \"image/png\", \"purpose\": \"any maskable\" }}"
            )
        })
        .collect();

    let color: &str = base.theme_color().primary();

    format!(
        "{{\n  \"name\": {name:?},\n  \"short_name\": {name:?},\n  \"description\": {description:?},\n  \"start_url\": \"/\",\n  \"scope\": \"/\",\n  \"display\": \"minimal-ui\",\n  \"theme_color\": {color:?},\n  \"background_color\": {color:?},\n  \"icons\": [\n    {icons}\n  ]\n}}\n",
        name = base.project(),
        description = base.description(),
        icons = icons.join(",\n    "),
    )
}

#[cfg(test)]
mod tests {
    use super::{ProxyPaths, short_hash, slugify};

    #[test]
    fn a_project_name_becomes_a_lowercase_dashed_slug() {
        assert_eq!(String::from("boggledygook"), slugify("Boggledygook"));
        assert_eq!(String::from("eat-out"), slugify("Eat Out"));
        assert_eq!(
            String::from("template-web-server"),
            slugify("Template Web Server")
        );
        assert_eq!(
            String::from("palms-small-engine"),
            slugify("Palms  Small!Engine")
        );
    }

    #[test]
    fn the_proxy_paths_look_like_ordinary_bundler_output() {
        let paths: ProxyPaths = ProxyPaths::derive("Boggledygook");

        assert!(paths.analytics_script.starts_with("/script/boggledygook-"));
        assert!(
            std::path::Path::new(&paths.analytics_script)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("js"))
        );
        assert!(paths.analytics_event.starts_with("/api/v1/boggledygook-"));

        // None of Plausible's forbidden words appear anywhere.
        for path in [&paths.analytics_script, &paths.analytics_event] {
            for forbidden in ["plausible", "analytics", "tracking", "stats"] {
                assert!(!path.contains(forbidden), "`{path}` contains `{forbidden}`");
            }
        }
    }

    #[test]
    fn the_two_proxies_do_not_share_a_hash() {
        let paths: ProxyPaths = ProxyPaths::derive("Boggledygook");

        assert_ne!(paths.analytics_script, paths.sentry_script);
        assert_ne!(paths.analytics_event, paths.sentry_tunnel);
    }

    #[test]
    fn two_projects_never_share_a_path_so_one_filter_rule_cannot_block_both() {
        let first: ProxyPaths = ProxyPaths::derive("Boggledygook");
        let second: ProxyPaths = ProxyPaths::derive("Eat Out");

        assert_ne!(first.analytics_script, second.analytics_script);
    }

    #[test]
    fn the_derived_hash_is_stable_across_runs() {
        let expected: String = short_hash("Boggledygook");
        let actual: String = short_hash("Boggledygook");
        assert_eq!(expected, actual);

        let expected: usize = 8;
        let actual: usize = short_hash("Boggledygook").len();
        assert_eq!(expected, actual);
    }
}
