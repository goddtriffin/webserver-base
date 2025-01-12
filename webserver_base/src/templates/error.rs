use handlebars::{RenderError, TemplateError};
use std::fmt::{Debug, Formatter};
use std::{error, fmt, io};

#[derive(Debug)]
pub enum TemplateRegistryError {
    FileIOError(io::Error),
    TemplateError(TemplateError),
    RenderError(RenderError),
}

impl error::Error for TemplateRegistryError {}

impl fmt::Display for TemplateRegistryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileIOError(io_error) => std::fmt::Display::fmt(&io_error, f),
            Self::TemplateError(template_error) => std::fmt::Display::fmt(&template_error, f),
            Self::RenderError(render_error) => std::fmt::Display::fmt(&render_error, f),
        }
    }
}

impl From<io::Error> for TemplateRegistryError {
    fn from(io_error: io::Error) -> Self {
        Self::FileIOError(io_error)
    }
}

impl From<TemplateError> for TemplateRegistryError {
    fn from(template_error: TemplateError) -> Self {
        Self::TemplateError(template_error)
    }
}

impl From<RenderError> for TemplateRegistryError {
    fn from(render_error: RenderError) -> Self {
        Self::RenderError(render_error)
    }
}
