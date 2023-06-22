use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct NavLink {
    display_name: String,
    url: String,
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
