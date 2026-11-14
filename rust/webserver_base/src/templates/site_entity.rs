//! Who or what a site *is*, for schema.org.

use serde::{Deserialize, Serialize};

/// The entity behind a site, emitted as the `Person` or `Organization` node of
/// the home page's JSON-LD graph.
///
/// Google uses this node — and especially its `sameAs` links — to reconcile a
/// site with a known entity and to drive knowledge panels. Getting the type
/// wrong is a factual claim about the world: a band is not a person, and a
/// personal project is not a company.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SiteEntity {
    /// A human being.
    Person(String),
    /// A company, band, or any other group.
    Organization(String),
}

impl SiteEntity {
    /// A site that represents a person.
    #[must_use]
    pub fn person(name: impl Into<String>) -> Self {
        Self::Person(name.into())
    }

    /// A site that represents an organization.
    #[must_use]
    pub fn organization(name: impl Into<String>) -> Self {
        Self::Organization(name.into())
    }

    /// The schema.org `@type`.
    #[must_use]
    pub const fn schema_type(&self) -> &'static str {
        match self {
            Self::Person(_) => "Person",
            Self::Organization(_) => "Organization",
        }
    }

    /// The entity's name.
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Person(name) | Self::Organization(name) => name,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::SiteEntity;

    #[test]
    fn the_schema_type_follows_the_constructor_that_was_used() {
        let expected: &str = "Person";
        let actual: &str = SiteEntity::person("Todd Everett Griffin").schema_type();
        assert_eq!(expected, actual);

        let expected: &str = "Organization";
        let actual: &str = SiteEntity::organization("Palms Small Engine").schema_type();
        assert_eq!(expected, actual);
    }

    #[test]
    fn the_name_survives_either_constructor() {
        let expected: &str = "Triple Entendre";
        let actual: SiteEntity = SiteEntity::organization("Triple Entendre");
        assert_eq!(expected, actual.name());
    }
}
