use chrono::{Datelike, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Copyright {
    pub start: String,
    pub end: String,
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
