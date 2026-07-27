/**
 * Unit tests for the free-port module.
 * @module
 */

import { assertEquals, assertThrows } from "@std/assert";
import { freePortForProject, isPortFree, portForProject } from "./free-port.ts";

Deno.test("portForProject() - determinism - same slug yields same port", () => {
  assertEquals(portForProject("personal-website"), portForProject("personal-website"));
});

Deno.test("portForProject() - determinism - known slugs are pinned", () => {
  // These are committed expectations: changing the hash silently would move every project's
  // port, breaking bookmarks and documented URLs across every repo that depends on this.
  assertEquals(portForProject("personal-website"), 8749);
  assertEquals(portForProject("template-web-server"), 8360);
  assertEquals(portForProject("eat-out"), 8348);
});

Deno.test("portForProject() - distribution - distinct slugs yield distinct ports", () => {
  const slugs: string[] = [
    "personal-website",
    "eat-out",
    "palms-small-engine",
    "triple-entendre-website",
    "vogue-bot",
    "template-web-server",
    "sound-board-bot",
    "boggledygook",
    "oasis",
    "cross-stitch-pattern-generator",
  ];
  const ports: Set<number> = new Set(slugs.map(portForProject));

  assertEquals(ports.size, slugs.length);
});

Deno.test("portForProject() - range - stays within the reserved window", () => {
  for (let i = 0; i < 1000; i++) {
    const port: number = portForProject(`slug-${i}`);

    assertEquals(port >= 8080 && port < 9080, true);
  }
});

Deno.test("isPortFree() - detects a wildcard listener", () => {
  const port: number = portForProject("free-port-test-wildcard");
  const listener: Deno.Listener = Deno.listen({ hostname: "0.0.0.0", port, transport: "tcp" });

  try {
    assertEquals(isPortFree(port), false);
  } finally {
    listener.close();
  }

  assertEquals(isPortFree(port), true);
});

Deno.test("isPortFree() - detects a loopback-only listener", () => {
  // A wildcard bind does not conflict with a loopback bind on macOS, so probing only
  // 0.0.0.0 would report this port as free. Both probes are required.
  const port: number = portForProject("free-port-test-loopback");
  const listener: Deno.Listener = Deno.listen({ hostname: "127.0.0.1", port, transport: "tcp" });

  try {
    assertEquals(isPortFree(port), false);
  } finally {
    listener.close();
  }

  assertEquals(isPortFree(port), true);
});

Deno.test("freePortForProject() - returns the derived port when free", () => {
  const slug = "free-port-test-available";

  assertEquals(freePortForProject(slug), portForProject(slug));
});

Deno.test("freePortForProject() - throws when the port is held", () => {
  const slug = "free-port-test-held";
  const listener: Deno.Listener = Deno.listen({ hostname: "0.0.0.0", port: portForProject(slug), transport: "tcp" });

  try {
    assertThrows(() => freePortForProject(slug), Error, "is unavailable");
  } finally {
    listener.close();
  }
});
