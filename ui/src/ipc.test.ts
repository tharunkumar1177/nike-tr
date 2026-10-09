import { describe, expect, it } from "vitest";
import { describeIpcError } from "./ipc";

const fallback = "The desktop core returned an error without a message.";

describe("describeIpcError", () => {
  it("passes through string rejections from the core", () => {
    expect(describeIpcError("project file is locked")).toBe(
      "project file is locked",
    );
  });

  it("reads the message of Error instances", () => {
    expect(describeIpcError(new Error("connection lost"))).toBe(
      "connection lost",
    );
  });

  it("reads the message of serialized error objects", () => {
    expect(describeIpcError({ kind: "Io", message: "disk full" })).toBe(
      "disk full",
    );
  });

  it("falls back when no message is available", () => {
    expect(describeIpcError(undefined)).toBe(fallback);
    expect(describeIpcError("")).toBe(fallback);
    expect(describeIpcError({ message: 42 })).toBe(fallback);
  });
});
