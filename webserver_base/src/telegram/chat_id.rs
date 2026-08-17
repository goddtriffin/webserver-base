use std::fmt::{Display, Formatter};

use serde::{Serialize, Serializer};

/// Identifies the chat a message is sent to.
///
/// Telegram accepts either a numeric id or an `@channelusername`, and this type
/// serializes both as a JSON string, which Telegram accepts for either form.
///
/// This is also the key the rate limiter paces against, since Telegram's
/// tightest documented limit — roughly one message per second — is per chat.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ChatId {
    /// A numeric chat id. Negative for groups, supergroups, and channels.
    Id(i64),
    /// An `@channelusername`, stored with its leading `@`.
    Username(String),
}

impl ChatId {
    /// Creates a [`ChatId`] from a numeric id.
    #[must_use]
    pub const fn id(id: i64) -> Self {
        Self::Id(id)
    }

    /// Creates a [`ChatId`] from a channel username, adding the leading `@` if absent.
    #[must_use]
    pub fn username(username: impl AsRef<str>) -> Self {
        let username: &str = username.as_ref();

        if username.starts_with('@') {
            Self::Username(username.to_string())
        } else {
            Self::Username(format!("@{username}"))
        }
    }
}

impl Display for ChatId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(id) => write!(f, "{id}"),
            Self::Username(username) => write!(f, "{username}"),
        }
    }
}

impl Serialize for ChatId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl From<i64> for ChatId {
    fn from(id: i64) -> Self {
        Self::Id(id)
    }
}

impl From<&str> for ChatId {
    /// Parses a numeric id if possible, otherwise treats the value as a username.
    fn from(value: &str) -> Self {
        let value: &str = value.trim();

        value
            .parse::<i64>()
            .map_or_else(|_| Self::username(value), Self::Id)
    }
}

impl From<String> for ChatId {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::ChatId;

    #[test]
    fn numeric_id_round_trips() {
        let expected: ChatId = ChatId::Id(7_518_714_136);
        let actual: ChatId = ChatId::from(7_518_714_136_i64);
        assert_eq!(expected, actual);
    }

    #[test]
    fn negative_group_id_is_preserved() {
        let expected: ChatId = ChatId::Id(-1_001_234_567_890);
        let actual: ChatId = ChatId::from(-1_001_234_567_890_i64);
        assert_eq!(expected, actual);
    }

    #[test]
    fn username_gains_a_leading_at_sign() {
        let expected: ChatId = ChatId::Username(String::from("@mychannel"));
        let actual: ChatId = ChatId::username("mychannel");
        assert_eq!(expected, actual);
    }

    #[test]
    fn username_keeps_an_existing_at_sign() {
        let expected: ChatId = ChatId::Username(String::from("@mychannel"));
        let actual: ChatId = ChatId::username("@mychannel");
        assert_eq!(expected, actual);
    }

    #[test]
    fn numeric_string_parses_as_an_id() {
        let expected: ChatId = ChatId::Id(7_518_714_136);
        let actual: ChatId = ChatId::from("7518714136");
        assert_eq!(expected, actual);
    }

    #[test]
    fn non_numeric_string_parses_as_a_username() {
        let expected: ChatId = ChatId::Username(String::from("@mychannel"));
        let actual: ChatId = ChatId::from("@mychannel");
        assert_eq!(expected, actual);
    }

    #[test]
    fn numeric_id_serializes_as_a_string() {
        let expected: String = String::from("\"7518714136\"");
        let actual: String =
            serde_json::to_string(&ChatId::Id(7_518_714_136)).expect("chat id should serialize");
        assert_eq!(expected, actual);
    }

    #[test]
    fn username_serializes_as_a_string() {
        let expected: String = String::from("\"@mychannel\"");
        let actual: String = serde_json::to_string(&ChatId::username("mychannel"))
            .expect("chat id should serialize");
        assert_eq!(expected, actual);
    }
}
