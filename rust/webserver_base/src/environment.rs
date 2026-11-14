//! Which deployment this process is: a laptop, or the droplet. There is no dev
//! or staging tier.
//!
//! [`Environment::Production`] is the strict one — error monitoring is required
//! there, and analytics only fire there. Required everywhere rather than
//! defaulted, because a default means a misconfigured deploy behaves like a
//! laptop.

use std::fmt::{self, Display, Formatter};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::env::{self, EnvError};

/// The environment variable [`Environment::from_env`] reads.
pub const ENV_ENVIRONMENT: &str = "WSB_ENVIRONMENT";

/// Which deployment this process is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Running on a development machine.
    Local,
    /// Running in production.
    Production,
}

impl Environment {
    /// The lowercase name, as written in `WSB_ENVIRONMENT`.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Production => "production",
        }
    }

    /// Whether this is production.
    #[must_use]
    pub const fn is_production(&self) -> bool {
        matches!(self, Self::Production)
    }

    /// Whether this is a development machine.
    #[must_use]
    pub const fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    /// Reads [`ENV_ENVIRONMENT`]. Deliberately has no default.
    ///
    /// # Errors
    ///
    /// [`EnvError::Missing`] if unset, [`EnvError::Empty`] if blank, or
    /// [`EnvError::Invalid`] if it is neither `local` nor `production`.
    pub fn from_env() -> Result<Self, EnvError> {
        let value: String = env::required(ENV_ENVIRONMENT)?;
        value.parse::<Self>().map_err(|_| EnvError::Invalid {
            key: ENV_ENVIRONMENT.to_string(),
            expected: "environment (`local` or `production`)",
            value,
        })
    }
}

/// The value was neither `local` nor `production`.
#[derive(Debug, thiserror::Error)]
#[error("`{value}` is not a supported environment; use `local` or `production`")]
pub struct EnvironmentParseError {
    /// What was supplied.
    pub value: String,
}

impl FromStr for Environment {
    type Err = EnvironmentParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "local" => Ok(Self::Local),
            "production" => Ok(Self::Production),
            _ => Err(EnvironmentParseError {
                value: s.to_string(),
            }),
        }
    }
}

impl Display for Environment {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::{Environment, EnvironmentParseError};

    #[test]
    fn both_environments_round_trip_through_their_string_form() {
        for expected in [Environment::Local, Environment::Production] {
            let actual: Environment = expected
                .as_str()
                .parse::<Environment>()
                .expect("its own as_str is always parseable");
            assert_eq!(expected, actual);
        }
    }

    #[test]
    fn parsing_is_case_and_whitespace_insensitive() {
        let expected: Environment = Environment::Production;
        let actual: Environment = "  PRODUCTION  ".parse().expect("trimmed and lowercased");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_tier_that_does_not_exist_is_rejected_by_name() {
        let error: EnvironmentParseError = "staging"
            .parse::<Environment>()
            .expect_err("there is no staging");

        let expected: String =
            String::from("`staging` is not a supported environment; use `local` or `production`");
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_predicates_agree_with_the_variant() {
        assert!(Environment::Production.is_production());
        assert!(!Environment::Production.is_local());
        assert!(Environment::Local.is_local());
        assert!(!Environment::Local.is_production());
    }

    #[test]
    fn it_serializes_lowercase_for_template_data() {
        let expected: String = String::from("\"production\"");
        let actual: String =
            serde_json::to_string(&Environment::Production).expect("plain enum serializes");
        assert_eq!(expected, actual);
    }
}
