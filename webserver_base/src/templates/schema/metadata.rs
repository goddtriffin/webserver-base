use serde::{Deserialize, Serialize};

use super::twitter::Twitter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    pub language_code: String,
    pub country_code: String,
    pub charset: String,
    pub description: String,
    pub project: String,
    pub author: String,
    pub twitter: Twitter,
    pub home_url: String,
    pub keywords: Vec<String>,
    pub theme_color: String,
    pub social_image: String,
}

impl Metadata {
    #[expect(clippy::too_many_arguments)]
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

    #[must_use]
    pub fn new_with_defaults(
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
