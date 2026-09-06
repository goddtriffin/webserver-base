---
name: contribution-guide
description: Use when writing, reviewing, refactoring, testing, or documenting any code in this repository — before editing Rust or TypeScript sources, adding or changing a feature, touching Cargo.toml or deno.jsonc, adding a dependency or feature gate, writing tests, choosing a log level, or updating README.md. Invoke it at the start of any change here, however small. Use the separate rollout skill when shipping a finished change.
---

# Contribution Guide

Invoke this before touching anything in this repo. The rules it points to are
the reason the library is small, and most of them exist because breaking them
fails silently.

## Load the reference you need

| reference | load it when |
|---|---|
| [`architecture.md`](reference/architecture.md) | changing behaviour: adding a parameter, touching the asset pipeline or boot path, adding a dependency or feature gate, wiring logging |
| [`code-style.md`](reference/code-style.md) | writing any line: comments, types, errors, tests, log severity, README updates |

Shipping a finished change is the **rollout** skill, not this one. Read its
step 0 before assuming a change needs a version bump: what you touched decides
what shipping costs, and a docs-only change is a commit and nothing else.

## Diagnose before you implement

Asked to solve a problem, do **not** open with a patch. First research it, then
report, in this order:

1. every likely cause you can find, not just the first that fits
2. the plausible solutions
3. which one you would pick, and why

Then stop and wait for the go-ahead. A request to investigate is not a request
to change code; the go-ahead has to come *after* the diagnosis, so it can be a
response to it. When the ask already carries an explicit instruction to fix,
still lead with the diagnosis — it is what makes the fix reviewable.

Hold security to the same shape. If a change would introduce something unsafe —
or if existing code is already unsafe — say so, name the risk and the possible
fixes, and wait. Do not quietly harden it and do not quietly ship it.

## The development loop

- **`make lint test`** is the gate: `cargo fmt --check`, `clippy --all-targets
  --all-features`, the Rust suite and doctests, plus `deno check`, `deno lint`,
  `deno doc --lint` and `deno test`. Run it from the repo root after
  implementing or modifying **any** code, and leave it clean. Not "before a
  commit" — after every change.
- **`make dev`** is the running site. It builds, regenerates assets in the
  right order, picks a stable per-project port, and serves.

**Do not reach for Docker while developing.** `make docker_build` is a
`linux/amd64` cross-build that runs under emulation and recompiles the whole
dependency tree. It belongs to the rollout smoke test and nowhere else.

## Two rules that outrank convenience

**Never hand-edit a build output.** `cache-buster.json`,
`static/script/generated/` and everything under `bin/` are generated. A lint or
type failure in one is a bug in the *generator* under
`webserver_base/src/assets/` — fix it there and regenerate.

**A feature that is not in `README.md` is a documentation bug.** Every feature
created, modified or deleted updates the README in the same change. See
`code-style.md` for how to write the entry.
