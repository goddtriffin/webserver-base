use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Image {
    pub name: String,
    pub alt: String,
}

impl Image {
    #[must_use]
    pub fn new(name: &str, alt: &str) -> Self {
        Self {
            name: String::from(name),
            alt: String::from(alt),
        }
    }
}
