use sitemap_rs::url_error::UrlError;
use sitemap_rs::url_set_error::UrlSetError;
use std::fmt::{Debug, Formatter};
use std::{error, fmt};
use webserver_base::TemplateRegistryError;
use xml_builder::XMLError;

pub type WebserverResult<T> = Result<T, WebserverError>;

#[derive(Debug)]
pub enum WebserverError {
    TemplateRegistryError(TemplateRegistryError),
    UrlSetError(UrlSetError),
    UrlError(UrlError),
    XMLError(XMLError),
    IoError(std::io::Error),
}

impl error::Error for WebserverError {}

impl fmt::Display for WebserverError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TemplateRegistryError(template_registry_error) => {
                std::fmt::Display::fmt(&template_registry_error, f)
            }
            Self::UrlSetError(url_set_error) => std::fmt::Display::fmt(&url_set_error, f),
            Self::UrlError(url_error) => std::fmt::Display::fmt(&url_error, f),
            Self::XMLError(xml_error) => std::fmt::Display::fmt(&xml_error, f),
            Self::IoError(io_error) => std::fmt::Display::fmt(&io_error, f),
        }
    }
}

impl From<TemplateRegistryError> for WebserverError {
    fn from(template_registry_error: TemplateRegistryError) -> Self {
        Self::TemplateRegistryError(template_registry_error)
    }
}

impl From<UrlSetError> for WebserverError {
    fn from(url_set_error: UrlSetError) -> Self {
        Self::UrlSetError(url_set_error)
    }
}

impl From<UrlError> for WebserverError {
    fn from(url_error: UrlError) -> Self {
        Self::UrlError(url_error)
    }
}

impl From<XMLError> for WebserverError {
    fn from(xml_error: XMLError) -> Self {
        Self::XMLError(xml_error)
    }
}

impl From<std::io::Error> for WebserverError {
    fn from(io_error: std::io::Error) -> Self {
        Self::IoError(io_error)
    }
}
