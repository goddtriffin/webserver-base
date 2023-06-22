use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Twitter {
    username: String,
}

impl Twitter {
    #[must_use]
    pub const fn new(username: String) -> Self {
        Self { username }
    }
}
