---
name: rollout
description: Use when shipping finished work in this repo — cutting a version, bumping Cargo.toml or deno.jsonc for a release, running the end-to-end smoke test, building or pushing the Docker image, committing to main, tagging, drafting a GitHub release, or publishing webserver-base to crates.io or @todd/webserver-base to JSR. Also use when asked whether a change is ready to ship, whether a version bump or publish is needed at all, when picking the next version number, or when any question arises about which rollout step comes next.
---

# Rollout

Shipping this repo is a **fixed twelve-step sequence**. The steps never
reorder and never merge. Which of them run is decided by what changed — see
step 0 — but a step that is in scope is never skipped because a change "looks
small."

Writing the code is the **contribution-guide** skill. This one starts once the
change is finished.

## Step 0: what changed decides what runs

**Do not run steps that nothing changed.** Version bumps, smoke tests, image
pushes and publishes are each earned by a change to what they cover. A
docs-only change that bumps a version ships a release consumers must read for
nothing, and re-pushes seven production images for nothing.

Diff against `main` and classify:

| what changed | earns |
|---|---|
| `webserver_base/**` (the Rust lib) | Cargo bump, tag, release, crates.io publish — **and** the image steps, because the binary embeds the library |
| `static/script/webserver-base/**` (the TS lib) | `deno.jsonc` PATCH bump, JSR publish |
| `template_web_server/**`, `html/**`, `static/` (art, styles, non-lib scripts), `Dockerfile`, `Makefile` | the image steps: build, smoke test, push |
| `README.md`, `CLAUDE.md`, `.claude/**`, other docs | nothing but the commit |

Then run only the steps those rows earn, **in the order below**, skipping the
rest. The order never changes; only which steps are present.

Two consequences worth stating outright:

- **A docs-only change is `make lint test` and a commit.** No bump, no Docker,
  no tag, no publish.
- **A Rust-lib change always drags the image with it.** `template-web-server`
  compiles `webserver_base` in, so the built image is stale the moment the
  library moves. Never publish the crate without pushing the image.

The tag and the GitHub release track the **Rust crate version**, so they are cut
only when that version moves. A TS-only change is a commit plus a JSR publish,
with no tag — there is no new Rust version to name one after.

When in doubt about a row, run the step. The cost of a needless smoke test is
minutes; the cost of a skipped one is a stale production image.

## The order

The constraint that fixes everything else: **the production deploy watches
commits to `main` and pulls `:latest`.** The image must already be in the
registry when the commit lands. Push the image, *then* commit.

Reverse those two and the deploy fires against the new commit while the registry
still holds the old image — production runs stale code, with no error anywhere.

| # | Step | Command |
|---|---|---|
| 1 | **Bump versions** | edit `Cargo.toml` + `deno.jsonc` |
| 2 | Green gate | `make lint test` |
| 3 | Build the image | `make docker_build` |
| 4 | E2E smoke test | `make docker_run`, then browse |
| 5 | Tear down | `make docker_stop` |
| 6 | **Push the image** | `make docker_push` |
| 7 | **Commit to main** | `git commit` + `git push` |
| 8 | Tag | `git tag vX.Y.Z` + `git push origin vX.Y.Z` |
| 9 | GitHub release | `gh release create` |
| 10 | Dry-run both libs | `make publish_dry_run` |
| 11 | Publish Rust | `cargo publish --package webserver-base` |
| 12 | Publish TypeScript | **the user runs** `deno publish` |

Work directly on `main`. No feature branch, no pull request — that is the
default here, not a shortcut.

---

## 1. Bump versions — first, before anything is built

The two libraries version independently:

- The **Rust crate** follows semver in `[workspace.package] version`.
- **`deno.jsonc` is bumped by PATCH only.** The TypeScript library is small and
  its consumers pin a caret range; a minor or major bump forces every project to
  edit its import map for no benefit.

Bump from what is on `main` now, not from what is published — the two drift when
a version is committed but never released.

**Why this is step 1 and not step 2.** Bumping mutates `Cargo.toml` and
`Cargo.lock`, which are build inputs, and `bootstrap!` resolves
`CARGO_PKG_VERSION` at *compile* time into the Sentry release string. Bump after
the green gate and the artifact you ship was never the artifact you verified.
Step 1 is the point after which nothing about the build changes until it ships.

## 2. Green gate

`make lint test` must be fully clean — see contribution-guide for what it covers
and for the rule that a lint failure in generated output is a bug in the
generator.

## 3. Build the image

`make docker_build`. Slow, because it is an emulated `linux/amd64` cross-build.
That is expected.

## 4. E2E smoke test

`make docker_run` prints the port it chose. `test.toddgriffin.me` maps to
`127.0.0.1` in `/etc/hosts`, so browse **`http://test.toddgriffin.me:PORT`**.

Confirm in the browser, not just with curl:

- **the home page** renders, styled, with the theme applied
- **a 404 URL** renders the real 404 page — this is the load-bearing check,
  because its content-hashed image proves the whole cache-buster pipeline
  survived the build
- the **console is clean**

Then the routes the layout depends on: `/api/v1/health`, `/robots.txt`,
`/humans.txt`, `/site.webmanifest`, `/sitemap.xml`, `/favicon.ico` — all 200,
an unknown path 404.

Finally, check the container's first log line names the version from step 1
(`release=template-web-server@X.Y.Z`). A mismatch means the image predates the
bump: go back to step 3.

## 5. Tear down

`make docker_stop`.

## 6. Push the image — before the commit

`make docker_push` tags the single built image into **seven** production Docker
Hub repos and pushes each: `scannable-codes-website`, `turnbased-website`,
`scribble-jump-website`, `video-game-recipe-book-website`, `vogue-bot-website`,
`5dcheckers-server`, `5ddiplomacy-server`.

Say so plainly when reporting it — seven live `:latest` tags are moving, not
one. Confirm every push reports the same digest; a differing digest means a repo
got a stale image.

## 7. Commit to main

Directly on `main`. Verify nothing under `bin/`, `cache-buster.json`, or
`static/script/generated/` is staged — all three are build output.

Match the existing commit style: a one-line summary, then a body of grouped
bullets that say *why* each change exists and what failure motivated it, closing
with `Bump Rust lib version to X.Y.Z and JSR lib version to A.B.C.`

## 8. Tag

Tags are **lightweight** (`git tag vX.Y.Z`, no `-a`) and take the name from the
**Rust** workspace version — never the deno one. Push explicitly:
`git push origin vX.Y.Z`.

## 9. GitHub release

`gh release create vX.Y.Z --title "vX.Y.Z" --verify-tag`. The body is a bullet
changelog grouped by area, with a **Breaking changes** section whenever a
consuming project must edit something to upgrade, closing with:

```
**Full Changelog**: https://github.com/goddtriffin/webserver-base/compare/vPREV...vX.Y.Z
```

`vPREV` is the previous **tag**, which is not always the previous release — tags
and releases have drifted here before. Check both, and when a tag was never
released, cover its contents in this changelog so nothing goes undocumented.

## 10. Dry-run both libraries

`make publish_dry_run`. It runs `cargo publish --dry-run`, which **refuses a
dirty working tree** — which is precisely why this step sits after the commit.
Run it earlier for a packaging sanity check if you like, but expect that refusal
until step 7 has landed.

Confirm the embedded assets are in the packaged crate:

```sh
cargo package --list -p webserver-base | grep '^assets/'
```

Two lines of output. Empty output is the dirty-tree refusal on stderr, not a
missing asset — do not misread one as the other. Why their absence matters is
the `assets/**` invariant in contribution-guide's `architecture.md`.

## 11. Publish Rust

`cargo publish --package webserver-base`. **Irreversible** — crates.io allows
yanking, never deletion.

## 12. Publish TypeScript — the user's step

`deno publish` needs interactive browser auth and fails in an agent shell with
`error: No means to authenticate`. Do not work around this and do not handle a
token. Ask the user to run `! deno publish` themselves.

---

## Verify what actually landed

Never report a publish from the command's own output alone. crates.io needs a
User-Agent or it returns non-JSON:

```sh
curl -s -H "User-Agent: release-check (tgriffin115@gmail.com)" \
  https://crates.io/api/v1/crates/webserver-base | grep -o '"max_version":"[^"]*"'
curl -s https://jsr.io/@todd/webserver-base/meta.json | grep -o '"latest":"[^"]*"'
```

## If a step fails

Everything through step 5 is free to retry. From step 6 on, each step is visible
to someone else, so stop and report rather than pressing ahead — a half-rolled-out
version is worth a human decision.

The asymmetry worth knowing: steps 6 and 7 are the deploy. If step 11 fails, the
new code is *already in production* and only the library publish is missing,
which is recoverable. That is why step 10 exists.
