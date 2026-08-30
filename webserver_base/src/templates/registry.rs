//! The Handlebars registry.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use handlebars::{Handlebars, HelperDef, handlebars_helper};
use serde::Serialize;
use serde_json::Value;
use tracing::instrument;

use super::error::TemplateError;

/// The subdirectories scanned, in order. A template's name is its file stem, so
/// `pages/home.hbs` is `{{> home}}`; stems must be unique across all three.
pub const TEMPLATE_DIRECTORIES: [&str; 3] = ["layouts", "pages", "partials"];

/// Where a project's templates live. Not configurable: every project uses this
/// layout, and a knob that can only correctly hold one value is not a knob.
pub const TEMPLATE_ROOT: &str = "html";

/// The name of the embedded layout every page extends.
pub const BASE_TEMPLATE_NAME: &str = "base";

/// The 404 template every frontend must provide.
///
/// Its data is not a per-project decision — every site wants the same page name,
/// the same `/404` URL and the same `noindex, follow` — so the library declares
/// it. What it *looks* like is still entirely the project's.
pub const NOT_FOUND_TEMPLATE_NAME: &str = "404";

/// The layout itself, compiled into the crate.
///
/// Shipping it here rather than copying it into each project is the whole
/// point: the `<head>` is pure function — spec conformance, Open Graph, JSON-LD
/// — and solving it once means no project can drift into a stale or subtly
/// wrong version of it.
const BASE_TEMPLATE: &str = include_str!("../../assets/html/layouts/base.hbs");

// Comma-joins a list of strings, for `<meta name="keywords">`.
handlebars_helper!(join: |list: Vec<String>| list.join(","));

// Renders a UTC date as "January 15, 1990".
handlebars_helper!(pretty_date: |date: DateTime<Utc>| date.format("%B %e, %Y").to_string());

// True when an object has the given key.
handlebars_helper!(has_key: |object: Value, key: str| {
    object.as_object().is_some_and(|object| object.contains_key(key))
});

/// Every Handlebars template the server can render.
#[derive(Clone)]
pub struct TemplateRegistry<'a> {
    handlebars: Handlebars<'a>,
}

impl<'a> TemplateRegistry<'a> {
    /// A registry holding only the embedded `base` layout, plus the built-in
    /// helpers and strict mode. Touches no files.
    ///
    /// # Panics
    ///
    /// Never in practice: the only template registered is compiled into the
    /// binary, so a failure here means this crate shipped a layout that does
    /// not parse, which its own tests would have caught.
    #[must_use]
    pub fn empty() -> Self {
        let mut handlebars: Handlebars<'a> = Handlebars::new();

        handlebars.register_helper("join", Box::new(join));
        handlebars.register_helper("pretty_date", Box::new(pretty_date));
        handlebars.register_helper("has_key", Box::new(has_key));

        // A missing field becomes an error rather than an empty string: a blank
        // `<title>` is a bug that ships, a failed render is one that gets fixed.
        handlebars.set_strict_mode(true);

        handlebars
            .register_template_string(BASE_TEMPLATE_NAME, BASE_TEMPLATE)
            .expect("the embedded base layout compiles");

        Self { handlebars }
    }

    /// Loads every template under `root`'s `layouts`, `pages` and `partials`,
    /// on top of the embedded `base` layout.
    ///
    /// # Errors
    ///
    /// [`TemplateError::ReadDirectory`] if `root` or a subdirectory is
    /// unreadable, [`TemplateError::ReservedName`] if a file would shadow the
    /// embedded layout, [`TemplateError::NoPages`] if no page templates exist,
    /// or [`TemplateError::Compile`] if a template does not compile.
    #[instrument(skip_all)]
    pub fn from_dir(root: impl AsRef<Path>) -> Result<Self, TemplateError> {
        let root: &Path = root.as_ref();
        let mut registry: Self = Self::empty();
        let mut pages: usize = 0;

        if !root.is_dir() {
            return Err(TemplateError::ReadDirectory {
                path: root.to_path_buf(),
                source: std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "a frontend needs a template directory",
                ),
            });
        }

        for directory in TEMPLATE_DIRECTORIES {
            let path: PathBuf = root.join(directory);
            if !path.is_dir() {
                // A site with no partials is a site, not a misconfiguration.
                continue;
            }

            let entries = fs::read_dir(&path).map_err(|source| TemplateError::ReadDirectory {
                path: path.clone(),
                source,
            })?;

            for entry in entries {
                let entry = entry.map_err(|source| TemplateError::ReadDirectory {
                    path: path.clone(),
                    source,
                })?;
                let file: PathBuf = entry.path();
                if file.is_dir() {
                    continue;
                }
                let Some(name) = file.file_stem().and_then(|stem| stem.to_str()) else {
                    continue;
                };

                // Silently letting a project's file win would disable the
                // embedded `<head>` and nobody would notice until the SEO tags
                // went missing in production.
                if name == BASE_TEMPLATE_NAME {
                    return Err(TemplateError::ReservedName {
                        name: BASE_TEMPLATE_NAME,
                        path: file,
                    });
                }

                if directory == "pages" {
                    pages += 1;
                }

                registry
                    .handlebars
                    .register_template_file(name, &file)
                    .map_err(|source| TemplateError::Compile {
                        name: name.to_string(),
                        source: Box::new(source),
                    })?;
            }
        }

        if pages == 0 {
            return Err(TemplateError::NoPages {
                path: root.join("pages"),
            });
        }

        if !registry.has_template(NOT_FOUND_TEMPLATE_NAME) {
            return Err(TemplateError::MissingNotFoundPage {
                path: root.join("pages").join("404.hbs"),
            });
        }

        Ok(registry)
    }

    /// Registers an extra helper, for a project that needs one.
    pub fn register_helper(&mut self, name: &str, helper: Box<dyn HelperDef + Send + Sync + 'a>) {
        self.handlebars.register_helper(name, helper);
    }

    /// Whether a template with this name is registered.
    #[must_use]
    pub fn has_template(&self, name: &str) -> bool {
        self.handlebars.has_template(name)
    }

    /// Renders `name` against `data`.
    ///
    /// # Errors
    ///
    /// [`TemplateError::Render`], most often because strict mode caught a field
    /// the template asked for and the data did not supply.
    #[instrument(skip_all)]
    pub fn render<T>(&self, name: &str, data: &T) -> Result<String, TemplateError>
    where
        T: Serialize,
    {
        self.handlebars
            .render(name, data)
            .map_err(|source| TemplateError::Render {
                name: name.to_string(),
                source: Box::new(source),
            })
    }
}

impl std::fmt::Debug for TemplateRegistry<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&String> = self.handlebars.get_templates().keys().collect();
        names.sort();
        f.debug_struct("TemplateRegistry")
            .field("templates", &names)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use serde_json::json;

    use super::TemplateRegistry;
    use crate::templates::error::TemplateError;

    #[test]
    fn a_registry_holds_the_embedded_layout_before_it_touches_any_file() {
        let registry: TemplateRegistry<'_> = TemplateRegistry::empty();

        let expected: bool = true;
        let actual: bool = registry.has_template(super::BASE_TEMPLATE_NAME);
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_registry_holding_only_the_layout_has_no_pages() {
        let registry: TemplateRegistry<'_> = TemplateRegistry::empty();

        let expected: bool = false;
        let actual: bool = registry.has_template("home");
        assert_eq!(expected, actual);
    }

    #[test]
    fn strict_mode_turns_a_missing_field_into_an_error() {
        let mut registry: TemplateRegistry<'_> = TemplateRegistry::empty();
        registry
            .handlebars
            .register_template_string("greet", "hello {{name}}")
            .expect("valid template");

        let error: TemplateError = registry
            .render("greet", &json!({ "other": "field" }))
            .expect_err("strict mode rejects a missing field");
        assert!(matches!(error, TemplateError::Render { ref name, .. } if name == "greet"));
    }

    #[test]
    fn the_join_helper_comma_delimits_for_the_keywords_tag() {
        let mut registry: TemplateRegistry<'_> = TemplateRegistry::empty();
        registry
            .handlebars
            .register_template_string("keywords", "{{join words}}")
            .expect("valid template");

        let expected: String = String::from("boggle,word-game,scrabble");
        let actual: String = registry
            .render(
                "keywords",
                &json!({ "words": ["boggle", "word-game", "scrabble"] }),
            )
            .expect("renders");
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_has_key_helper_distinguishes_present_from_absent() {
        let mut registry: TemplateRegistry<'_> = TemplateRegistry::empty();
        registry
            .handlebars
            .register_template_string("check", "{{#if (has_key map 'a')}}yes{{else}}no{{/if}}")
            .expect("valid template");

        let expected_present: String = String::from("yes");
        let actual_present: String = registry
            .render("check", &json!({ "map": { "a": 1 } }))
            .expect("renders");
        assert_eq!(expected_present, actual_present);

        let expected_absent: String = String::from("no");
        let actual_absent: String = registry
            .render("check", &json!({ "map": { "b": 1 } }))
            .expect("renders");
        assert_eq!(expected_absent, actual_absent);
    }

    #[test]
    fn a_frontend_without_a_template_root_cannot_start() {
        let error: TemplateError = TemplateRegistry::from_dir("/tmp/wsb-nonexistent-template-root")
            .expect_err("a frontend needs templates");
        assert!(matches!(error, TemplateError::ReadDirectory { .. }));
    }

    #[test]
    fn a_project_layout_named_base_is_refused_rather_than_silently_winning() {
        let root: PathBuf = PathBuf::from("/tmp/wsb-reserved-name/html");
        fs::create_dir_all(root.join("layouts")).expect("temp dirs");
        fs::create_dir_all(root.join("pages")).expect("temp dirs");
        fs::write(root.join("pages/home.hbs"), "hi").expect("temp file");
        fs::write(root.join("pages/404.hbs"), "nope").expect("temp file");
        fs::write(root.join("layouts/base.hbs"), "<html></html>").expect("temp file");

        let error: TemplateError =
            TemplateRegistry::from_dir(&root).expect_err("`base` is reserved");
        assert!(matches!(error, TemplateError::ReservedName { name, .. } if name == "base"));

        fs::remove_dir_all("/tmp/wsb-reserved-name").ok();
    }

    #[test]
    fn a_project_may_add_any_other_layout() {
        let root: PathBuf = PathBuf::from("/tmp/wsb-other-layout/html");
        fs::create_dir_all(root.join("layouts")).expect("temp dirs");
        fs::create_dir_all(root.join("pages")).expect("temp dirs");
        fs::write(root.join("pages/home.hbs"), "hi").expect("temp file");
        fs::write(root.join("pages/404.hbs"), "nope").expect("temp file");
        fs::write(root.join("layouts/chapter.hbs"), "shell").expect("temp file");

        let registry: TemplateRegistry<'_> =
            TemplateRegistry::from_dir(&root).expect("a non-reserved layout is fine");

        let expected: bool = true;
        let actual: bool = registry.has_template("chapter");
        assert_eq!(expected, actual);

        fs::remove_dir_all("/tmp/wsb-other-layout").ok();
    }

    #[test]
    fn a_frontend_with_no_pages_cannot_start() {
        let root: PathBuf = PathBuf::from("/tmp/wsb-no-pages/html");
        fs::create_dir_all(root.join("partials")).expect("temp dirs");
        fs::write(root.join("partials/footer.hbs"), "<footer></footer>").expect("temp file");

        let error: TemplateError =
            TemplateRegistry::from_dir(&root).expect_err("a frontend must serve a page");
        assert!(matches!(error, TemplateError::NoPages { .. }));

        fs::remove_dir_all("/tmp/wsb-no-pages").ok();
    }

    #[test]
    fn a_frontend_without_a_404_template_cannot_start() {
        let root: PathBuf = PathBuf::from("/tmp/wsb-no-404/html");
        fs::create_dir_all(root.join("pages")).expect("temp dirs");
        fs::write(root.join("pages/home.hbs"), "hi").expect("temp file");

        let error: TemplateError =
            TemplateRegistry::from_dir(&root).expect_err("every frontend serves a 404");
        assert!(matches!(error, TemplateError::MissingNotFoundPage { .. }));

        fs::remove_dir_all("/tmp/wsb-no-404").ok();
    }
}
