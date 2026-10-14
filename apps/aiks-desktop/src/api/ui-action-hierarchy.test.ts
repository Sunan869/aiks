import { describe, expect, it } from "vitest";
import work from "../pages/KnowledgeWorkbenchPage.tsx?raw";
import overview from "../pages/OverviewPage.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import editor from "../components/KnowledgeEditor.tsx?raw";
import legacy from "../pages/KnowledgePage.tsx?raw";

describe("desktop shared action hierarchy", () => {
  it("uses primary actions and exposes selected knowledge tabs", () => {
    expect(work).toContain('variant="primary" size="md"');
    expect(work).toContain('variant={view === key ? "primary" : "ghost"}');
    expect(work).toContain("aria-pressed={view === key}");
    expect(overview).toContain('variant="primary" size="sm" onClick={() => void handleSync()}');
    expect(overview).toContain('disabled={isSyncing}');
    expect(legacy).toContain('variant="primary" size="sm"');
  });
  it("preserves editor controls and accessible settings switches", () => {
    expect(editor).toContain('<Tabs.Root value={preview ? "preview" : "edit"}');
    expect(editor).toContain('setPreview(value === "preview")');
    expect(editor).toContain('Tabs.Trigger value="edit"');
    expect(editor).toContain('Tabs.Trigger value="preview"');
    expect(editor).toContain("createKnowledge(");
    expect(settings).toContain("<Switch.Root checked={value} onCheckedChange={onChange}");
    expect(settings).toContain("<Switch.Thumb");
  });
});
