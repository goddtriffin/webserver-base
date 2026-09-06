# Code Style

How to write a line in this repo: comments, types, errors, tests, log severity,
and the README entry that has to accompany the change.

## Comments

- **Maximally succinct.** Never restate what the code already says.
- **Every comment must justify its existence.** If it would not teach a
  competent reader something the code cannot tell them, delete it.
- Comments are **purely additive**: they explain *why*, name a constraint, or
  record the failure mode that motivated the shape. The code is the source of
  truth for *what*.
- Doc comments on public items say what a caller needs, including the reason a
  thing is required or fixed.

## Style

- **Write the type out everywhere it can be written**: `let` bindings,
  parameters, struct fields, return types. Explicitness is the house style, not
  a fallback for when inference struggles.
- `#[must_use]` on builders and pure accessors.
- Errors are `thiserror` enums with a `#[source]`; error text tells the reader
  what to do, not just what happened.
- Follow standard practice for the language — Rust or TypeScript — but
  **defer to the surrounding code** where they disagree, especially on naming
  and error handling. Consistency with this repo beats a general convention.
- `unsafe_code` is forbidden.
- `cargo clippy --all-targets --all-features --workspace` must be clean before a
  commit.

### Silencing a lint

`#[allow]` is **forbidden** — `clippy::allow_attributes` is set to `forbid`, so
it will not compile. Fix the lint instead.

When silencing genuinely is correct, use `#[expect(lint, reason = "...")]` with
the reason filled in. It is safe in a way `#[allow]` is not:
`unfulfilled_lint_expectations` is also `forbid`, so an expectation that stops
firing becomes a hard error rather than quietly outliving the problem it was
written for.

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

House style:

```rust
let expected: T = ...;
let actual: T = ...;
assert_eq!(expected, actual);
```

**The expected value always comes first** — that ordering is absolute, because
a reversed `assert_eq!` prints a failure message that reads backwards. Binding
it to `expected` is the default and what most tests should do; a bare literal
inline (`assert_eq!(2, actual.len())`) is fine when the value is small enough
that a name would add nothing.

- Name a test as a **sentence describing the behaviour**, not the function:
  `a_trailing_slash_is_stripped_so_urls_never_double_up`, not `test_base_url`.
- Test the behaviour that matters and the reason it matters. A test that would
  still pass with the bug reintroduced is not a test.
- Every bug fixed gets a test that fails without the fix.
- Asserting a struct? Compare **every** field and sub-field, not a convenient
  subset — a partial assertion silently stops covering whatever is added later.
  Exempt only what cannot be deterministic: generated timestamps, UUIDs, ports.

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
