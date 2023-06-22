use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Page {
    display_name: String,
    url: String,
    style_sheets: Vec<String>,
    scripts: Vec<String>,
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
