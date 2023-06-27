use crate::templates::Twitter;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

    #[allow(dead_code)]
    #[must_use]
    fn new_with_defaults(
        project_description: &str,
        project_name: &str,
        home_url: &str,
        keywords: &str,
        theme_color: &str,
        social_image: &str,
    ) -> Self {
        let keywords: Vec<String> = keywords
            .split(',')
            .map(String::from)
            .collect::<Vec<String>>();

        Self::new(
            String::from("en"),
            String::from("US"),
            String::from("utf-8"),
            String::from(project_description),
            String::from(project_name),
            String::from("Todd Everett Griffin"),
            Twitter::new(String::from("@goddtriffin")),
            String::from(home_url),
            keywords,
            String::from(theme_color),
            String::from(social_image),
        )
    }
}
