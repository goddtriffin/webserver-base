use std::fmt::{Display, Formatter};

#[derive(PartialEq, Eq, Clone)]
pub enum Environment {
    Development,
    Production,
}

impl Default for Environment {
    fn default() -> Self {
        Self::Development
    }
}

impl Environment {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Development => "development",
            Self::Production => "production",
        }
    }
}

impl TryFrom<String> for Environment {
    type Error = String;

    fn try_from(s: String) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            "development" => Ok(Self::Development),
            "production" => Ok(Self::Production),
            other => Err(format!(
                "{other} is not a supported environment. Use either `development` or `production`."
            )),
        }
    }
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Development => write!(f, "development"),
            Self::Production => write!(f, "production"),
        }
    }
}
