import { describe, expect, it } from "vitest";
import pluginSource from "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js?raw";
import { TauriAiksApi } from "./tauri";
import { WORKBENCH_ACTIONS } from "./workbench";

const LOCAL_PRESENTATION_ACTIONS = [
  "showSearch",
  "showGraph",
  "showOutline",
  "showBacklinks",
  "showDatabase",
] as const;

describe("V4.2 cross-system bridge boundary", () => {
  it("keeps pure SiYuan presentation actions out of the cross-system allowlist", () => {
    for (const action of LOCAL_PRESENTATION_ACTIONS) {
      expect(WORKBENCH_ACTIONS).not.toContain(action);
    }
  });

  it("supports AI Assist request and structured result across the bridge", () => {
    expect(WORKBENCH_ACTIONS).toContain("aiAssistResult");
    expect(pluginSource).toContain("requestAiAssist");
    expect(pluginSource).toContain("aiAssistResult");
  });

  it("does not expose native presentation forwarding methods on the AIKS API", () => {
    const api = new TauriAiksApi() as unknown as Record<string, unknown>;
    expect(api.showWorkbenchSearch).toBeUndefined();
    expect(api.showWorkbenchDatabase).toBeUndefined();
    expect(api.showWorkbenchGraph).toBeUndefined();
  });
});
