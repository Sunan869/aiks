import { describe, expect, it } from "vitest";
import pluginManifest from "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge/plugin.json";
import pluginSource from "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js?raw";
import {
  BRIDGE_PROTOCOL_VERSION,
  WORKBENCH_ACTIONS,
  WORKSPACE_MODES,
  isWorkbenchAction,
  type WorkbenchActionName,
} from "./workbench";

describe("V4.1 workbench bridge contract", () => {
  it("keeps the version-one workspace modes stable", () => {
    expect(BRIDGE_PROTOCOL_VERSION).toBe(1);
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

  it("ships a SiYuan plugin resource that exposes protocol v1", () => {
    expect(pluginManifest.name).toBe("aiks-bridge");
    expect(pluginManifest.version).toMatch(/^1\./);
    expect(pluginSource).toContain("window.__AIKS_BRIDGE__");
    expect(pluginSource).toContain("protocolVersion: 1");
    expect(pluginSource).toContain("setReadOnly");
    expect(pluginSource).toContain("bridgeReady");
  });
});
