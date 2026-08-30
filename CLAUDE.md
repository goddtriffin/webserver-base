# CLAUDE.md

Working rules for this repository.

## The library's prerogative

**webserver-base has the prerogative to be opinionated.** It exists to solve a
problem once for every project that consumes it, not to offer every project a
different answer.

**It is perfectly okay — and often required — to offer a non-configurable
default for every feature this library provides, in order to minimise API
surface area.** Before adding a parameter, ask what a second value would look
like and whether it would ever be correct. A knob that can only hold one value is
not flexibility; it is surface area, and it is a lie about what varies.

Currently non-configurable, deliberately: the `html/` and `static/` directory
names, the `/api/v1` prefix, the `theme` storage key, the sitemap route shape,
the derived analytics and Sentry proxy paths, `<meta charset="utf-8">`, the
viewport string, and the whole of `<head>`.

## Documentation

**Every time a feature is created, modified, or deleted, `README.md` must be
updated under the relevant section** — "The Rust library" or "The TypeScript
library". A feature that is not in the README is a documentation bug.

- Write from the **consumer's** perspective: how to use it properly, and what it
  does for them. Not how it is implemented.
- **Maximally succinct yet maximally insightful.** Say the thing that is not
  obvious from the signature — the constraint, the failure mode, the reason.
- **Never link to line numbers**, or to anything else that drifts. The code is
  always the source of truth; the README exists so a reader knows what to look
  for and why. Point at modules and types, not positions.

## Comments

- **Maximally succinct.** Never restate what the code already says.
- **Every comment must justify its existence.** If it would not teach a competent
  reader something the code cannot tell them, delete it.
- Comments are **purely additive**: they explain *why*, name a constraint, or
  record the failure mode that motivated the shape. The code is the source of
  truth for *what*.
- Doc comments on public items say what a caller needs, including the reason a
  thing is required or fixed.

## Versioning

- The Rust crate follows semver in `[workspace.package] version`.
- **`deno.jsonc` is bumped by PATCH only.** The TypeScript library is small and
  its consumers pin a caret range; a minor or major bump forces every project to
  edit its import map for no benefit.

## Invariants

Break one of these and the failure is silent or remote, so they are worth
stating:

- **`assets/**` must stay in `Cargo.toml`'s `include` list.** The base layout and
  `humans.txt` are pulled in with `include_str!`; omitting them compiles locally
  and fails for every consumer of the published crate.
- **`base` is a reserved template name.** It is embedded, and `from_dir` errors
  on a project file that would shadow it. Do not add a way to override it —
  silently disabling the `<head>` is exactly the failure this prevents.
- **Hashing happens at build time, never at startup.** The server reads the
  manifest; it must never rename a file. Startup hashing was not idempotent, and
  an in-place restart 404'd every asset on the site.
- **Boot order:** generate icons → hash non-scripts → write manifest → build JS →
  hash scripts. The JavaScript inlines the manifest, so hashing precedes it; the
  built JavaScript can only be hashed once it exists.
- **The server writes nothing to disk.** `robots.txt`, `humans.txt`,
  `site.webmanifest` and the sitemaps are generated or embedded and served from
  memory. A production image must stay runnable read-only.
- **Everything under `static/` is content-hashed**, because `/static/*` is served
  immutable-for-a-year and an un-hashed URL there could serve stale bytes for a
  year.
- **The icon directory is a closed set** — one source plus the four derived
  files, nothing else. Extending it means extending `ALLOWED_FAVICON_FILES`
  deliberately, not letting stray files through.
- **Boot validation must stay exhaustive.** Every declared asset is proved to
  resolve before the server binds. Never reintroduce a fallback to the un-hashed
  path: it turns a loud misconfiguration into a link that 404s for the visitor
  and reports nothing to us.

## Feature gates

- `default = []`. Nothing is on unless asked for.
- Every dependency is `optional = true` and named by exactly the gates that need
  it. Never make a kit's dependency unconditional.
- **A sidecar must always boot.** `WebServer::from_env()?.run(shutdown)` with
  no frontend needs no templates, no icons, no analytics, and no `static/`. If a
  change makes that fail, the change is wrong.

## Logging

- **`bootstrap!` is a macro on purpose.** The Sentry release must name the
  *application*; resolved inside this library every project would report
  `webserver-base@x.y.z` and Sentry could not attribute an issue to a deploy.
  Never replace it with a plain function, and never use `sentry::release_name!()`
  here.
- **The request span must stay at `Level::INFO`.** `tower-http` defaults it to
  DEBUG, which under an INFO filter drops the method and URI from every request
  log.
- Local emits the human format, production emits JSON with ANSI off.

## Logging severity

`sentry-tracing` maps `error!` to a Sentry event and `warn!` to a breadcrumb, so
**a `warn!` with no subsequent error is never seen by anyone.** Therefore:

- **Boot failure** — misconfiguration that makes the site wrong: no icon source,
  no page templates, no `404.hbs`, a project `base.hbs`, a malformed DSN, a
  declared asset absent from the manifest.
- **`error!`** — it boots and serves, but a feature is silently degraded, and a
  human must act: an undersized social image, an implausible content mtime, a
  truncated `robots.txt`. These *must* reach Sentry.
- **`warn!` / `info!`** — informational only. Never use `warn!` for something a
  human needs to act on.

## Tests

House style, followed everywhere:

```rust
let expected: T = ...;
let actual: T = ...;
assert_eq!(expected, actual);
```

- Name a test as a **sentence describing the behaviour**, not the function:
  `a_trailing_slash_is_stripped_so_urls_never_double_up`, not `test_base_url`.
- Test the behaviour that matters and the reason it matters. A test that would
  still pass with the bug reintroduced is not a test.
- Every bug fixed gets a test that fails without the fix.

## Style

- Explicit types on `let` bindings, matching the surrounding code.
- `#[must_use]` on builders and pure accessors.
- Errors are `thiserror` enums with a `#[source]`; error text tells the reader
  what to do, not just what happened.
- `unsafe_code` is forbidden and `allow` attributes are forbidden — fix the lint
  rather than silencing it.
- `cargo clippy --all-targets --all-features --workspace` must be clean before a
  commit.
