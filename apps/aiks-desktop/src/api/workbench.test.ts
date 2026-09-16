import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import { describe, expect, it } from "vitest";
import {
  BRIDGE_PROTOCOL_VERSION,
  WORKBENCH_ACTIONS,
  WORKSPACE_MODES,
  isWorkbenchAction,
  type WorkbenchActionName,
} from "./workbench";

const here = dirname(fileURLToPath(import.meta.url));
const pluginRoot = resolve(
  here,
  "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge",
);

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
    const manifest = JSON.parse(
      readFileSync(resolve(pluginRoot, "plugin.json"), "utf8"),
    ) as { name: string; version: string };
    const source = readFileSync(resolve(pluginRoot, "index.js"), "utf8");

    expect(manifest.name).toBe("aiks-bridge");
    expect(manifest.version).toMatch(/^1\./);
    expect(source).toContain("window.__AIKS_BRIDGE__");
    expect(source).toContain("protocolVersion: 1");
    expect(source).toContain("setReadOnly");
    expect(source).toContain("bridgeReady");
  });
});
