use webserver_base::analytics::AnalyticsConfig;
use webserver_base::env;
use webserver_base::observability::Observability;
use webserver_base::templates::{
    BaseTemplateData, GoddtriffinParams, PageTemplateData, TemplateRegistry, robots,
};
use webserver_base::webserver::{Pages, WebServer, WebServerError};
use webserver_base::{Environment, bootstrap};

fn main() -> Result<(), WebServerError> {
    let environment: Environment = Environment::from_env()?;

    bootstrap(
        Observability::from_env(environment)?,
        move |shutdown| async move {
            WebServer::from_env(environment)?
                .templates(
                    TemplateRegistry::from_dir("html")?,
                    BaseTemplateData::goddtriffin(GoddtriffinParams {
                        project: env::required("TWS_PROJECT")?,
                        description: env::required("TWS_DESCRIPTION")?,
                        keywords: keywords(&env::required("TWS_KEYWORDS")?),
                        base_url: env::required("TWS_BASE_URL")?,
                        social_image: String::from("/static/image/social/todo.webp"),
                        theme_color: String::from("#f7cb64"),
                        copyright_start: String::from("1998"),
                        style_sheets: vec![String::from("static/stylesheet/main.css")],
                        scripts: vec![String::from("static/script/main.js")],
                    }),
                )
                .assets("static")
                .write_cache_manifest("..")
                .analytics(AnalyticsConfig::from_env()?)
                .health()
                .pages(
                    Pages::new()
                        .static_page(PageTemplateData::new("home", "Home", "/"), ())
                        .extend_images(["/static/image/social/todo.webp"])
                        .not_found(
                            PageTemplateData::new("404", "404", "/404")
                                .with_robots(robots::NOINDEX_FOLLOW),
                            (),
                        ),
                )
                .run(shutdown)
                .await
        },
    )
}

/// Splits a comma-delimited keyword list, dropping blanks.
fn keywords(raw: &str) -> Vec<String> {
    raw.split(',')
        .map(str::trim)
        .filter(|keyword| !keyword.is_empty())
        .map(String::from)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::keywords;

    #[test]
    fn keywords_are_split_trimmed_and_compacted() {
        let expected: Vec<String> = vec![
            String::from("Todd"),
            String::from("Everett"),
            String::from("Griffin"),
        ];
        let actual: Vec<String> = keywords(" Todd , Everett ,, Griffin, ");
        assert_eq!(expected, actual);
    }

    #[test]
    fn a_single_keyword_needs_no_delimiter() {
        let expected: Vec<String> = vec![String::from("qr")];
        let actual: Vec<String> = keywords("qr");
        assert_eq!(expected, actual);
    }

    #[test]
    fn an_all_blank_list_yields_nothing_rather_than_an_empty_keyword() {
        let expected: Vec<String> = Vec::new();
        let actual: Vec<String> = keywords(" , , ");
        assert_eq!(expected, actual);
    }
}
