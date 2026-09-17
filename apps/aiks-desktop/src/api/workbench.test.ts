import { invoke } from "@tauri-apps/api/core";
import { beforeEach, describe, expect, it, vi } from "vitest";
import pluginManifest from "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge/plugin.json";
import pluginSource from "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js?raw";
import { TauriAiksApi } from "./tauri";
import {
  BRIDGE_PROTOCOL_VERSION,
  WORKBENCH_ACTIONS,
  WORKSPACE_MODES,
  isWorkbenchAction,
  type WorkbenchActionName,
} from "./workbench";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(),
}));

const invokeMock = vi.mocked(invoke);

describe("V4.2 workbench bridge contract", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it("keeps the version-one workspace modes stable", () => {
    expect(BRIDGE_PROTOCOL_VERSION).toBe(1);
    expect(WORKSPACE_MODES).toEqual(["knowledge", "session"]);
  });

  it("contains only cross-system workbench actions", () => {
    expect(WORKBENCH_ACTIONS).toEqual([
      "showKnowledgeRoot",
      "showSessionRoot",
      "openDocument",
      "openBlock",
      "setWorkspaceMode",
      "refreshDocument",
      "aiAssistResult",
    ]);
  });

  it("rejects arbitrary bridge actions and local presentation actions", () => {
    expect(isWorkbenchAction("openDocument")).toBe(true);
    expect(isWorkbenchAction("aiAssistResult")).toBe(true);
    expect(isWorkbenchAction("showSearch")).toBe(false);
    expect(isWorkbenchAction("showGraph")).toBe(false);
    expect(isWorkbenchAction("runShellCommand")).toBe(false);
  });

  it("derives the action-name type from the allowlist", () => {
    const action: WorkbenchActionName = "aiAssistResult";
    expect(action).toBe("aiAssistResult");
  });

  it("ships a SiYuan plugin resource that exposes protocol v1 and AI Assist", () => {
    expect(pluginManifest.name).toBe("aiks-bridge");
    expect(pluginManifest.version).toMatch(/^1\./);
    expect(pluginSource).toContain("window.__AIKS_BRIDGE__");
    expect(pluginSource).toContain("protocolVersion: 1");
    expect(pluginSource).toContain("setReadOnly");
    expect(pluginSource).toContain("bridgeReady");
    expect(pluginSource).toContain("requestAiAssist");
    expect(pluginSource).toContain("aiAssistResult");
  });
});

describe("V4.2 Tauri workbench API mapping", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it("reads workbench status from the dedicated command", async () => {
    const expected = {
      available: true,
      ready: false,
      mode: "knowledge" as const,
      origin: "http://127.0.0.1:6812/",
      protocol_version: 1,
    };
    invokeMock.mockResolvedValueOnce(expected);

    const result = await new TauriAiksApi().getWorkbenchStatus();

    expect(result).toEqual(expected);
    expect(result.protocol_version).toBe(1);
    expect(invokeMock).toHaveBeenCalledWith("get_workbench_status");
  });

  it("reads aggregate V4.1 diagnostics from one command", async () => {
    const expected = {
      siyuan_ready: true,
      workbench: {
        available: true,
        ready: true,
        mode: "session" as const,
        origin: "http://127.0.0.1:6812/",
        protocol_version: 1,
      },
      migration: {
        total: 7,
        pending: 2,
        migrated: 2,
        reused: 1,
        conflicts: 1,
        failed: 1,
      },
    };
    invokeMock.mockResolvedValueOnce(expected);

    const result = await new TauriAiksApi().getV41Diagnostics();

    expect(result).toEqual(expected);
    expect(invokeMock).toHaveBeenCalledWith("get_v41_diagnostics");
  });

  it("mounts the persistent child webview into the host rectangle", async () => {
    await new TauriAiksApi().mountWorkbench({ x: 248, y: 156, width: 960, height: 620 });

    expect(invokeMock).toHaveBeenCalledWith("mount_workbench", {
      x: 248,
      y: 156,
      width: 960,
      height: 620,
    });
  });

  it("shows the workbench and selects the requested root", async () => {
    await new TauriAiksApi().showWorkbench("session");

    expect(invokeMock.mock.calls).toEqual([
      ["show_workbench"],
      ["show_workbench_root", { mode: "session" }],
    ]);
  });

  it("hides the persistent workbench", async () => {
    await new TauriAiksApi().hideWorkbench();

    expect(invokeMock).toHaveBeenCalledWith("hide_workbench");
  });

  it("opens a SiYuan document with camelCase payload after setting mode", async () => {
    await new TauriAiksApi().openSiyuanDocument("doc-123", "knowledge");

    expect(invokeMock.mock.calls).toEqual([
      ["show_workbench"],
      ["set_workbench_mode", { mode: "knowledge" }],
      ["open_siyuan_document", { docId: "doc-123" }],
    ]);
  });

  it("opens a SiYuan block with camelCase document and block IDs", async () => {
    await new TauriAiksApi().openSiyuanBlock("doc-123", "block-456", "session");

    expect(invokeMock.mock.calls).toEqual([
      ["show_workbench"],
      ["set_workbench_mode", { mode: "session" }],
      ["open_siyuan_block", { docId: "doc-123", blockId: "block-456" }],
    ]);
  });
});
