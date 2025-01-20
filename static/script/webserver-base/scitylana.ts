/**
 * Publishes a highly-opinionated custom analytics event to the server.
 */
async function scitylana(e: Event): Promise<void> {
  e.preventDefault();

  const response: Response = await fetch("/api/v1/scitylana", {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded;charset=UTF-8",
    },
    body: new URLSearchParams({
      "user_agent": navigator.userAgent,
      "url": globalThis.location.href,
      "referrer": document.referrer,
      "screen_width": globalThis.innerWidth.toString(),
    }),
  });
  if (!response.ok) {
    console.error("Failed scitylana: ", response);
  }
}

/**
 * Configures the analytics event to be sent on page load.
 */
export function initScitylana(): void {
  document.addEventListener("DOMContentLoaded", scitylana);
}
