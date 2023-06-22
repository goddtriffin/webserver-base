use crate::templates::Copyright;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Footer {
    copyright: Copyright,
}

impl Footer {
    #[must_use]
    pub const fn new(copyright: Copyright) -> Self {
        Self { copyright }
    }
}
