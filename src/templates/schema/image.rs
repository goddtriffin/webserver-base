use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Image {
    name: String,
    alt: String,
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
