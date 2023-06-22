use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Copyright {
    start: String,
    end: String,
}

impl Copyright {
    #[must_use]
    pub fn new(start: String) -> Self {
        Self {
            start,
            end: Utc::now().year().to_string(),
        }
    }
}
