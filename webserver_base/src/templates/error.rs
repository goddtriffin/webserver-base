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
