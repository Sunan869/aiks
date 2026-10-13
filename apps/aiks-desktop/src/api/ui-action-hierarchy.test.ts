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
    expect(overview).toContain('variant="primary" size="sm" onClick={handleSync}');
    expect(legacy).toContain('variant="primary" size="sm"');
  });
  it("preserves editor controls and accessible settings switches", () => {
    expect(editor).toContain('variant={!preview ? "secondary" : "ghost"}');
    expect(editor).toContain('variant={preview ? "secondary" : "ghost"}');
    expect(editor).toContain("aria-pressed={!preview}");
    expect(editor).toContain("createKnowledge(");
    expect(settings).toContain('role="switch"');
    expect(settings).toContain("aria-checked={value}");
  });
});
