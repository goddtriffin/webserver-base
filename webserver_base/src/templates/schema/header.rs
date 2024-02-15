use crate::templates::NavLink;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Header {
    pub nav_links: Vec<NavLink>,
}

impl Header {
    #[must_use]
    pub fn new(nav_links: Vec<NavLink>) -> Self {
        Self { nav_links }
    }
}
