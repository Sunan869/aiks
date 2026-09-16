import { describe, expect, it } from "vitest";
import {
  WORKBENCH_ACTIONS,
  WORKSPACE_MODES,
  isWorkbenchAction,
  type WorkbenchActionName,
} from "./workbench";

describe("V4.1 workbench bridge contract", () => {
  it("keeps the version-one workspace modes stable", () => {
    expect(WORKSPACE_MODES).toEqual(["knowledge", "session"]);
  });

  it("contains the supported workbench actions", () => {
    expect(WORKBENCH_ACTIONS).toContain("showKnowledgeRoot");
    expect(WORKBENCH_ACTIONS).toContain("showSessionRoot");
    expect(WORKBENCH_ACTIONS).toContain("openDocument");
    expect(WORKBENCH_ACTIONS).toContain("openBlock");
    expect(WORKBENCH_ACTIONS).toContain("setWorkspaceMode");
    expect(WORKBENCH_ACTIONS).toContain("showBacklinks");
    expect(WORKBENCH_ACTIONS).toContain("showOutline");
    expect(WORKBENCH_ACTIONS).toContain("showDatabase");
    expect(WORKBENCH_ACTIONS).toContain("showGraph");
    expect(WORKBENCH_ACTIONS).toContain("showSearch");
    expect(WORKBENCH_ACTIONS).toContain("refreshDocument");
  });

  it("rejects arbitrary bridge actions", () => {
    expect(isWorkbenchAction("openDocument")).toBe(true);
    expect(isWorkbenchAction("runShellCommand")).toBe(false);
  });

  it("derives the action-name type from the allowlist", () => {
    const action: WorkbenchActionName = "openBlock";
    expect(action).toBe("openBlock");
  });
});
