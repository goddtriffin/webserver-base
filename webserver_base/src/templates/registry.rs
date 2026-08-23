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
    /// An empty registry with the built-in helpers and strict mode, touching no
    /// files.
    #[must_use]
    pub fn empty() -> Self {
        let mut handlebars: Handlebars<'a> = Handlebars::new();

        handlebars.register_helper("join", Box::new(join));
        handlebars.register_helper("pretty_date", Box::new(pretty_date));
        handlebars.register_helper("has_key", Box::new(has_key));

        // A missing field becomes an error rather than an empty string: a blank
        // `<title>` is a bug that ships, a failed render is one that gets fixed.
        handlebars.set_strict_mode(true);

        Self { handlebars }
    }

    /// Loads every template under `root`'s `layouts`, `pages` and `partials`.
    ///
    /// # Errors
    ///
    /// [`TemplateError::ReadDirectory`] if a directory is unreadable, or
    /// [`TemplateError::Compile`] if a template does not compile.
    #[instrument(skip_all)]
    pub fn from_dir(root: impl AsRef<Path>) -> Result<Self, TemplateError> {
        let root: &Path = root.as_ref();
        let mut registry: Self = Self::empty();

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

                registry
                    .handlebars
                    .register_template_file(name, &file)
                    .map_err(|source| TemplateError::Compile {
                        name: name.to_string(),
                        source: Box::new(source),
                    })?;
            }
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
    use serde_json::json;

    use super::TemplateRegistry;
    use crate::templates::error::TemplateError;

    #[test]
    fn an_empty_registry_has_no_templates() {
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
    fn a_missing_template_root_is_not_an_error() {
        let registry: TemplateRegistry<'_> =
            TemplateRegistry::from_dir("/tmp/wsb-nonexistent-template-root")
                .expect("absent directories are skipped");

        let expected: bool = false;
        let actual: bool = registry.has_template("home");
        assert_eq!(expected, actual);
    }
}
