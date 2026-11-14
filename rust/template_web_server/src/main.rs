use template_web_server::{Settings, web_server};
use webserver_base::bootstrap;
use webserver_base::webserver::{WebServer, WebServerError};

fn main() -> Result<(), WebServerError> {
    bootstrap!(|shutdown| async move {
        web_server(WebServer::from_env()?, Settings::from_env()?)
            .run(shutdown)
            .await
    })
}
