//! Reading configuration out of the environment.
//!
//! A variable that is present but blank counts as missing: a deploy whose
//! secret failed to mount sets an empty string far more often than it unsets
//! the variable.

use std::env::{self, VarError};
use std::str::FromStr;

/// The prefix on every environment variable this crate reads.
pub const PREFIX: &str = "WSB_";

/// Why an environment variable could not be used.
#[derive(Debug, thiserror::Error)]
pub enum EnvError {
    /// The variable is not set at all.
    #[error("environment variable `{key}` is not set")]
    Missing {
        /// The variable's name.
        key: String,
    },

    /// The variable is set, but its value is blank once trimmed.
    #[error("environment variable `{key}` is set but empty")]
    Empty {
        /// The variable's name.
        key: String,
    },

    /// The variable is set to something that is not valid for its type.
    #[error("environment variable `{key}` is not a valid {expected}: `{value}`")]
    Invalid {
        /// The variable's name.
        key: String,
        /// What the value should have been, for the error message.
        expected: &'static str,
        /// What it actually was.
        value: String,
    },

    /// The variable's value is not valid Unicode.
    #[error("environment variable `{key}` is not valid unicode")]
    NotUnicode {
        /// The variable's name.
        key: String,
    },
}

/// Reads a variable which must be present and non-blank.
///
/// # Errors
///
/// [`EnvError::Missing`] if unset, [`EnvError::Empty`] if blank once trimmed,
/// [`EnvError::NotUnicode`] if the value is not valid Unicode.
pub fn required(key: &str) -> Result<String, EnvError> {
    interpret(key, env::var(key))
}

/// [`required`]'s logic over an already-read value, so it can be tested without
/// mutating the process environment (which is `unsafe`, and forbidden here).
fn interpret(key: &str, raw: Result<String, VarError>) -> Result<String, EnvError> {
    match raw {
        Ok(value) => {
            let value: String = value.trim().to_string();
            if value.is_empty() {
                return Err(EnvError::Empty {
                    key: key.to_string(),
                });
            }
            Ok(value)
        }
        Err(VarError::NotPresent) => Err(EnvError::Missing {
            key: key.to_string(),
        }),
        Err(VarError::NotUnicode(_)) => Err(EnvError::NotUnicode {
            key: key.to_string(),
        }),
    }
}

/// Reads a variable which may be absent.
///
/// A blank value is reported as absent, for the reason given in the module
/// docs.
#[must_use]
pub fn optional(key: &str) -> Option<String> {
    interpret(key, env::var(key)).ok()
}

/// Reads and parses a variable which must be present and non-blank.
///
/// # Errors
///
/// As [`required`], plus [`EnvError::Invalid`] if the value does not parse.
pub fn parse_required<T>(key: &str, expected: &'static str) -> Result<T, EnvError>
where
    T: FromStr,
{
    parse_value(key, expected, required(key)?)
}

/// Parses an already-read value, naming it in any failure.
fn parse_value<T>(key: &str, expected: &'static str, value: String) -> Result<T, EnvError>
where
    T: FromStr,
{
    value.parse::<T>().map_err(|_| EnvError::Invalid {
        key: key.to_string(),
        expected,
        value,
    })
}

/// Reads and parses a variable, falling back to `default` when absent.
///
/// # Errors
///
/// [`EnvError::Invalid`] if present but malformed — substituting the default
/// for a typo is how a server ends up listening on the wrong port.
pub fn parse_or<T>(key: &str, expected: &'static str, default: T) -> Result<T, EnvError>
where
    T: FromStr,
{
    match optional(key) {
        None => Ok(default),
        Some(value) => parse_value(key, expected, value),
    }
}

#[cfg(test)]
mod tests {
    use std::env::VarError;

    use super::{EnvError, interpret, optional, parse_or, parse_value, required};

    #[test]
    fn a_present_value_is_trimmed() {
        let expected: String = String::from("hello");
        let actual: String = interpret("WSB_X", Ok(String::from("  hello  "))).expect("present");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_missing_variable_names_itself() {
        let error: EnvError =
            interpret("WSB_X", Err(VarError::NotPresent)).expect_err("not present");
        assert!(matches!(error, EnvError::Missing { ref key } if key == "WSB_X"));

        let expected: String = String::from("environment variable `WSB_X` is not set");
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_blank_variable_is_reported_distinctly_from_a_missing_one() {
        let error: EnvError = interpret("WSB_X", Ok(String::from("   "))).expect_err("blank");
        assert!(matches!(error, EnvError::Empty { ref key } if key == "WSB_X"));

        let expected: String = String::from("environment variable `WSB_X` is set but empty");
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_non_unicode_value_is_its_own_failure() {
        let error: EnvError = interpret(
            "WSB_X",
            Err(VarError::NotUnicode(std::ffi::OsString::new())),
        )
        .expect_err("not unicode");
        assert!(matches!(error, EnvError::NotUnicode { ref key } if key == "WSB_X"));
    }

    #[test]
    fn parsing_reports_the_offending_value() {
        let error: EnvError =
            parse_value::<u16>("WSB_PORT", "port number", String::from("not-a-number"))
                .expect_err("not a u16");

        let expected: String = String::from(
            "environment variable `WSB_PORT` is not a valid port number: `not-a-number`",
        );
        let actual: String = error.to_string();
        assert_eq!(expected, actual);
    }

    #[test]
    fn parsing_succeeds_for_a_well_formed_value() {
        let expected: u16 = 8080;
        let actual: u16 =
            parse_value("WSB_PORT", "port number", String::from("8080")).expect("valid");
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_absent_variable_falls_back_to_the_default() {
        let expected: u16 = 8080;
        let actual: u16 = parse_or("WSB_DEFINITELY_NOT_SET_ANYWHERE", "port number", 8080)
            .expect("absent is fine");
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_public_wrappers_agree_with_the_logic_they_wrap() {
        let error: EnvError = required("WSB_DEFINITELY_NOT_SET_ANYWHERE").expect_err("absent");
        assert!(matches!(error, EnvError::Missing { .. }));

        let expected: Option<String> = None;
        let actual: Option<String> = optional("WSB_DEFINITELY_NOT_SET_ANYWHERE");
        assert_eq!(expected, actual);
    }
}
