//! Failures from loading or rendering templates.

use std::path::PathBuf;

use handlebars::{RenderError, TemplateError as HandlebarsTemplateError};

/// Why a template could not be loaded or rendered.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// A template directory could not be read.
    #[error("failed to read template directory `{path}`")]
    ReadDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// A project supplied a template whose name this crate owns.
    #[error(
        "`{path}` shadows the built-in `{name}` layout, which webserver-base \
         embeds and every page extends. Rename it — a project may add any other \
         layout, just not this one."
    )]
    ReservedName { name: &'static str, path: PathBuf },

    /// A project registered a helper whose name is already taken.
    #[error(
        "the helper `{name}` is already defined by webserver-base or Handlebars. \
         Rename the project's helper; if several projects need the same one, \
         it belongs in the library."
    )]
    ReservedHelper { name: String },

    /// A frontend was declared with no page templates to serve.
    #[error(
        "`{path}` contains no `.hbs` pages. A frontend must serve at least one \
         page; add `html/pages/home.hbs`."
    )]
    NoPages { path: PathBuf },

    /// A frontend has no 404 template.
    #[error(
        "`{path}` is missing. Every frontend serves a 404 with a real 404 \
         status, so the library declares and routes it — but the page itself \
         is yours to design."
    )]
    MissingNotFoundPage { path: PathBuf },

    /// A template file failed to compile.
    #[error("failed to compile template `{name}`")]
    Compile {
        name: String,
        #[source]
        source: Box<HandlebarsTemplateError>,
    },

    /// A template failed to render — usually strict mode catching a field the
    /// data did not supply.
    #[error("failed to render template `{name}`")]
    Render {
        name: String,
        #[source]
        source: Box<RenderError>,
    },

    /// A page's JSON-LD document could not be serialized.
    #[error("failed to serialize the JSON-LD document for page `{page}`")]
    JsonLd {
        page: String,
        #[source]
        source: serde_json::Error,
    },

    /// The data handed to a render could not be serialized.
    #[error("failed to serialize template data for page `{page}`")]
    Serialize {
        page: String,
        #[source]
        source: serde_json::Error,
    },
}
