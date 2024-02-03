use crate::TemplateRegistryError;
use chrono::{DateTime, Utc};
use handlebars::{handlebars_helper, Handlebars};
use serde::Serialize;
use std::path::PathBuf;
use std::{env, fs};
use tracing::instrument;

// comma-delimits a list of strings
handlebars_helper!(
    join: |list: Vec<String>| list.join(",")
);

// prints a UTC date like "January 15, 1990"
handlebars_helper!(
    pretty_date: |date: DateTime<Utc>| date.format("%B %e, %Y").to_string()
);

#[derive(Clone)]
pub struct TemplateRegistry<'a> {
    handlebars: Handlebars<'a>,
}

impl TemplateRegistry<'_> {
    /// # Errors
    ///
    /// Will return `Error` if it encounters any `FileIO` or Handlebars Template errors.
    #[instrument(skip_all)]
    pub fn new() -> Result<Self, TemplateRegistryError> {
        // initialize Handlebars
        let mut handlebars = Handlebars::new();

        // register all helpers
        handlebars.register_helper("join", Box::new(join));
        handlebars.register_helper("pretty_date", Box::new(pretty_date));

        // enforce strict templates
        handlebars.set_strict_mode(true);

        // find all HTML files at this hard-coded path
        let mut html_path: PathBuf = env::current_dir()?;
        html_path.push("html");
        let template_files: Vec<PathBuf> =
            TemplateRegistry::find_all_template_files(&mut html_path)?;
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

    #[instrument(skip_all)]
    pub fn render<T>(&self, name: &str, data: &T) -> Result<String, TemplateRegistryError>
    where
        T: Serialize,
    {
        let rendered_template: String = self.handlebars.render(name, data)?;
        Ok(rendered_template)
    }
}
