/**
 * Unit tests for MockLogger class.
 * @module
 */

import { assertEquals, assertThrows } from "@std/assert";
import { MockLogger, serializeErrorData } from "./logger.ts";

Deno.test("serializeErrorData() - primitive types - single string", () => {
  const expected: string = "test message";
  const actual: string = serializeErrorData("test message");

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - primitive types - single number", () => {
  const expected: string = "42";
  const actual: string = serializeErrorData(42);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - primitive types - zero", () => {
  const expected: string = "0";
  const actual: string = serializeErrorData(0);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - primitive types - negative number", () => {
  const expected: string = "-42";
  const actual: string = serializeErrorData(-42);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - primitive types - boolean true", () => {
  const expected: string = "true";
  const actual: string = serializeErrorData(true);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - primitive types - boolean false", () => {
  const expected: string = "false";
  const actual: string = serializeErrorData(false);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - primitive types - empty string", () => {
  const expected: string = "";
  const actual: string = serializeErrorData("");

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - null and undefined - single null", () => {
  const expected: string = "null";
  const actual: string = serializeErrorData(null);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - null and undefined - single undefined", () => {
  const expected: string = "undefined";
  const actual: string = serializeErrorData(undefined);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - null and undefined - multiple null values", () => {
  const expected: string = "null null null";
  const actual: string = serializeErrorData(null, null, null);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - null and undefined - mixed null and undefined", () => {
  const expected: string = "null undefined null";
  const actual: string = serializeErrorData(null, undefined, null);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - null and undefined - mixed with other types", () => {
  const expected: string = "null test undefined 42";
  const actual: string = serializeErrorData(null, "test", undefined, 42);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - objects - simple object", () => {
  const testObject: { key: string; value: number } = { key: "test", value: 123 };
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - objects - nested objects", () => {
  const testObject: { nested: { deep: { value: number } } } = {
    nested: { deep: { value: 42 } },
  };
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - objects - empty object", () => {
  const testObject: Record<string, never> = {};
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - objects - object with null property", () => {
  const testObject: { key: string | null } = { key: null };
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - objects - object with undefined property", () => {
  const testObject: { key?: string } = {};
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - objects - object with array property", () => {
  const testObject: { items: number[] } = { items: [1, 2, 3] };
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - arrays - simple array of primitives", () => {
  const testArray: number[] = [1, 2, 3];
  const expected: string = JSON.stringify(testArray, null, 2);
  const actual: string = serializeErrorData(testArray);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - arrays - array of objects", () => {
  const testArray: Array<{ id: number; name: string }> = [
    { id: 1, name: "test" },
    { id: 2, name: "test2" },
  ];
  const expected: string = JSON.stringify(testArray, null, 2);
  const actual: string = serializeErrorData(testArray);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - arrays - nested arrays", () => {
  const testArray: number[][] = [[1, 2], [3, 4]];
  const expected: string = JSON.stringify(testArray, null, 2);
  const actual: string = serializeErrorData(testArray);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - arrays - empty array", () => {
  const testArray: unknown[] = [];
  const expected: string = JSON.stringify(testArray, null, 2);
  const actual: string = serializeErrorData(testArray);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - arrays - array with null elements", () => {
  const testArray: (number | null)[] = [1, null, 3];
  const expected: string = JSON.stringify(testArray, null, 2);
  const actual: string = serializeErrorData(testArray);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - arrays - array with undefined elements", () => {
  const testArray: (number | undefined)[] = [1, undefined, 3];
  const expected: string = JSON.stringify(testArray, null, 2);
  const actual: string = serializeErrorData(testArray);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - variadic arguments - multiple strings", () => {
  const expected: string = "first second third";
  const actual: string = serializeErrorData("first", "second", "third");

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - variadic arguments - multiple numbers", () => {
  const expected: string = "1 2 3";
  const actual: string = serializeErrorData(1, 2, 3);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - variadic arguments - mixed types", () => {
  const testObject: { key: string } = { key: "value" };
  const testArray: number[] = [1, 2, 3];
  const expected: string = `test ${JSON.stringify(testObject, null, 2)} 42 ${JSON.stringify(testArray, null, 2)}`;
  const actual: string = serializeErrorData("test", testObject, 42, testArray);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - variadic arguments - no arguments", () => {
  const expected: string = "";
  const actual: string = serializeErrorData();

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - variadic arguments - many arguments", () => {
  const expected: string = "1 2 3 4 5 6 7 8 9 10";
  const actual: string = serializeErrorData(1, 2, 3, 4, 5, 6, 7, 8, 9, 10);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - edge cases - complex nested structure", () => {
  const complexObject: {
    nested: { deep: { value: number } };
    array: Array<{ id: number; name: string }>;
    mixed: Array<unknown>;
  } = {
    nested: { deep: { value: 42 } },
    array: [{ id: 1, name: "test" }, { id: 2, name: "test2" }],
    mixed: ["string", 123, null, { inner: "value" }],
  };
  const expected: string = JSON.stringify(complexObject, null, 2);
  const actual: string = serializeErrorData(complexObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - edge cases - object with all primitive types", () => {
  const testObject: {
    string: string;
    number: number;
    boolean: boolean;
    nullValue: null;
    array: unknown[];
    nested: { value: number };
  } = {
    string: "test",
    number: 42,
    boolean: true,
    nullValue: null,
    array: [1, 2, 3],
    nested: { value: 100 },
  };
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - edge cases - multiple objects separated by spaces", () => {
  const obj1: { a: number } = { a: 1 };
  const obj2: { b: number } = { b: 2 };
  const expected: string = `${JSON.stringify(obj1, null, 2)} ${JSON.stringify(obj2, null, 2)}`;
  const actual: string = serializeErrorData(obj1, obj2);

  assertEquals(actual, expected);
});

Deno.test("serializeErrorData() - edge cases - JSON formatting with 2-space indentation", () => {
  const testObject: { level1: { level2: { level3: string } } } = {
    level1: { level2: { level3: "value" } },
  };
  const expected: string = JSON.stringify(testObject, null, 2);
  const actual: string = serializeErrorData(testObject);

  assertEquals(actual, expected);
  // Verify it has proper indentation (contains newlines and spaces)
  assertEquals(actual.includes("\n"), true);
  assertEquals(actual.includes("  "), true);
});

Deno.test("MockLogger - constructor", () => {
  const logger: MockLogger = new MockLogger();

  const expectedDebugLogs: unknown[][] = [];
  const expectedWarnLogs: unknown[][] = [];
  const expectedInfoLogs: unknown[][] = [];
  const expectedErrorLogs: unknown[][] = [];

  const actualDebugLogs: unknown[][] = logger.getDebugLogs();
  const actualWarnLogs: unknown[][] = logger.getWarnLogs();
  const actualInfoLogs: unknown[][] = logger.getInfoLogs();
  const actualErrorLogs: unknown[][] = logger.getErrorLogs();

  assertEquals(actualDebugLogs, expectedDebugLogs);
  assertEquals(actualWarnLogs, expectedWarnLogs);
  assertEquals(actualInfoLogs, expectedInfoLogs);
  assertEquals(actualErrorLogs, expectedErrorLogs);
});

Deno.test("MockLogger - debug() - single string entry", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("test message");

  const expected: unknown[][] = [["test message"]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - debug() - single number entry", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug(42);

  const expected: unknown[][] = [[42]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - debug() - single object entry", () => {
  const logger: MockLogger = new MockLogger();

  const testObject: { key: string; value: number } = { key: "test", value: 123 };
  logger.debug(testObject);

  const expected: unknown[][] = [[testObject]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - debug() - single array entry", () => {
  const logger: MockLogger = new MockLogger();

  const testArray: number[] = [1, 2, 3];
  logger.debug(testArray);

  const expected: unknown[][] = [[testArray]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - debug() - null and undefined entries", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug(null);
  logger.debug(undefined);

  const expected: unknown[][] = [[null], [undefined]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - debug() - multiple arguments in single call", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("message", 42, { key: "value" });

  const expected: unknown[][] = [["message", 42, { key: "value" }]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - debug() - multiple calls accumulate", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("first");
  logger.debug("second");
  logger.debug("third");

  const expected: unknown[][] = [["first"], ["second"], ["third"]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - warn() - single string entry", () => {
  const logger: MockLogger = new MockLogger();

  logger.warn("warning message");

  const expected: unknown[][] = [["warning message"]];
  const actual: unknown[][] = logger.getWarnLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - warn() - multiple arguments in single call", () => {
  const logger: MockLogger = new MockLogger();

  logger.warn("warning", 100, { error: true });

  const expected: unknown[][] = [["warning", 100, { error: true }]];
  const actual: unknown[][] = logger.getWarnLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - warn() - multiple calls accumulate", () => {
  const logger: MockLogger = new MockLogger();

  logger.warn("first warning");
  logger.warn("second warning");

  const expected: unknown[][] = [["first warning"], ["second warning"]];
  const actual: unknown[][] = logger.getWarnLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - info() - single string entry", () => {
  const logger: MockLogger = new MockLogger();

  logger.info("info message");

  const expected: unknown[][] = [["info message"]];
  const actual: unknown[][] = logger.getInfoLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - info() - multiple arguments in single call", () => {
  const logger: MockLogger = new MockLogger();

  logger.info("info", 200, { status: "ok" });

  const expected: unknown[][] = [["info", 200, { status: "ok" }]];
  const actual: unknown[][] = logger.getInfoLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - info() - multiple calls accumulate", () => {
  const logger: MockLogger = new MockLogger();

  logger.info("first info");
  logger.info("second info");

  const expected: unknown[][] = [["first info"], ["second info"]];
  const actual: unknown[][] = logger.getInfoLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - error() - stores entry before throwing", () => {
  const logger: MockLogger = new MockLogger();

  try {
    logger.error("error message");
  } catch {
    // Expected to throw
  }

  const expected: unknown[][] = [["error message"]];
  const actual: unknown[][] = logger.getErrorLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - error() - throws Error with serialized message", () => {
  const logger: MockLogger = new MockLogger();

  const expectedMessage: string = serializeErrorData("error message");
  const error: Error = assertThrows(() => {
    logger.error("error message");
  }, Error) as Error;

  const actualMessage: string = error.message;

  assertEquals(actualMessage, expectedMessage);
});

Deno.test("MockLogger - error() - multiple arguments serialized correctly", () => {
  const logger: MockLogger = new MockLogger();

  const expectedMessage: string = serializeErrorData("error", 500, { code: "E500" });
  const error: Error = assertThrows(() => {
    logger.error("error", 500, { code: "E500" });
  }, Error) as Error;

  const actualMessage: string = error.message;

  assertEquals(actualMessage, expectedMessage);
});

Deno.test("MockLogger - error() - stores entry with multiple arguments", () => {
  const logger: MockLogger = new MockLogger();

  try {
    logger.error("error", 500, { code: "E500" });
  } catch {
    // Expected to throw
  }

  const expected: unknown[][] = [["error", 500, { code: "E500" }]];
  const actual: unknown[][] = logger.getErrorLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - error() - multiple calls store multiple entries", () => {
  const logger: MockLogger = new MockLogger();

  try {
    logger.error("first error");
  } catch {
    // Expected to throw
  }

  try {
    logger.error("second error");
  } catch {
    // Expected to throw
  }

  const expected: unknown[][] = [["first error"], ["second error"]];
  const actual: unknown[][] = logger.getErrorLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - getDebugLogs() - returns copy not reference", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("test");

  const firstCall: unknown[][] = logger.getDebugLogs();
  const secondCall: unknown[][] = logger.getDebugLogs();

  // Modify the returned array
  firstCall.push(["modified"]);

  // Second call should not be affected
  const expected: unknown[][] = [["test"]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
  assertEquals(secondCall, expected);
});

Deno.test("MockLogger - getWarnLogs() - returns copy not reference", () => {
  const logger: MockLogger = new MockLogger();

  logger.warn("warning");

  const firstCall: unknown[][] = logger.getWarnLogs();
  const secondCall: unknown[][] = logger.getWarnLogs();

  // Modify the returned array
  firstCall.push(["modified"]);

  // Second call should not be affected
  const expected: unknown[][] = [["warning"]];
  const actual: unknown[][] = logger.getWarnLogs();

  assertEquals(actual, expected);
  assertEquals(secondCall, expected);
});

Deno.test("MockLogger - getInfoLogs() - returns copy not reference", () => {
  const logger: MockLogger = new MockLogger();

  logger.info("info");

  const firstCall: unknown[][] = logger.getInfoLogs();
  const secondCall: unknown[][] = logger.getInfoLogs();

  // Modify the returned array
  firstCall.push(["modified"]);

  // Second call should not be affected
  const expected: unknown[][] = [["info"]];
  const actual: unknown[][] = logger.getInfoLogs();

  assertEquals(actual, expected);
  assertEquals(secondCall, expected);
});

Deno.test("MockLogger - getErrorLogs() - returns copy not reference", () => {
  const logger: MockLogger = new MockLogger();

  try {
    logger.error("error");
  } catch {
    // Expected to throw
  }

  const firstCall: unknown[][] = logger.getErrorLogs();
  const secondCall: unknown[][] = logger.getErrorLogs();

  // Modify the returned array
  firstCall.push(["modified"]);

  // Second call should not be affected
  const expected: unknown[][] = [["error"]];
  const actual: unknown[][] = logger.getErrorLogs();

  assertEquals(actual, expected);
  assertEquals(secondCall, expected);
});

Deno.test("MockLogger - getDebugLogs() - returns empty array when no logs", () => {
  const logger: MockLogger = new MockLogger();

  const expected: unknown[][] = [];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - getWarnLogs() - returns empty array when no logs", () => {
  const logger: MockLogger = new MockLogger();

  const expected: unknown[][] = [];
  const actual: unknown[][] = logger.getWarnLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - getInfoLogs() - returns empty array when no logs", () => {
  const logger: MockLogger = new MockLogger();

  const expected: unknown[][] = [];
  const actual: unknown[][] = logger.getInfoLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - getErrorLogs() - returns empty array when no logs", () => {
  const logger: MockLogger = new MockLogger();

  const expected: unknown[][] = [];
  const actual: unknown[][] = logger.getErrorLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - clear() - removes all log entries", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("debug");
  logger.warn("warn");
  logger.info("info");
  try {
    logger.error("error");
  } catch {
    // Expected to throw
  }

  logger.clear();

  const expectedDebug: unknown[][] = [];
  const expectedWarn: unknown[][] = [];
  const expectedInfo: unknown[][] = [];
  const expectedError: unknown[][] = [];

  const actualDebug: unknown[][] = logger.getDebugLogs();
  const actualWarn: unknown[][] = logger.getWarnLogs();
  const actualInfo: unknown[][] = logger.getInfoLogs();
  const actualError: unknown[][] = logger.getErrorLogs();

  assertEquals(actualDebug, expectedDebug);
  assertEquals(actualWarn, expectedWarn);
  assertEquals(actualInfo, expectedInfo);
  assertEquals(actualError, expectedError);
});

Deno.test("MockLogger - clear() - can log again after clearing", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("first");
  logger.clear();
  logger.debug("second");

  const expected: unknown[][] = [["second"]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
});

Deno.test("MockLogger - integration - mix of all log levels", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("debug message");
  logger.warn("warn message");
  logger.info("info message");
  try {
    logger.error("error message");
  } catch {
    // Expected to throw
  }

  const expectedDebug: unknown[][] = [["debug message"]];
  const expectedWarn: unknown[][] = [["warn message"]];
  const expectedInfo: unknown[][] = [["info message"]];
  const expectedError: unknown[][] = [["error message"]];

  const actualDebug: unknown[][] = logger.getDebugLogs();
  const actualWarn: unknown[][] = logger.getWarnLogs();
  const actualInfo: unknown[][] = logger.getInfoLogs();
  const actualError: unknown[][] = logger.getErrorLogs();

  assertEquals(actualDebug, expectedDebug);
  assertEquals(actualWarn, expectedWarn);
  assertEquals(actualInfo, expectedInfo);
  assertEquals(actualError, expectedError);
});

Deno.test("MockLogger - integration - isolation between log levels", () => {
  const logger: MockLogger = new MockLogger();

  logger.debug("debug only");
  logger.warn("warn only");
  logger.info("info only");

  const expectedDebug: unknown[][] = [["debug only"]];
  const expectedWarn: unknown[][] = [["warn only"]];
  const expectedInfo: unknown[][] = [["info only"]];
  const expectedError: unknown[][] = [];

  const actualDebug: unknown[][] = logger.getDebugLogs();
  const actualWarn: unknown[][] = logger.getWarnLogs();
  const actualInfo: unknown[][] = logger.getInfoLogs();
  const actualError: unknown[][] = logger.getErrorLogs();

  assertEquals(actualDebug, expectedDebug);
  assertEquals(actualWarn, expectedWarn);
  assertEquals(actualInfo, expectedInfo);
  assertEquals(actualError, expectedError);
});

Deno.test("MockLogger - integration - complex nested objects", () => {
  const logger: MockLogger = new MockLogger();

  const complexObject: {
    nested: { deep: { value: number } };
    array: Array<{ id: number; name: string }>;
  } = {
    nested: { deep: { value: 42 } },
    array: [{ id: 1, name: "test" }, { id: 2, name: "test2" }],
  };

  logger.debug(complexObject);

  const expected: unknown[][] = [[complexObject]];
  const actual: unknown[][] = logger.getDebugLogs();

  assertEquals(actual, expected);
  assertEquals(actual[0][0], complexObject);
});

Deno.test("MockLogger - integration - arrays with mixed types", () => {
  const logger: MockLogger = new MockLogger();

  const mixedArray: unknown[] = ["string", 42, { key: "value" }, null, undefined, [1, 2, 3]];

  logger.info(mixedArray);

  const expected: unknown[][] = [[mixedArray]];
  const actual: unknown[][] = logger.getInfoLogs();

  assertEquals(actual, expected);
  assertEquals(actual[0][0], mixedArray);
});
