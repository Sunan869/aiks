import { describe, expect, it } from "vitest";
import {
  boundWorkbenchMode,
  shouldKeepWorkbenchMounted,
} from "./workbench";

describe("V4.2 Session workbench routing", () => {
  it("opens bound documents only in knowledge or session workspace modes", () => {
    expect(boundWorkbenchMode("session", "session-doc-1")).toBe("session");
    expect(boundWorkbenchMode("knowledge", "knowledge-doc-1")).toBe("knowledge");
    expect(boundWorkbenchMode("session", null)).toBeNull();
  });

  it("keeps the child workbench mounted only on the Knowledge route", () => {
    expect(shouldKeepWorkbenchMounted("knowledge", false)).toBe(true);
    expect(shouldKeepWorkbenchMounted("sessions", true)).toBe(false);
    expect(shouldKeepWorkbenchMounted("sessions", false)).toBe(false);
    expect(shouldKeepWorkbenchMounted("processing", false)).toBe(false);
  });
});
