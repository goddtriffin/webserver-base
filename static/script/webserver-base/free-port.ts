/**
 * Derives a stable, per-project localhost port for local development.
 *
 * Every project maps to exactly one port, derived from its slug. The same slug always
 * yields the same port, on every run and on every machine, regardless of what else is
 * running. Ports are never reassigned: if the derived port is occupied, this fails loudly
 * rather than silently moving the project somewhere else.
 *
 * Usage:
 *
 * ```sh
 * deno run --allow-net --allow-run=lsof free-port.ts <project-slug>
 * ```
 *
 * @module
 */

/** The lowest port this module will ever hand out. */
const BASE_PORT = 8080;

/**
 * The size of the port window, starting at {@linkcode BASE_PORT}.
 *
 * 8080-9079 sits well below macOS's ephemeral range (49152+), so a port in this window is
 * never claimed at random by an outbound connection; anything holding one is a real server.
 */
const PORT_SPAN = 1000;

/**
 * Hashes a project slug via FNV-1a (32-bit).
 *
 * @param slug The project slug to hash.
 * @returns An unsigned 32-bit hash.
 */
function hashProjectSlug(slug: string): number {
  let hash = 2166136261;
  for (let i = 0; i < slug.length; i++) {
    hash ^= slug.charCodeAt(i);
    hash = Math.imul(hash, 16777619) >>> 0;
  }
  return hash;
}

/**
 * Derives the port belonging to a project slug.
 *
 * Pure and deterministic: this says nothing about whether the port is currently available.
 *
 * @param slug The project slug.
 * @returns The port assigned to that slug.
 */
export function portForProject(slug: string): number {
  return BASE_PORT + (hashProjectSlug(slug) % PORT_SPAN);
}

/**
 * The addresses a port must be bindable on to count as free.
 *
 * Both are required because neither alone is sufficient. On macOS a wildcard bind and a
 * loopback bind do not conflict, so `0.0.0.0` still binds cleanly while a loopback-only
 * server holds the port, and `127.0.0.1` still binds cleanly while a wildcard server (such
 * as Docker's host-side proxy) holds it. Each probe misses exactly the case the other
 * catches. Requiring both also matches the question actually being asked: these servers bind
 * either `0.0.0.0` or `127.0.0.1` depending on `HOST`, so the port must be usable for both.
 */
const PROBE_HOSTNAMES: readonly string[] = ["0.0.0.0", "127.0.0.1"];

/**
 * Reports whether a port can currently be bound.
 *
 * Binds rather than connects, so the answer reflects whether a server could actually take
 * the port rather than merely whether something is reachable on it.
 *
 * @param port The port to test.
 * @returns Whether the port is free on every address in {@linkcode PROBE_HOSTNAMES}.
 */
export function isPortFree(port: number): boolean {
  return PROBE_HOSTNAMES.every((hostname: string) => {
    try {
      const listener: Deno.Listener = Deno.listen({ hostname, port, transport: "tcp" });
      listener.close();
      return true;
    } catch {
      return false;
    }
  });
}

/**
 * Describes whatever is currently listening on a port, for error reporting only.
 *
 * Degrades to `null` when `lsof` is missing or run permission was not granted, so callers
 * never depend on it succeeding.
 *
 * @param port The port to inspect.
 * @returns A human-readable description of the listener, or `null` if it could not be determined.
 */
function describePortHolder(port: number): string | null {
  try {
    const { success, stdout } = new Deno.Command("lsof", {
      args: ["-nP", `-iTCP:${port}`, "-sTCP:LISTEN"],
      stdout: "piped",
      stderr: "null",
    }).outputSync();
    if (!success) {
      return null;
    }

    const lines: string[] = new TextDecoder().decode(stdout).trim().split("\n");
    return lines.length > 1 ? lines.slice(1).join("\n") : null;
  } catch {
    return null;
  }
}

/**
 * Builds the diagnostic shown when a project's port is already taken.
 *
 * Causes are ordered by likelihood so that both humans and coding agents resolve them in
 * the right order, and so nobody "fixes" this by quietly choosing a different port.
 *
 * @param slug The project slug.
 * @param port The port that was unavailable.
 * @returns The full multi-line error text.
 */
function buildUnavailableMessage(slug: string, port: number): string {
  const holder: string | null = describePortHolder(port);

  return [
    `error: port ${port} is unavailable`,
    ``,
    `  project:  ${slug}`,
    `  required: ${port}`,
    ``,
    holder === null ? `  currently held by: (could not determine)` : `  currently held by:\n${holder}`,
    ``,
    `  This port is derived deterministically from the project slug, so it is the same`,
    `  on every run and is never reassigned. Something else currently holds it.`,
    ``,
    `  Likely causes, in order:`,
    ``,
    `    1. A previous instance of THIS project is still running.`,
    `         identify:  lsof -nP -iTCP:${port} -sTCP:LISTEN`,
    `         resolve:   stop the stray \`make dev\` process, then retry`,
    ``,
    `    2. This project's Docker container is still up.`,
    `         identify:  docker compose ps`,
    `         resolve:   make docker_stop, then retry`,
    ``,
    `    3. A different project's slug hashes to this same port (rare).`,
    `         resolve:   change the slug argument passed to free-port in this repo's`,
    `                    Makefile (e.g. "${slug}" -> "${slug}-web"). This is a permanent,`,
    `                    committed fix; the new slug derives a different port.`,
    ``,
    `    4. An unrelated process holds the port.`,
    `         resolve:   stop that process, or override for a single run:`,
    `                    PORT=9999 make dev`,
    ``,
    `  Do NOT resolve this by picking a different port ad hoc -- the whole point of the`,
    `  fixed port is that it never moves. Prefer causes 1-2 before 3-4.`,
  ].join("\n");
}

/**
 * Resolves a project's port, asserting that it is currently free.
 *
 * @param slug The project slug.
 * @returns The port assigned to that slug.
 * @throws {Error} If the port is already in use, with a diagnostic naming the holder.
 */
export function freePortForProject(slug: string): number {
  const port: number = portForProject(slug);
  if (!isPortFree(port)) {
    throw new Error(buildUnavailableMessage(slug, port));
  }
  return port;
}

if (import.meta.main) {
  const slug: string = Deno.args[0] ?? "";
  if (slug === "") {
    console.error("usage: free-port <project-slug>");
    Deno.exit(1);
  }

  try {
    console.log(freePortForProject(slug));
  } catch (error: unknown) {
    console.error(error instanceof Error ? error.message : String(error));
    Deno.exit(1);
  }
}
