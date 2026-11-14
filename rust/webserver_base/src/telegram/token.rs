use std::fmt::{Debug, Display, Formatter};

use super::error::TelegramError;

/// A Telegram bot token.
///
/// The token is a credential which grants full control of the bot, and it is
/// sent to Telegram as a **path segment** of every request URL. That makes it
/// unusually easy to leak: any error type, log line, or panic message which
/// includes the request URL includes the token.
///
/// This type therefore refuses to print itself. Both [`Debug`] and [`Display`]
/// render `***`, so a `{:?}` of any struct holding a `BotToken` is safe. The
/// raw value is reachable only via the crate-private [`BotToken::expose`].
#[derive(Clone, PartialEq, Eq)]
pub struct BotToken(String);

impl BotToken {
    /// Creates a new [`BotToken`], validating that it is plausibly a token.
    ///
    /// Blank-but-present is treated as missing: a server which boots with an
    /// empty token looks healthy while silently notifying nobody.
    ///
    /// # Errors
    ///
    /// Returns [`TelegramError::InvalidToken`] if the token is empty after
    /// trimming, or if it does not contain the `<bot_id>:<secret>` separator.
    pub fn new(token: impl Into<String>) -> Result<Self, TelegramError> {
        let token: String = token.into().trim().to_string();

        if token.is_empty() {
            return Err(TelegramError::InvalidToken(String::from(
                "token is empty (or only whitespace)",
            )));
        }

        // A real token looks like `123456789:AAF...`. Checking for the separator
        // catches the common misconfigurations (a chat id pasted into the token
        // slot, a quoted empty value) without hard-coding Telegram's exact format.
        let Some((bot_id, secret)) = token.split_once(':') else {
            return Err(TelegramError::InvalidToken(String::from(
                "token is missing the `<bot_id>:<secret>` separator",
            )));
        };

        if bot_id.is_empty() || !bot_id.chars().all(|c: char| c.is_ascii_digit()) {
            return Err(TelegramError::InvalidToken(String::from(
                "token's bot id is not numeric",
            )));
        }

        if secret.is_empty() {
            return Err(TelegramError::InvalidToken(String::from(
                "token's secret is empty",
            )));
        }

        Ok(Self(token))
    }

    /// Returns the raw token.
    ///
    /// Crate-private on purpose: the only legitimate use is building a request
    /// URL. Everything else should use the redacting [`Debug`]/[`Display`].
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }
}

impl Debug for BotToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "BotToken(***)")
    }
}

impl Display for BotToken {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "***")
    }
}

#[cfg(test)]
mod tests {
    use super::BotToken;
    use crate::telegram::error::TelegramError;

    const VALID: &str = "123456789:AAFNpHzr6wq4YimAMwIjqVrFU8TO5kcayEI";

    #[test]
    fn valid_token_is_accepted() {
        let token: BotToken = BotToken::new(VALID).expect("valid token should be accepted");

        let expected: &str = VALID;
        let actual: &str = token.expose();
        assert_eq!(expected, actual);
    }

    #[test]
    fn surrounding_whitespace_is_trimmed() {
        let token: BotToken =
            BotToken::new(format!("  {VALID}\n")).expect("valid token should be accepted");

        let expected: &str = VALID;
        let actual: &str = token.expose();
        assert_eq!(expected, actual);
    }

    #[test]
    fn empty_token_is_rejected() {
        let error: TelegramError = BotToken::new("").expect_err("empty token should be rejected");

        assert!(matches!(error, TelegramError::InvalidToken(_)));
    }

    #[test]
    fn whitespace_only_token_is_rejected() {
        let error: TelegramError =
            BotToken::new("   \n\t").expect_err("blank token should be rejected");

        assert!(matches!(error, TelegramError::InvalidToken(_)));
    }

    #[test]
    fn token_without_separator_is_rejected() {
        let error: TelegramError =
            BotToken::new("nosemicolonhere").expect_err("malformed token should be rejected");

        assert!(matches!(error, TelegramError::InvalidToken(_)));
    }

    #[test]
    fn token_with_non_numeric_bot_id_is_rejected() {
        let error: TelegramError =
            BotToken::new("abcdef:AAFsecret").expect_err("malformed token should be rejected");

        assert!(matches!(error, TelegramError::InvalidToken(_)));
    }

    #[test]
    fn token_with_empty_secret_is_rejected() {
        let error: TelegramError =
            BotToken::new("123456789:").expect_err("malformed token should be rejected");

        assert!(matches!(error, TelegramError::InvalidToken(_)));
    }

    #[test]
    fn debug_does_not_leak_the_token() {
        let token: BotToken = BotToken::new(VALID).expect("valid token should be accepted");

        let expected: String = String::from("BotToken(***)");
        let actual: String = format!("{token:?}");
        assert_eq!(expected, actual);
        assert!(!actual.contains("AAFNpHzr"));
    }

    #[test]
    fn display_does_not_leak_the_token() {
        let token: BotToken = BotToken::new(VALID).expect("valid token should be accepted");

        let expected: String = String::from("***");
        let actual: String = format!("{token}");
        assert_eq!(expected, actual);
        assert!(!actual.contains("AAFNpHzr"));
    }

    #[test]
    fn debug_of_a_containing_struct_does_not_leak_the_token() {
        #[derive(Debug)]
        struct Holder {
            #[expect(dead_code, reason = "only exercised through the derived Debug impl")]
            token: BotToken,
        }

        let holder: Holder = Holder {
            token: BotToken::new(VALID).expect("valid token should be accepted"),
        };

        let actual: String = format!("{holder:?}");
        assert!(!actual.contains("AAFNpHzr"));
        assert!(actual.contains("***"));
    }
}
