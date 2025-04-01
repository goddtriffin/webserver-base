/**
 * Logger class for logging messages to the browser console.
 * @module
 */

/**
 * The Logger class provides methods to log messages at different levels (DEBUG, WARN, INFO, ERROR).
 */
export default class Logger {
  private readonly _debug: boolean;

  /**
   * Creates an instance of the Logger class.
   *
   * @param debug If true, will log the debug/warning messages. If false, won't log them.
   */
  constructor(debug: boolean) {
    this._debug = debug;
  }

  /**
   * Logs a DEBUG-level message to the browser console.
   * @param message The debug message to log.
   */
  public debug(...data: unknown[]): void {
    if (!this._debug) {
      return;
    }

    console.debug(...data);
  }

  /**
   * Logs a WARN-level message to the browser console.
   * @param message The warning message to log.
   */
  public warn(...data: unknown[]): void {
    if (!this._debug) {
      return;
    }

    console.warn(...data);
  }

  /**
   * Logs an INFO-level message to the browser console.
   * @param message The info message to log.
   */
  public info(...data: unknown[]): void {
    console.info(...data);
  }

  /**
   * Logs an ERROR-level message to the browser console.
   * @param message The error message to log.
   */
  public error(...data: unknown[]): never {
    const serializedData: string[] = data.map((item: unknown) =>
      typeof item === "object" && item !== null ? JSON.stringify(item, null, 2) : String(item)
    );
    throw Error(serializedData.join(" "));
  }
}
