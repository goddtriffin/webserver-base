# @todd/webserver-base

[![JSR](https://jsr.io/badges/@todd/webserver-base)](https://jsr.io/@todd/webserver-base)

The TypeScript half of [`webserver-base`](https://github.com/goddtriffin/toddstack/tree/main/rust), part of
[toddstack](https://github.com/goddtriffin/toddstack).

Published to JSR as [`@todd/webserver-base`](https://jsr.io/@todd/webserver-base).

```jsonc
{ "imports": { "@todd/webserver-base": "jsr:@todd/webserver-base@^0.1.2" } }
```

## `@todd/webserver-base/bundle`

The esbuild wrapper every project's `gen_js` target runs. It discovers every
top-level `static/script/*.ts` as an entry point (subdirectories are pulled in
transitively, not bundled separately) and emits minified ES modules into
`bin/static/script/`.

**No source maps, ever.** A map embeds the project's original TypeScript, and
everything under `static/` is published at a content-hashed URL, so a map built
anywhere ships to production. Not configurable: the build that would want one
locally is the same `gen_js` the Docker image runs.

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
