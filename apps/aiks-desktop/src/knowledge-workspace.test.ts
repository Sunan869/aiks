import { describe, expect, it } from "vitest";
import {
  WORKBENCH_MAIN_MODES,
  resolveWorkbenchSurface,
} from "./knowledge-workspace";

describe("V4.2 Knowledge Workbench modes", () => {
  it("keeps only the cross-system document surface in AIKS", () => {
    expect(WORKBENCH_MAIN_MODES).toEqual(["document"]);
  });

  it("maps document mode to the active knowledge or session workspace", () => {
    expect(resolveWorkbenchSurface("document", "knowledge")).toBe("knowledge");
    expect(resolveWorkbenchSurface("document", "session")).toBe("session");
  });
});
