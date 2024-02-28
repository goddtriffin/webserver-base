use serde::{Deserialize, Serialize};
use webserver_base::{
    BaseSettings, Copyright, Footer, Header, Metadata, Page, SocialMedia, Twitter,
};

#[derive(Clone, Serialize, Deserialize)]
pub struct TemplateData {
    metadata: Metadata,
    pub header: Header,
    footer: Footer,
    pub page: Option<Page>,

    social_media: Vec<SocialMedia>,

    analytics_domain: String,
    uptime_domain: String,
}

impl TemplateData {
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn new(settings: BaseSettings) -> Self {
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
                String::from("/static/image/social/todo.webp"),
            ),
            header: Header::new(vec![]),
            footer: Footer::new(Copyright::new(String::from("1998"))),
            page: None,
            social_media,
            analytics_domain: settings.analytics_domain,
            uptime_domain: settings.uptime_domain,
        }
    }

    #[must_use]
    pub fn render(mut self, page: Page) -> Self {
        self.page = Some(page);
        self
    }
}
