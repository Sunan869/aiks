import { describe, expect, it } from "vitest";
import settings from "../pages/SettingsPage.tsx?raw";
import editor from "../components/KnowledgeEditor.tsx?raw";
import host from "../components/WorkbenchHost.tsx?raw";
import restore from "../components/OfflineRestore.tsx?raw";

describe("desktop UI readiness guardrails", () => {
  it("keeps accessible compact switches and primary save", () => {
    expect(settings).toContain('role="switch"');
    expect(settings).toContain("aiks-switch");
    expect(settings).toContain('variant="primary" size="md"');
  });
  it("keeps knowledge editor save and preview controls", () => {
    expect(editor).toContain("aria-pressed={!preview}");
    expect(editor).toContain("aria-pressed={preview}");
    expect(editor).toContain("updateKnowledge(initial.id");
  });
  it("keeps embedded workbench retry", () => {
    expect(host).toContain("mountWorkbench(bounds)");
    expect(host).toContain("重新打开工作台");
  });
  it("keeps explicit backup confirmation and isolated restore barriers", () => {
    expect(restore).toContain("canConfirmIsolatedRestore");
    expect(restore).toContain("restore_offline_backup_isolated");
    expect(restore).toContain("confirmedDestinationPath");
  });
});
