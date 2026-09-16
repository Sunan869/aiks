import { describe, expect, it } from "vitest";
import {
  boundWorkbenchMode,
  shouldKeepWorkbenchMounted,
} from "./workbench";

describe("V4.1 Session workbench routing", () => {
  it("opens a bound Session document in session mode", () => {
    expect(boundWorkbenchMode("session", "session-doc-1")).toBe("session");
    expect(boundWorkbenchMode("knowledge", "knowledge-doc-1")).toBe("knowledge");
    expect(boundWorkbenchMode("session", null)).toBeNull();
    expect(boundWorkbenchMode("graph", "ignored-doc")).toBeNull();
  });

  it("keeps the child workbench mounted on knowledge and Session detail routes", () => {
    expect(shouldKeepWorkbenchMounted("knowledge", false)).toBe(true);
    expect(shouldKeepWorkbenchMounted("sessions", true)).toBe(true);
    expect(shouldKeepWorkbenchMounted("sessions", false)).toBe(false);
    expect(shouldKeepWorkbenchMounted("processing", false)).toBe(false);
  });
});
