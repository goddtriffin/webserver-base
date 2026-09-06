# CLAUDE.md

Working rules for this repository.

This file is a router. It holds no rules of its own — every rule lives in the
skill that owns it, listed below.

## Skills

| skill | use it when |
|---|---|
| [`contribution-guide`](.claude/skills/contribution-guide/SKILL.md) | writing, reviewing, refactoring, testing or documenting **any** code here — invoke it at the start of every change, however small |
| [`rollout`](.claude/skills/rollout/SKILL.md) | shipping a finished change: version bump, E2E smoke test, Docker push, commit to main, tag, GitHub release, crates.io and JSR publishes |

**Invoke `contribution-guide` before touching anything in this repo.** Most of
what it points to exists because breaking it fails silently, remotely, or only
for consumers of the published crate — so the cost of skipping it is not visible
at the time you skip it.

`README.md` is the consumer-facing reference for both libraries; keep it current
as part of any feature change.
