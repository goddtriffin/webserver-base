use crate::templates::Copyright;
use serde::{Deserialize, Serialize};

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
