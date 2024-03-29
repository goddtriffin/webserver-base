use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SocialMedia {
    pub name: String,
    pub link: String,
}

impl SocialMedia {
    #[must_use]
    pub fn new(name: &str, link: &str) -> Self {
        Self {
            name: String::from(name),
            link: String::from(link),
        }
    }
}
