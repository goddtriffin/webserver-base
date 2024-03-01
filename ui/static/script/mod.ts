/**
 * A Typescript library which contains shared logic for all of my webserver projects.
 * @module
 */

/**
 * Publishes a highly-opinionated custom analytics event to the server.
 *
 * @example How to use:
 * ```ts
 * import { scitylana } from "@todd/webserver-base";
 *
 * document.addEventListener("DOMContentLoaded", scitylana);
 * ```
 */
export async function scitylana(e: Event): Promise<void> {
  e.preventDefault();

  const response: Response = await fetch("/api/v1/scitylana", {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8",
    },
    body: new URLSearchParams({
      "user_agent": navigator.userAgent,
      "url": window.location.href,
      "referrer": document.referrer,
      "screen_width": window.innerWidth.toString(),
    }),
  });
  if (!response.ok) {
    console.error("Failed scitylana: ", response);
  }
}
