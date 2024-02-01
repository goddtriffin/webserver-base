use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Twitter {
    pub username: String,
}

impl Twitter {
    #[must_use]
    pub const fn new(username: String) -> Self {
        Self { username }
    }
}
