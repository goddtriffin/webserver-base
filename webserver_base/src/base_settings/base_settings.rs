use std::env;

use super::Environment;

#[derive(Clone)]
pub struct BaseSettings {
    pub host: String,
    pub port: u16,
    pub environment: Environment,

    pub project_name: String,
    pub project_description: String,
    pub project_keywords: String,
    pub home_url: String,

    pub analytics_domain: String,
    pub sentry_dsn: String,
}

impl Default for BaseSettings {
    /// # Panics
    ///
    /// Will panic if env var `PORT` is not a u16.
    #[must_use]
    fn default() -> Self {
        // env
        let environment = env::var("ENVIRONMENT").map_or(Environment::Development, |s| {
            Environment::try_from(s.clone()).unwrap_or_else(|_| {
                panic!("failed to parse `ENVIRONMENT` environment variable: {s}")
            })
        });

        // host:port
        let host = env::var("HOST").unwrap_or_else(|_| {
            if environment == Environment::Development {
                "127.0.0.1".to_string()
            } else {
                "0.0.0.0".to_string()
            }
        });
        let port = env::var("PORT").map_or(8080, |s| {
            s.parse::<u16>()
                .expect("failed to parse `PORT` environment variable")
        });

        // project details
        let Ok(project_name) = env::var("PROJECT_NAME") else {
            panic!("environment variable `PROJECT_NAME` is not set");
        };
        let Ok(project_description) = env::var("PROJECT_DESCRIPTION") else {
            panic!("environment variable `PROJECT_DESCRIPTION` is not set");
        };
        let Ok(project_keywords) = env::var("PROJECT_KEYWORDS") else {
            panic!("environment variable `PROJECT_KEYWORDS` is not set");
        };
        let Ok(home_url) = env::var("HOME_URL") else {
            panic!("environment variable `HOME_URL` is not set");
        };

        // analytics domain
        let Ok(analytics_domain) = env::var("ANALYTICS_DOMAIN") else {
            panic!("environment variable `ANALYTICS_DOMAIN` is not set");
        };

        // sentry DSN
        let Ok(sentry_dsn) = env::var("SENTRY_DSN") else {
            panic!("environment variable `SENTRY_DSN` is not set");
        };

        // all settings
        Self {
            host,
            port,
            environment,

            project_name,
            project_description,
            project_keywords,
            home_url,

            analytics_domain,
            sentry_dsn,
        }
    }
}
