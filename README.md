# webserver-base

[![Crates.io](https://img.shields.io/crates/v/webserver-base)](https://crates.io/crates/webserver-base)
[![JSR](https://jsr.io/badges/@todd/webserver-base)](https://jsr.io/@todd/webserver-base)

Shared logic for Todd Everett Griffin's web servers, as feature-gated **kits**.

This library is deliberately opinionated. Most of what it does is not
configurable, because most of it has exactly one correct answer — the `<head>`,
the sitemap shape, where the icons come from. A knob that can only ever hold one
value is not flexibility, it is surface area. Projects supply the things that
genuinely differ (their name, their pages, their design) and inherit the rest.

This document is the reference. Every feature the library offers is described
here; if something is missing from this file, it is a documentation bug. Read the
code for low-level detail — it is always the source of truth.

---

## Install

```toml
[dependencies]
webserver-base = { version = "0.2", features = ["pages", "observability", "preset"] }
```

Nothing is enabled by default.

| feature | what it gives you |
|---|---|
| `templates` | the Handlebars registry, the embedded base layout, the template-data types |
| `analytics` | first-party proxies for Plausible and the Sentry browser SDK |
| `sitemap` | sitemap index and url-set generation |
| `observability` | Sentry + tracing, initialised in the one order that works |
| `telegram` | outbound Telegram Bot API notifier |
| `webserver` | the server builder, shared state, bootstrap, the static-asset pipeline |
| `pages` | page declarations producing routes *and* sitemap entries — implies `webserver`, `templates`, `sitemap`, `analytics` |
| `preset` | Todd Everett Griffin's identity defaults, including the shared `humans.txt` |
| `full` | all of the above |

---

# The Rust library

## A complete server

```rust
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
                    project: env::required("MY_PROJECT")?,
                    description: env::required("MY_DESCRIPTION")?,
                    base_url: env::required("MY_BASE_URL")?,
                    social_image: String::from("static/image/social/card.webp"),
                    theme_color: ThemeColor::light_dark("#fafafa", "#121212"),
                    theme_fallback: Fallback::Dark,
                    copyright_start: String::from("1998"),
                    style_sheets: vec![String::from("static/stylesheet/main.css")],
                    scripts: vec![String::from("static/script/main.js")],
                }),
                Pages::new().static_page(PageTemplateData::new("home", "Home", "/"), ()),
            )?)
            .run(shutdown)
            .await
    })
}
```

That is the whole file. There is no `Environment::from_env`, no `Observability`
wiring, and no 404 declaration, because those are identical in every project —
[`bootstrap`](#bootstrap) and [`frontend`](#frontend--the-line-between-a-site-and-a-service)
own them.

## `WebServer`

A server needs only a host, a port and an [`Environment`]. Everything else is
opt-in, so a sidecar serving one health check is a valid server.

| method | effect |
|---|---|
| `new` / `with_state` | explicit host, port and environment; `with_state` carries app state reachable as `state.app()` |
| `from_env` / `from_env_with_state` | reads `WSB_ENVIRONMENT` (required), `WSB_HOST` (defaults `127.0.0.1` locally, `0.0.0.0` in production) and `WSB_PORT` (defaults `8080`) |
| `body_limit` | request body cap, default 256 KiB — raise it for uploads |
| `nest` / `merge` / `nest_service` | your own routes |
| `frontend` | declares this server a website; see below |
| `run` | binds, serves, drains |

Always on, no method to enable them: `GET /api/v1/health`, the `/api/v1` prefix
itself, and the graceful-drain window.

On the shutdown signal the server stops accepting connections and returns as
soon as the last in-flight request finishes — with nothing in flight, that is
immediate. `DEFAULT_DRAIN_TIMEOUT` is the ceiling on that wait, not a pause: it
exists because a WebSocket never closes on its own, and reaching it logs a
warning and drops what remained.

Static assets are **presence-detected**: if a `static/` directory exists the
server serves `/static` and loads the manifest; if not, it does neither. There is
no `.assets()` call.

## `frontend` — the line between a site and a service

`WebServer::frontend` says "this server returns HTML to humans." Everything a
website needs then follows automatically, and every required input is a field on
`FrontendParams`, so forgetting one is a compile error rather than a page that
silently ships without a share card.

| field | why it is required |
|---|---|
| `base` | site identity: name, description, origin, social image, theme, copyright |
| `pages` | the routes, which are also the sitemap |
| `analytics` | the Plausible site — every frontend is measured |
| `sentry_browser_dsn` | browser errors, separate from the server's DSN |

Calling it gives you, with no further code: the embedded base layout, the
generated icon set, `site.webmanifest`, `robots.txt`, the sitemap index and its
url sets, first-party analytics, the Sentry browser tunnel, and the pre-paint
theme script.

`FrontendParams::from_env(base, pages)` reads the analytics id and browser DSN
for you.

**The 404 is not a per-project decision.** Every frontend gets
`PageTemplateData::new("404", "404", "/404").with_robots(NOINDEX_FOLLOW)`
automatically — the data is identical everywhere, so the library declares it.
What the page *looks* like is entirely yours, in `html/pages/404.hbs`.

### What a frontend must ship

Each of these is a boot error, because a site missing one is broken in a way no
test catches:

| requirement | why |
|---|---|
| `html/pages/*.hbs`, at least one | a frontend with no pages is not a frontend |
| `html/pages/404.hbs` | every frontend serves a 404 |
| one icon source — see [Icons](#icons) | the whole set derives from it |
| every declared asset present in the manifest | see below |

### Boot-time asset validation

Every asset a page will reference — site-wide and per-page stylesheets and
scripts, the social image, and all four generated icons — is proved to resolve
before the server binds, and **every** miss is reported at once.

Resolving a missing asset to its un-hashed path would produce a link that 404s
for the visitor and reports nothing to you: invisible from the inside. Failing at
*boot* rather than per-request is the safer half of that trade — a failed request
means someone sees a broken page, a failed boot means the deploy never cuts over
and the previous container keeps serving.

Absolute URLs are exempt: a CDN stylesheet will never be in the manifest, and
that is the one legitimate reason for a path to be absent. Assets a handler looks
up dynamically at request time cannot be enumerated at boot and remain yours to
get right.

## Templates

Templates live in `html/{layouts,pages,partials}` — a fixed convention, not a
setting. A template's name is its file stem, so `pages/home.hbs` is `{{> home}}`;
stems must be unique across all three directories.

### The base layout is embedded

`base` ships **inside this crate**. No project owns a copy, and a project file
that would shadow it is a boot error naming the offender. You may add any other
layout; you may not add this one.

That is the whole point. The `<head>` is pure function — spec conformance, Open
Graph, JSON-LD, icons, resource hints — so it is solved once here and inherited
everywhere, rather than copy-pasted eight times and left to drift.

A page supplies one block and extends it:

```handlebars
{{#*inline "body"}}
<main class="page-home">
  <h1>{{project}}</h1>
</main>
{{> footer}}
{{/inline}}
{{> base}}
```

The block is named `body`, not `main`, because it *is* everything inside
`<body>` — your header, your `<main>`, your footer. The layout contributes no
chrome at all: `<body>` is bare, and what a page looks like is entirely yours.

### What the layout emits

`<meta charset>` first, then viewport, title and description — the highest-value
crawler signals as high in the document as they go. Then `robots` and
`canonical`; `color-scheme` and one `theme-color` per scheme; the full Open Graph
block with an **absolute, content-hashed** `og:image` plus its real width, height
and MIME type; four X/Twitter tags (the rest provably fall back to Open Graph);
the icon set and manifest link; JSON-LD; the pre-paint theme script; auto-derived
`preconnect` hints; then stylesheets. Scripts go at the end of `<body>`.

### Template data

Three types, split by lifetime.

**`BaseTemplateData`** — per-server, what is true of every page. Built once, held
in server state. `BaseTemplateDataParams` names every field so two cannot be
transposed. `with_*`, `replace_*` and `extend_*` methods override afterwards.

A leading slash on `social_image` is stripped on the way in, so it keys into the
cache-buster map exactly like the stylesheet and script paths beside it.

**`PageTemplateData`** — per-render, what this page is. `new(template, page_name,
page_url)` plus optional overrides:

| method | effect |
|---|---|
| `with_description` / `with_social_image` / `with_social_image_alt` | override the site default for this page |
| `with_robots` | see the `robots` module for named directives |
| `with_jsonld` | attach a schema.org document; merged with the library's own |
| `with_article` | mark the page an article — this is what sets `og:type` and emits the `article:*` tags |
| `replace_style_sheets` / `extend_style_sheets` | and the `scripts` equivalents |

**`TemplateData`** — what Handlebars actually sees, assembled from both plus your
own data. Your data is namespaced under `{{app.…}}`; pass `()` for none.
Computed for you: `display_name` (`"{page_name} | {project}"`, the only way to
influence `<title>`), `canonical_url`, `locale`, `social_image_url`,
`copyright_end` (per render, so a long-running server does not keep claiming last
year), `preconnect`, `color_scheme` and `theme_colors`.

Strict mode is on: a field a template asks for and the data does not supply is a
render error, not an empty string. A blank `<title>` is a bug that ships; a
failed render is one that gets fixed.

Helpers available in any template: `join` (comma-joins a list), `pretty_date`
(`January 15, 1990`), `has_key` (whether a map holds a key).

### `ThemeColor`

Browser-chrome colour and supported schemes are one type, because they have to
agree — `color-scheme: light dark` with a single theme colour gives a dark page a
light address bar.

```rust
ThemeColor::light("#fafafa")                  // color-scheme: light
ThemeColor::dark("#121212")                   // color-scheme: dark
ThemeColor::light_dark("#fafafa", "#121212")  // color-scheme: light dark, two media-scoped tags
```

Use the page **background** per scheme, so the chrome reads as continuous with
the page rather than as an accent stripe above it.

### The theme script

`BaseTemplateData` carries a `theme_fallback` beside its colours, and the library
builds the script from it — there is nothing to construct or pass.

It is inline, blocking and pre-paint, so a page never flashes the wrong theme.
Three states, and the middle one matters:

1. the reader chose a theme → stamp `data-theme` on `<html>`;
2. the reader chose nothing but the OS states a preference → stamp **nothing**,
   so your media queries stay live and follow the OS mid-session;
3. neither states anything → stamp `theme_fallback`.

The storage key is always `theme`. What `[data-theme="dark"]` *means* is entirely
your CSS; this library owns only the handshake.

### `SiteEntity`

Who or what the site is, for the JSON-LD entity node. `SiteEntity::person(..)` or
`SiteEntity::organization(..)` — a band is not a person, and search engines
reconcile this claim against the wider web. The `goddtriffin` preset defaults to
`Person`; a project that is a business or a band overrides it with
`with_site_entity`.

### JSON-LD

On the **home page only** — Google requires `WebSite` markup at the domain root —
the library emits a `@graph` containing `WebSite` and your `SiteEntity`, with
`sameAs` populated from `see_also` and `logo` from the social image.

A page's own `with_jsonld` is **merged**, not replaced. Nodes match by `@id`
(falling back to `@type`); a matching node has its properties shallow-merged over
the default, so naming one property does not discard the rest, and a node
matching nothing is appended. Documents are hex-escaped, so a `</script>` inside
a string value cannot break out of the block.

## Pages

`Pages` declares routes and the sitemap together, so they cannot disagree.

```rust
Pages::new()
    .static_page(PageTemplateData::new("home", "Home", "/"), ())
    .dynamic_page("/blog/{slug}", handler)
    .extend_images(["/static/image/social/card.webp"])
    .with_last_modified(post.updated_at)
    .unlisted()                                   // route it, keep it out of the sitemap
```

There is no `not_found` — the library declares it.

`extend_images`, `with_last_modified` and `unlisted` apply to the page declared
immediately before them.

## Sitemaps

`/sitemap.xml` is **always** a `<sitemapindex>`, naming one or more
`/sitemap-N.xml` url sets — on a three-page site and a three-million-page one
alike. `robots.txt` and any Search Console submission therefore point at one URL
forever; a site that outgrows the protocol's 50,000-URL limit simply gains
another url set inside the index, and nothing outside the server changes.

Chunking respects both the URL count and a serialized-byte budget, because
`sitemap-rs` enforces only the former and image-heavy entries can pass that check
while still exceeding 50 MB.

`<changefreq>` and `<priority>` are not emitted: Google ignores both. `<lastmod>`
is the newest modification time across `html/`, `static/` and the binary —
stable across restarts, moving only when something that determines the output
actually changed. If that value is implausible (an epoch-normalised build, a
skewed clock) the tag is **omitted** and an `error!` is logged, because Google
uses `lastmod` only when it is consistently and verifiably accurate and one bad
pattern discredits the whole file.

Sitemaps are held in memory and served from there. Nothing is written to disk.

## Static assets

Hashing happens at **build time**, never at startup.

```
gen_css             →  static/stylesheet/main.css
gen_static_assets   →  derive the icon set from favicon.svg; hash every non-script
                       asset; write cache-buster.json and the TypeScript module
gen_js              →  the bundler inlines that module
gen_static_scripts  →  hash the built JavaScript
server boot         →  read the manifest. No hashing, no writes.
```

Two phases, because the JavaScript *contents* depend on the manifest while the
JavaScript *files* can only be hashed once they exist. Scripts never need their
own hash — the layout reads that from the manifest.

Both commands are subcommands of **your own server binary**, so no project needs
a shim binary or a line of glue:

```sh
my-server gen-static-assets
my-server gen-static-scripts
my-server                      # serve
```

They exit before the runtime or error monitoring start. They do still read
`WSB_ENVIRONMENT`, because `main` resolves it before calling in — set it to
`local` in your build stage.

**Everything under `static/` is hashed**, including the icons, because `/static/*`
is served immutable-for-a-year and every byte in it must therefore be
content-addressed. Well-known root routes (`/favicon.ico`, `/robots.txt`, …)
serve the same bytes never-cached, where the URL cannot change.

`cache-buster.json` and `static/script/generated/cache-buster.ts` are **build
outputs — gitignore both.** Add this to every consuming project:

```gitignore
cache-buster.json
static/script/generated/
```

### Icons

A project authors **exactly one** source, and everything else is generated:

- `static/image/favicon/favicon.svg` — preferred. You also get
  `<link rel="icon" type="image/svg+xml">` and, if the SVG carries its own
  `prefers-color-scheme` rules, a dark-mode-adaptive tab icon.
- `static/image/favicon/favicon-512.png` — exactly **512×512**, for art that
  cannot be vectorised. A photograph is the obvious case: autotracing a face
  yields either a posterised caricature or a multi-megabyte pile of paths. The
  size is in the filename so the requirement is hard to miss. No SVG link is
  emitted in this case, because linking a file that does not exist is worse than
  not linking one.

`favicon.ico` (32), `apple-touch-icon.png` (180), `icon-192.png` and
`icon-512.png` derive from whichever you provide. Every derived size is a
*downscale* from 512, which is faithful; upscaling never is, so a source smaller
than 512 is refused.

**The icon directory is a closed set.** It may contain only the source and the
four derived files:

```
static/image/favicon/
  favicon.svg  |  favicon-512.png     <- exactly one of these
  favicon.ico                          <- generated
  apple-touch-icon.png                 <- generated
  icon-192.png                         <- generated
  icon-512.png                         <- generated
```

Boot errors: neither source present; **both** present (two sources drift the
moment one is updated); a PNG that is not exactly 512×512; a derived icon that
is absent or the wrong size; or **any other file in the directory**. A stale
`favicon-16.png` from a previous design is invisible until the wrong picture
turns up in a browser tab, so it fails the boot instead.

A file already present is left alone, so a hand-tuned 32×32 `.ico` — the one
size where a naive downscale of a detailed mark really does look muddy — still
wins.

An SVG **must not rely on system fonts**; convert text to paths.

## Analytics and error monitoring

Both vendors' origins are on blocklists, and a blocked request is a visitor you
never counted or an error you never saw. This library serves both from your own
origin, which Plausible themselves recommend — they put the cost of not doing it
at 5–25% of visitors.

Routes are derived from the project name, never configured: Plausible advises
against their documented default paths (blocklists target them) and against
words like "analytics" or "stats". A per-project name also means no single filter
rule can take out every one of your sites at once. The shape — `name-hash.js` —
is what every bundler already emits.

```
GET  /script/{project}-{hash}.js   →  the Plausible script
POST /api/v1/{project}-{hash}      →  events, with the real visitor IP forwarded
GET  /script/{project}-{hash2}.js  →  the Sentry browser loader
POST /api/v1/{project}-{hash2}     →  Sentry envelopes
```

Both are **byte relays**. Nothing is parsed or re-reported — which matters most
for Sentry: a server that interpreted a client error and re-raised it through its
own SDK would file a browser problem as a server one, with the wrong stack. The
tunnel's destination comes from the configured DSN, not the envelope, so it
cannot be used as an open relay.

`X-Forwarded-For` carries the true client IP (resolved through the usual proxy
headers); without it Plausible's bot filter rejects proxied events outright.

Server-side and browser-side Sentry are **complementary, not alternatives**: the
Rust SDK catches panics and handler errors, the browser SDK catches JavaScript
exceptions. Use two Sentry projects — Sentry recommends one per language and per
deployable, and it isolates rate limits.

| variable | what it is |
|---|---|
| `WSB_ANALYTICS_ID` | the Plausible script id, e.g. `pa-1qi0TQ…` — Site Settings → Site Installation |
| `WSB_SENTRY_SERVER_DSN` | the Rust server's DSN |
| `WSB_SENTRY_BROWSER_DSN` | the browser's DSN — required for a frontend |

All three are required in **every** environment, local included. A DSN exercised
only in production is a DSN nobody has proved works; point local runs at
development projects.

⚠️ Plausible's script disables itself on `localhost`, so a local pageview will
not register no matter what you configure. To exercise it end to end, add your
development domain as its own Plausible site and map it to `127.0.0.1` in
`/etc/hosts`.

## Observability

`bootstrap` owns this. It resolves `WSB_ENVIRONMENT`, builds observability from
`WSB_SENTRY_SERVER_DSN`, and initialises it on the main thread *before* the
runtime — so the Sentry hub reaches the runtime's workers — then drops the guard
after the drain so errors raised during shutdown are still flushed. No project
writes any of that.

`bootstrap!` also owns the build-tool subcommands, checking them before it
touches the environment at all, so `gen-static-assets` needs no configuration.

**It is a macro, not a function**, and that is load-bearing. Sentry attributes
issues to a *release*, and the release has to name your application. Resolving it
inside this library — which is what `sentry::release_name!()` would do — makes
every project report `webserver-base@x.y.z`, so Sentry cannot tell one site's
deploys from another's and regression detection stops working. The macro expands
`CARGO_PKG_NAME` and `CARGO_PKG_VERSION` at *your* call site, so the release is
correct with nothing to configure.

## Logging

Format follows the environment, because a human reads one and a machine reads
the other:

```
local       INFO request{method=GET uri=/robots.txt version=HTTP/1.1}:
                 finished processing request latency=0 ms status=200

production  {"timestamp":"…","level":"INFO","message":"finished processing request",
             "span":{"name":"request","method":"GET","uri":"/robots.txt"},
             "status":200,"latency":"0 ms","target":"tower_http::trace::on_response"}
```

Production JSON is `flatten_event`ed with the current span attached, so `method`,
`uri`, `status` and `latency` are queryable fields rather than a string to grep.
ANSI colour is off there too — a log driver stores escape codes verbatim and
nothing downstream strips them.

Note the request span is created at **INFO**. `tower-http` defaults it to DEBUG,
which under an INFO filter means the span never exists and every request logs a
status and a latency with no way to tell which route it was.

One line at boot names the running build — the one thing no other log can tell
you when an incident starts:

```
INFO starting release=my-project@1.2.3 environment=production log_filter=info
```

`RUST_LOG` is honoured and stays unprefixed, being an ecosystem convention;
`with_log_filter` sets the fallback.

`RUST_LOG` is honoured; `with_log_filter` sets the fallback.

**Severity policy.** `sentry-tracing` maps `error!` to a Sentry event and
`warn!` to a mere breadcrumb, so a warning with no subsequent error is never
seen. Therefore: a boot failure for misconfiguration that makes the site wrong;
`error!` for "boots, but a feature is silently degraded"; `warn!`/`info!` only
for things nobody needs to act on.

## Telegram

`telegram` provides an outbound Bot API notifier: entity-based formatting (no
escaping), UTF-16 message chunking, per-chat rate limiting, `retry_after`-aware
retries, and structural bot-token redaction in errors and logs.

## Environment

Every variable this crate reads is prefixed `WSB_` so it cannot collide with
yours. `Environment::from_env` reads `WSB_ENVIRONMENT` (`local` or `production`).
The `env` module offers `required`, `optional` and `parse_or` for your own.

---

# The TypeScript library

Published to JSR as [`@todd/webserver-base`](https://jsr.io/@todd/webserver-base).

```jsonc
{ "imports": { "@todd/webserver-base": "jsr:@todd/webserver-base@^0.1.1" } }
```

## `@todd/webserver-base/bundle`

The esbuild wrapper every project's `gen_js` target runs. It discovers every
top-level `static/script/*.ts` as an entry point (subdirectories are pulled in
transitively, not bundled separately) and emits minified ES modules with source
maps into `bin/static/script/`.

```sh
deno run --allow-read --allow-write --allow-env --allow-net --allow-run \
  @todd/webserver-base/bundle
```

## `@todd/webserver-base/logger`

`ILogger` with three implementations: a real console logger, a no-op, and a mock
that records calls for assertions.

## `@todd/webserver-base/free-port`

Picks a stable per-project development port, so two of your servers running at
once do not collide and the port does not move between runs.

## The generated cache-buster module

Written by `gen_static_assets` to `static/script/generated/cache-buster.ts` and
inlined by the bundler, so resolving an asset costs the browser nothing:

```ts
import { CACHE_BUSTER, asset } from "./generated/cache-buster.ts";

img.src = asset("static/image/assets/flag.png");
//               ^ a typo here is a compile error, not a silent `undefined`
```

`CacheBustedPath` is the union of every known asset path. That type is the point:
the old untyped JSON map returned `undefined` for a mistyped key and produced a
broken image with no error anywhere.

---

## Conventions a consuming project inherits

```
html/
  pages/          at least one .hbs, and 404.hbs — both required
  partials/       your chrome
  layouts/        optional; `base` is reserved
static/
  image/favicon/favicon.svg    (or favicon-512.png at 512x512) — the rest is generated
  image/  script/  stylesheet/  scss/
cache-buster.json              build output — gitignored
static/script/generated/       build output — gitignored
```

Fixed and not configurable: the `html/` and `static/` directory names, the
`/api/v1` prefix, the `theme` storage key, the sitemap route shape, the derived
proxy paths, `<meta charset="utf-8">`, the viewport string, and the entire
`<head>`.
