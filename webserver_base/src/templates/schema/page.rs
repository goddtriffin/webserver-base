use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub display_name: String,
    pub url: String,
    pub style_sheets: Vec<String>,
    pub scripts: Vec<String>,
}

impl Page {
    #[must_use]
    pub fn new(
        display_name: String,
        url: String,
        style_sheets: Vec<String>,
        scripts: Vec<String>,
    ) -> Self {
        Self {
            display_name,
            url,
            style_sheets,
            scripts,
        }
    }
}
