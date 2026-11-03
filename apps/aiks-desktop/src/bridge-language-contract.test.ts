import { describe, expect, it } from "vitest";
import bridge from "../src-tauri/resources/siyuan/data/plugins/aiks-bridge/index.js?raw";
import workspace from "./pages/KnowledgeWorkspacePage.tsx?raw";
describe("embedded SiYuan locale boundary", () => {
  it("bundled Bridge is syntactically valid and supports native zh/en locale", () => {
    expect(() => new Function(bridge)).not.toThrow();
    expect(bridge).toContain("window.siyuan?.config?.lang");
    expect(bridge).toContain("Ask AIKS");
    expect(bridge).toContain("bridgeText");
    expect(bridge).toContain("refreshLocalizedUi");
    expect(bridge).toContain("clearInterval(this.bridgeLocaleTimer)");
    expect(bridge).not.toContain('button.title = "基于 AIKS');
  });
  it("uses the single AIKS language setting instead of an inaccessible child-webview dropdown", () => {
    expect(workspace).not.toContain("RadixSelectField");
    expect(workspace).not.toContain("set_embedded_siyuan_language");
    expect(workspace).toContain("<WorkbenchHost");
  });
});
