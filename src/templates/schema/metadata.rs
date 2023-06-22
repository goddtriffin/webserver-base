use crate::templates::Twitter;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Metadata {
    language_code: String,
    country_code: String,
    charset: String,
    description: String,
    project: String,
    author: String,
    twitter: Twitter,
    home_url: String,
    keywords: Vec<String>,
    theme_color: String,
    social_image: String,
}

impl Metadata {
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        language_code: String,
        country_code: String,
        charset: String,
        description: String,
        project: String,
        author: String,
        twitter: Twitter,
        home_url: String,
        keywords: Vec<String>,
        theme_color: String,
        social_image: String,
    ) -> Self {
        Self {
            language_code,
            country_code,
            charset,
            description,
            project,
            author,
            twitter,
            home_url,
            keywords,
            theme_color,
            social_image,
        }
    }
}
