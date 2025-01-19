use serde::{Deserialize, Serialize};

use super::copyright::Copyright;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Footer {
    pub copyright: Copyright,
}

impl Footer {
    #[must_use]
    pub const fn new(copyright: Copyright) -> Self {
        Self { copyright }
    }
}
