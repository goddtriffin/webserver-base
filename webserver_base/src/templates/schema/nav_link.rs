use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NavLink {
    pub display_name: String,
    pub url: String,
}

impl NavLink {
    #[must_use]
    pub fn new(display_name: &str, url: &str) -> Self {
        Self {
            display_name: String::from(display_name),
            url: String::from(url),
        }
    }
}
