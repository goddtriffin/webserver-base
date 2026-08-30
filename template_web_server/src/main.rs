use webserver_base::templates::{
    BaseTemplateData, Fallback, GoddtriffinParams, PageTemplateData, ThemeColor,
};
use webserver_base::webserver::{FrontendParams, Pages, WebServer, WebServerError};
use webserver_base::{bootstrap, env};

fn main() -> Result<(), WebServerError> {
    bootstrap!(|shutdown| async move {
        WebServer::from_env()?
            .frontend(FrontendParams::from_env(
                BaseTemplateData::goddtriffin(GoddtriffinParams {
                    project: env::required("TWS_PROJECT")?,
                    description: env::required("TWS_DESCRIPTION")?,
                    base_url: env::required("TWS_BASE_URL")?,
                    social_image: String::from("static/image/social/todo.webp"),
                    theme_color: ThemeColor::light_dark("#fafafa", "#121212"),
                    theme_fallback: Fallback::Dark,
                    copyright_start: String::from("1998"),
                    style_sheets: vec![String::from("static/stylesheet/main.css")],
                    scripts: vec![],
                }),
                Pages::new()
                    .static_page(PageTemplateData::new("home", "Home", "/"), ())
                    .extend_images(["static/image/social/todo.webp"]),
            )?)
            .run(shutdown)
            .await
    })
}
