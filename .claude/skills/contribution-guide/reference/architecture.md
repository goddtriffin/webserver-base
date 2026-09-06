# Architecture

What must remain true about this library. Each rule below is here because
breaking it fails silently, remotely, or only for consumers of the published
crate.

## The library's prerogative

**webserver-base has the prerogative to be opinionated.** It exists to solve a
problem once for every project that consumes it, not to offer every project a
different answer.

**It is perfectly okay — and often required — to offer a non-configurable
default for every feature this library provides, in order to minimise API
surface area.** Before adding a parameter, ask what a second value would look
like and whether it would ever be correct. A knob that can only hold one value
is not flexibility; it is surface area, and it is a lie about what varies.

Currently non-configurable, deliberately: the `html/` and `static/` directory
names, the `/api/v1` prefix, the `theme` storage key, the sitemap route shape,
the derived analytics and Sentry proxy paths, `<meta charset="utf-8">`, the
viewport string, and the whole of `<head>`.

## Invariants

Break one of these and the failure is silent or remote, so they are worth
stating:

- **`assets/**` must stay in `Cargo.toml`'s `include` list.** The base layout
  and `humans.txt` are pulled in with `include_str!`; omitting them compiles
  locally and fails for every consumer of the published crate.
- **`base` is a reserved template name.** It is embedded, and `from_dir` errors
  on a project file that would shadow it. Do not add a way to override it —
  silently disabling the `<head>` is exactly the failure this prevents.
- **Hashing happens at build time, never at startup.** The server reads the
  manifest; it must never rename a file. Startup hashing was not idempotent,
  and an in-place restart 404'd every asset on the site.
- **Boot order:** generate icons → hash non-scripts → write manifest → build JS
  → hash scripts. The JavaScript inlines the manifest, so hashing precedes it;
  the built JavaScript can only be hashed once it exists.
- **The server writes nothing to disk.** `robots.txt`, `humans.txt`,
  `site.webmanifest` and the sitemaps are generated or embedded and served from
  memory. A production image must stay runnable read-only.
- **Everything under `static/` is content-hashed**, because `/static/*` is
  served immutable-for-a-year and an un-hashed URL there could serve stale bytes
  for a year.
- **The icon directory is a closed set** — one source plus the four derived
  files, nothing else. Extending it means extending `ALLOWED_FAVICON_FILES`
  deliberately, not letting stray files through.
- **Shutdown cleanup runs *concurrently* with the connection drain, under one
  shared ceiling.** Never sequence them. `DEFAULT_DRAIN_TIMEOUT` is 10s and
  Docker's default kill grace is also 10s, so anything scheduled after the drain
  is killed rather than run — and the messages it was meant to save are lost in
  the deploys where losing them matters most.
- **Boot validation must stay exhaustive.** Every declared asset is proved to
  resolve before the server binds. Never reintroduce a fallback to the un-hashed
  path: it turns a loud misconfiguration into a link that 404s for the visitor
  and reports nothing to us.

## Feature gates

- `default = []`. Nothing is on unless asked for.
- Every dependency is `optional = true` and named by exactly the gates that need
  it. Never make a kit's dependency unconditional.
- **Four are deliberately unconditional**: `serde`, `serde_json`, `thiserror`
  and `tracing`. Every kit needs them, and gating them would mean cfg-ing every
  error enum and every log line in the crate. They are the floor a consumer
  pays with `default = []`; nothing else may join them without the same
  argument.
- **A sidecar must always boot.** `WebServer::from_env()?.run(shutdown)` with no
  frontend needs no templates, no icons, no analytics, and no `static/`. If a
  change makes that fail, the change is wrong.

## Logging setup

- **`bootstrap!` is a macro on purpose.** The Sentry release must name the
  *application*; resolved inside this library every project would report
  `webserver-base@x.y.z` and Sentry could not attribute an issue to a deploy.
  Never replace it with a plain function, and never use
  `sentry::release_name!()` here.
- **The request span must stay at `Level::INFO`.** `tower-http` defaults it to
  DEBUG, which under an INFO filter drops the method and URI from every request
  log.
- Local emits the human format, production emits JSON with ANSI off.

Which macro to reach for is a code-style question — see
[`code-style.md`](code-style.md).
