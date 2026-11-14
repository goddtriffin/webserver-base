# toddstack

Shared code for all of Todd Everett Griffin's projects, in every language they are
written in. Each language has its own directory; each package inside it is
published on its own and documented in its own README.

## Packages

| package | language | published as | docs |
|---|---|---|---|
| `webserver-base` | Rust | [crates.io](https://crates.io/crates/webserver-base) | [`rust/README.md`](rust/README.md) |
| `@todd/webserver-base` | TypeScript | [JSR](https://jsr.io/@todd/webserver-base) | [`typescript/README.md`](typescript/README.md) |
| `template-web-server` | Rust | not published; Docker image for several sites | [`rust/template_web_server/`](rust/template_web_server/) |

## Layout

```
rust/              Cargo workspace: every Rust crate
  webserver_base/        the web server library
  template_web_server/   a complete site built on it (crate + html/ + static/ + Dockerfile)
typescript/        JSR packages (deno.jsonc is the package config)
  webserver-base/        the browser/build-side half of webserver-base
```

New languages (Swift, Kotlin, ...) get their own top-level directory when the
first project needs one.

## Working on this repo

Everything runs from the repo root through the `Makefile` (`make help`).

| skill | use it when |
|---|---|
| [`contribution-guide`](.claude/skills/contribution-guide/SKILL.md) | writing, reviewing, refactoring, testing or documenting **any** code here — invoke it at the start of every change, however small |
| [`rollout`](.claude/skills/rollout/SKILL.md) | shipping a finished change: patch bump, E2E smoke test, Docker push, pull request, tag, GitHub release, crates.io and JSR publishes |

Rules live in the skills, not here. Invoke `contribution-guide` before any
change to this repo.

Releases are tagged per package: `webserver-base-vX.Y.Z` (tags up to `v0.3.0`
predate the rename from `goddtriffin/webserver-base`).
