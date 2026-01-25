/**
 * Logger interface and implementations for logging messages.
 * @module
 */

/**
 * Serializes error data into a string.
 * This function provides consistent error serialization across all logger implementations.
 *
 * @param data Variadic arguments to serialize into the error message.
 * @returns A string containing the serialized error data.
 */
export function serializeErrorData(...data: unknown[]): string {
  const serializedData: string[] = data.map((item: unknown) =>
    typeof item === "object" && item !== null ? JSON.stringify(item, null, 2) : String(item)
  );
  return serializedData.join(" ");
}

/**
 * Interface defining the contract for logger implementations.
 */
export interface ILogger {
  /**
   * Logs a DEBUG-level message.
   * @param data Variadic arguments to log.
   */
  debug(...data: unknown[]): void;

  /**
   * Logs a WARN-level message.
   * @param data Variadic arguments to log.
   */
  warn(...data: unknown[]): void;

  /**
   * Logs an INFO-level message.
   * @param data Variadic arguments to log.
   */
  info(...data: unknown[]): void;

  /**
   * Logs an ERROR-level message and throws an Error.
   * @param data Variadic arguments to log.
   * @throws {Error} Always throws an Error with the serialized log data.
   */
  error(...data: unknown[]): never;
}

/**
 * The Logger class provides methods to log messages at different levels (DEBUG, WARN, INFO, ERROR).
 */
export default class Logger implements ILogger {
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
   * @param data Variadic arguments to log.
   */
  public debug(...data: unknown[]): void {
    if (!this._debug) {
      return;
    }

    console.debug(...data);
  }

  /**
   * Logs a WARN-level message to the browser console.
   * @param data Variadic arguments to log.
   */
  public warn(...data: unknown[]): void {
    if (!this._debug) {
      return;
    }

    console.warn(...data);
  }

  /**
   * Logs an INFO-level message to the browser console.
   * @param data Variadic arguments to log.
   */
  public info(...data: unknown[]): void {
    console.info(...data);
  }

  /**
   * Logs an ERROR-level message to the browser console.
   * @param data Variadic arguments to log.
   */
  public error(...data: unknown[]): never {
    throw Error(serializeErrorData(...data));
  }
}

/**
 * A no-op logger implementation that does nothing.
 * All methods except error() return immediately without any action.
 * The error() method still throws an Error as per the ILogger contract.
 */
export class NoOpLogger implements ILogger {
  /**
   * Creates an instance of the NoOpLogger class.
   */
  constructor() {
    // No initialization needed
  }

  /**
   * No-op: does nothing.
   * @param data Variadic arguments (ignored).
   */
  public debug(..._data: unknown[]): void {
    // Do nothing
  }

  /**
   * No-op: does nothing.
   * @param data Variadic arguments (ignored).
   */
  public warn(..._data: unknown[]): void {
    // Do nothing
  }

  /**
   * No-op: does nothing.
   * @param data Variadic arguments (ignored).
   */
  public info(..._data: unknown[]): void {
    // Do nothing
  }

  /**
   * Throws an Error without logging anything.
   * @param data Variadic arguments to serialize into the error message.
   * @throws {Error} Always throws an Error with the serialized log data.
   */
  public error(...data: unknown[]): never {
    throw Error(serializeErrorData(...data));
  }
}

/**
 * A mock logger implementation for unit testing.
 * Stores all log entries by level for deterministic verification.
 */
export class MockLogger implements ILogger {
  private readonly _debugLogs: unknown[][] = [];
  private readonly _warnLogs: unknown[][] = [];
  private readonly _infoLogs: unknown[][] = [];
  private readonly _errorLogs: unknown[][] = [];

  /**
   * Creates an instance of the MockLogger class.
   */
  constructor() {
    // No initialization needed
  }

  /**
   * Stores a DEBUG-level log entry.
   * @param data Variadic arguments to store.
   */
  public debug(...data: unknown[]): void {
    this._debugLogs.push([...data]);
  }

  /**
   * Stores a WARN-level log entry.
   * @param data Variadic arguments to store.
   */
  public warn(...data: unknown[]): void {
    this._warnLogs.push([...data]);
  }

  /**
   * Stores an INFO-level log entry.
   * @param data Variadic arguments to store.
   */
  public info(...data: unknown[]): void {
    this._infoLogs.push([...data]);
  }

  /**
   * Stores an ERROR-level log entry and throws an Error.
   * @param data Variadic arguments to store and serialize into the error message.
   * @throws {Error} Always throws an Error with the serialized log data.
   */
  public error(...data: unknown[]): never {
    this._errorLogs.push([...data]);
    throw Error(serializeErrorData(...data));
  }

  /**
   * Gets all stored DEBUG-level log entries.
   * @returns Array of log entries, where each entry is an array of the original arguments.
   */
  public getDebugLogs(): unknown[][] {
    return [...this._debugLogs];
  }

  /**
   * Gets all stored WARN-level log entries.
   * @returns Array of log entries, where each entry is an array of the original arguments.
   */
  public getWarnLogs(): unknown[][] {
    return [...this._warnLogs];
  }

  /**
   * Gets all stored INFO-level log entries.
   * @returns Array of log entries, where each entry is an array of the original arguments.
   */
  public getInfoLogs(): unknown[][] {
    return [...this._infoLogs];
  }

  /**
   * Gets all stored ERROR-level log entries.
   * @returns Array of log entries, where each entry is an array of the original arguments.
   */
  public getErrorLogs(): unknown[][] {
    return [...this._errorLogs];
  }

  /**
   * Clears all stored log entries.
   */
  public clear(): void {
    this._debugLogs.length = 0;
    this._warnLogs.length = 0;
    this._infoLogs.length = 0;
    this._errorLogs.length = 0;
  }
}
