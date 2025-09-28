use chrono::{DateTime, Utc};
use handlebars::{Handlebars, handlebars_helper};
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::{env, fs};
use tracing::instrument;

use super::error::TemplateRegistryError;

// comma-delimits a list of strings
handlebars_helper!(
    join: |list: Vec<String>| list.join(",")
);

// prints a UTC date like "January 15, 1990"
handlebars_helper!(
    pretty_date: |date: DateTime<Utc>| date.format("%B %e, %Y").to_string()
);

// returns true if the object has the key
handlebars_helper!(has_key: |obj: Value, key: str| {
    obj.as_object()
        .is_some_and(|o| o.contains_key(key))
});

#[derive(Clone)]
pub struct TemplateRegistry<'a> {
    handlebars: Handlebars<'a>,
}

impl Default for TemplateRegistry<'_> {
    fn default() -> Self {
        let mut html_path: PathBuf = env::current_dir().expect("failed to get current directory");
        html_path.push("html");

        let template_files: Vec<PathBuf> =
            TemplateRegistry::find_all_template_files(&mut html_path)
                .expect("failed to retrieve template files");

        Self::new(template_files).expect("failed to create TemplateRegistry")
    }
}

impl TemplateRegistry<'_> {
    /// # Errors
    ///
    /// Will return `Error` if it encounters any `FileIO` or Handlebars Template errors.
    ///
    /// # Panics
    ///
    /// Panics if it encounters any `FileIO` errors.
    #[instrument(skip_all)]
    pub fn new(template_files: Vec<PathBuf>) -> Result<Self, TemplateRegistryError> {
        // initialize Handlebars
        let mut handlebars = Handlebars::new();

        // register all helpers
        handlebars.register_helper("join", Box::new(join));
        handlebars.register_helper("pretty_date", Box::new(pretty_date));
        handlebars.register_helper("has_key", Box::new(has_key));

        // enforce strict templates
        handlebars.set_strict_mode(true);

        // find all HTML files at this hard-coded path
        for template_file in template_files {
            let file_name = template_file
                .file_stem()
                .expect("failed to extract stem (non-file-extension) part of template file name")
                .to_string_lossy();
            handlebars.register_template_file(file_name.as_ref(), &template_file)?;
        }

        Ok(Self { handlebars })
    }

    #[instrument(skip_all)]
    fn find_all_template_files(
        html_path: &mut PathBuf,
    ) -> Result<Vec<PathBuf>, TemplateRegistryError> {
        let mut template_files: Vec<PathBuf> = vec![];

        for directory in &["layouts", "pages", "partials"] {
            html_path.push(directory);
            template_files.extend(TemplateRegistry::get_all_files(html_path)?);
            html_path.pop();
        }

        Ok(template_files)
    }

    #[instrument(skip_all)]
    fn get_all_files(current_dir: &PathBuf) -> Result<Vec<PathBuf>, TemplateRegistryError> {
        let mut files: Vec<PathBuf> = vec![];

        for entry in fs::read_dir(current_dir)? {
            let entry = entry?;
            let path = entry.path();
            files.push(path);
        }

        Ok(files)
    }

    /// # Errors
    ///
    /// Will return `Error` if the template cannot be rendered.
    #[instrument(skip_all)]
    pub fn render<T>(&self, name: &str, data: &T) -> Result<String, TemplateRegistryError>
    where
        T: Serialize,
    {
        let rendered_template: String = self.handlebars.render(name, data)?;
        Ok(rendered_template)
    }
}
