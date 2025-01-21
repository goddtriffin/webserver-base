/**
 * This module provides functions to report frontend errors to the server.
 * @module
 */

/**
 * The payload of a frontend error report.
 */
type FrontendErrorPayload = {
  sourceFile: string | null;
  lineNumber: number | null;
  columnNumber: number | null;

  message: string | null;
  stackTrace: string | null;
  currentUrl: string | null;
  timestamp: string | null;
};

/**
 * Reports a frontend error to the server.
 */
async function reportError(frontend_error_payload: FrontendErrorPayload): Promise<void> {
  const response: Response = await fetch("/api/v1/frontend-error", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(frontend_error_payload),
  });
  if (!response.ok) {
    console.error("Failed to send frontend error report: ", response);
  }
}

/**
 * Initializes global error handlers to report frontend errors to the server.
 */
export function initGlobalErrorHandlers(): void {
  // handle synchronous errors
  globalThis.onerror = (
    message: string | Event,
    source: string | undefined,
    lineno: number | undefined,
    colno: number | undefined,
    error: Error | undefined,
  ) => {
    const errorReport: FrontendErrorPayload = {
      sourceFile: source || null,
      lineNumber: lineno || null,
      columnNumber: colno || null,

      message: String(message),
      stackTrace: error?.stack || null,
      currentUrl: globalThis.location.href,
      timestamp: new Date().toString(),
    };
    reportError(errorReport);
    return false;
  };

  // handle Promise rejections
  globalThis.addEventListener("unhandledrejection", (event: PromiseRejectionEvent) => {
    const error = event.reason;
    const errorReport: FrontendErrorPayload = {
      sourceFile: null, // not available in Promise rejections
      lineNumber: null,
      columnNumber: null,

      message: error instanceof Error ? error.message : String(error),
      stackTrace: error instanceof Error ? error.stack || null : null,
      currentUrl: globalThis.location.href,
      timestamp: new Date().toString(),
    };
    reportError(errorReport);
  });
}
