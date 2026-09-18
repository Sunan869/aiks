import { describe, expect, it } from "vitest";
import pluginSource from "../../src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js?raw";
import eventsSource from "../../src-tauri/src/workbench/events.rs?raw";
import protocolSource from "../../src-tauri/src/workbench/protocol.rs?raw";
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

  it("supports AI Assist request and structured result across the full bridge", () => {
    expect(WORKBENCH_ACTIONS).toContain("aiAssistResult");
    expect(pluginSource).toContain("requestAiAssist");
    expect(pluginSource).toContain("aiAssistResult");
    expect(eventsSource).toContain('"requestAiAssist"');
    expect(protocolSource).toContain("AiAssistResult");
  });

  it("handles AI Assist in AIKS from canonical SiYuan content instead of window message body", () => {
    expect(eventsSource).toContain("AiAssistService");
    expect(eventsSource).toContain("get_document_markdown");
    expect(eventsSource).toContain("WorkbenchAction::AiAssistResult");
    expect(pluginSource).toContain('this.emit("requestAiAssist", {requestId, docId: id, operation: op})');
  });

  it("does not expose native presentation forwarding methods on the AIKS API", () => {
    const api = new TauriAiksApi() as unknown as Record<string, unknown>;
    expect(api.showWorkbenchSearch).toBeUndefined();
    expect(api.showWorkbenchDatabase).toBeUndefined();
    expect(api.showWorkbenchGraph).toBeUndefined();
  });
});
