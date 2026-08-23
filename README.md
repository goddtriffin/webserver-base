# webserver-base

[![Version](https://img.shields.io/crates/v/webserver-base)](https://crates.io/crates/webserver-base)
[![Docs](https://docs.rs/webserver-base/badge.svg)](https://docs.rs/webserver-base)
[![JSR](https://jsr.io/badges/@todd/webserver-base)](https://jsr.io/@todd/webserver-base)

A Rust library which contains shared logic for all of my webserver projects.

## Kits

A collection of **kits**. Each kit is self-contained, opinionated, and independently feature-gated —
take one, or use them all together. Nothing is enabled by default, so a bot that wants only Telegram
notifications does not compile axum or Handlebars.

| feature         | what it is                                                                |
| --------------- | ------------------------------------------------------------------------- |
| `webserver`     | the server builder, shared state, `bootstrap`, graceful shutdown           |
| `pages`         | page declarations that produce routes **and** `sitemap.xml`                |
| `templates`     | Handlebars registry and the three template-data types                      |
| `assets`        | content-hashed static files and the cache-control middleware pair          |
| `analytics`     | Plausible event forwarding with true-client-IP resolution                  |
| `sitemap`       | `sitemap.xml` generation                                                   |
| `observability` | Sentry + tracing, initialised in the one order that works                  |
| `telegram`      | outbound Bot API notifier: entities (no escaping), chunking, rate limiting |
| `preset`        | Todd Everett Griffin's personal defaults                                   |
| `full`          | all of the above                                                           |

```toml
# a bot
webserver-base = { version = "0.2", features = ["telegram"] }

# a sidecar that serves one health check
webserver-base = { version = "0.2", features = ["webserver"] }

# a full site
webserver-base = { version = "0.2", features = ["full"] }
```

## Usage

The application owns `main`. [`bootstrap`] handles what is process-global and easy to order wrongly
— error monitoring, the runtime, signal handling — and hands back a `Shutdown` handle, so one binary
can run several servers and drain them together.

```rust,no_run
fn main() -> Result<(), WebServerError> {
    let environment = Environment::from_env()?;

    bootstrap(Observability::from_env(environment)?, move |shutdown| async move {
        WebServer::from_env(environment)?
            .templates(TemplateRegistry::from_dir("html")?, base_template_data())
            .assets("static")
            .analytics(AnalyticsConfig::from_env()?)
            .health()
            .pages(pages())
            .run(shutdown)
            .await
    })
}
```

`template_web_server/` is the reference consumer: 73 lines, and the whole server.

## Configuration

Every kit is constructed by hand first; `from_env` is a convenience on top. Every environment
variable this crate reads is prefixed `WSB_`, so it cannot collide with a project's own.

| variable                | kit             | required                            |
| ----------------------- | --------------- | ----------------------------------- |
| `WSB_ENVIRONMENT`       | core            | always (`local` or `production`)    |
| `WSB_HOST`              | `webserver`     | no — `127.0.0.1` / `0.0.0.0`        |
| `WSB_PORT`              | `webserver`     | no — `8080`                         |
| `WSB_SENTRY_DSN`        | `observability` | **in production only**              |
| `WSB_ANALYTICS_DOMAIN`  | `analytics`     | when analytics is enabled           |
| `RUST_LOG`              | `observability` | no — `info`; unprefixed by convention |

`template_web_server` is one image serving many domains (see `make docker_push`), so the copy that
differs per site is its own config, under `TWS_*`. All four are required — a site that booted
without them would serve the template's placeholder copy under a real domain.

| variable          | becomes                          |
| ----------------- | -------------------------------- |
| `TWS_PROJECT`     | `GoddtriffinParams.project`      |
| `TWS_DESCRIPTION` | `GoddtriffinParams.description`  |
| `TWS_KEYWORDS`    | `GoddtriffinParams.keywords` (comma-delimited) |
| `TWS_BASE_URL`    | `GoddtriffinParams.base_url`     |

## Developers

**Project is under active maintenance - even if there are no recent commits! Please submit an issue / bug request if the
library needs updating for any reason!**

### Commands

- `make lint`
- `make test`
- `make fix`

## Credits

Made by [Todd Everett Griffin](https://www.toddgriffin.me/).
