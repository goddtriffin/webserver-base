use std::{collections::BTreeMap, fmt::Display};

use serde::{Deserialize, Serialize};
use webserver_base::{
    base_settings::BaseSettings,
    cache_buster::CacheBuster,
    templates::schema::{
        copyright::Copyright, footer::Footer, metadata::Metadata, page::Page,
        social_media::SocialMedia, twitter::Twitter,
    },
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateData {
    metadata: Metadata,
    footer: Footer,
    pub page: Option<Page>,

    social_media: Vec<SocialMedia>,

    cache_buster: BTreeMap<String, String>,
}

impl TemplateData {
    #[must_use]
    pub fn new(settings: BaseSettings, cache_buster: &CacheBuster) -> Self {
        let social_media: Vec<SocialMedia> = vec![
            SocialMedia::new("Twitter", "https://twitter.com/goddtriffin"),
            SocialMedia::new("Instagram", "https://www.instagram.com/goddtriffin/"),
            SocialMedia::new("Facebook", "https://www.facebook.com/goddtriffin/"),
            SocialMedia::new(
                "YouTube",
                "https://www.youtube.com/channel/UC1YNQd3SqLkl2CjwbvJBYoQ",
            ),
            SocialMedia::new("Github", "https://github.com/goddtriffin"),
            SocialMedia::new(
                "Stack Overflow",
                "https://stackoverflow.com/users/11767294/goddtriffin",
            ),
            SocialMedia::new("Reddit", "https://www.reddit.com/user/goddtriffin"),
        ];

        let keywords: Vec<String> = settings
            .project_keywords
            .split(',')
            .map(String::from)
            .collect::<Vec<String>>();

        Self {
            metadata: Metadata::new(
                String::from("en"),
                String::from("US"),
                String::from("utf-8"),
                settings.project_description.clone(),
                settings.project_name.clone(),
                String::from("Todd Everett Griffin"),
                Twitter::new(String::from("@goddtriffin")),
                settings.home_url,
                keywords,
                String::from("#f7cb64"),
                format!(
                    "/{}",
                    cache_buster.get_file("static/image/social/todo.webp")
                ),
            ),
            footer: Footer::new(Copyright::new(String::from("1998"))),
            page: None,
            social_media,
            cache_buster: cache_buster.get_cache(),
        }
    }

    #[must_use]
    pub fn render(mut self, page: Page) -> Self {
        self.page = Some(page);
        self
    }
}

impl Display for TemplateData {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", serde_json::to_string_pretty(self).unwrap())
    }
}
