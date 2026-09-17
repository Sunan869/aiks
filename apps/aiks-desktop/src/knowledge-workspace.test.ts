import { describe, expect, it } from "vitest";
import {
  WORKBENCH_MAIN_MODES,
  resolveWorkbenchSurface,
} from "./knowledge-workspace";

describe("V4.2 Knowledge Workbench modes", () => {
  it("uses only Document Database and Graph as center modes", () => {
    expect(WORKBENCH_MAIN_MODES).toEqual(["document", "database", "graph"]);
  });

  it("keeps raw conversations in document mode and maps native main surfaces", () => {
    expect(resolveWorkbenchSurface("document", "knowledge")).toBe("knowledge");
    expect(resolveWorkbenchSurface("document", "session")).toBe("session");
    expect(resolveWorkbenchSurface("database", "session")).toBe("database");
    expect(resolveWorkbenchSurface("graph", "knowledge")).toBe("graph");
  });
});
