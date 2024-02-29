document.addEventListener("DOMContentLoaded", analytics);

async function analytics(e: Event) {
  e.preventDefault();

  const response = await fetch("/api/v1/scitylana", {
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
    console.log("Failed analytics.");
  }
}
