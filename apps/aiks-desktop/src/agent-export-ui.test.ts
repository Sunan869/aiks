import { describe, expect, it } from "vitest";
import pageSource from "./pages/ProjectMemoryPage.tsx?raw";

describe("S3.3 Agent suggestion export UI boundary", () => {
  it("keeps the new suggestion formatting behind explicit preview and user export", () => {
    expect(pageSource).toContain('renderAgentSuggestion(output, agentFormat)');
    expect(pageSource).toContain('projectExportFileName(');
    expect(pageSource).toContain('navigator.clipboard.writeText(preview)');
    expect(pageSource).toContain('anchor.download = filename');
    expect(pageSource).toContain('onClick={downloadPreview}');
    expect(pageSource).not.toContain('writeFile(');
  });

  it("invalidates drafts after changing every relevant generation parameter", () => {
    expect(pageSource).toContain('const invalidatePreview = () => {');
    expect(pageSource).toContain('generation.current += 1;');
    expect(pageSource).toContain('setPreview("");');
    expect(pageSource).toContain('setPreviewType("");');
    // The extra security reset is intentional; check the ordered sequence.
    const sourceFilterHandler = pageSource.slice(
      pageSource.indexOf('id="project-memory-source"'),
      pageSource.indexOf('id="project-memory-source"') + 500,
    );
    expect(sourceFilterHandler).toContain('invalidatePreview();');
    expect(sourceFilterHandler).toContain('setSelectedAgentKnowledgeIds([]);');
    expect(sourceFilterHandler).toContain('setSourceFilter(event.target.value);');
    expect(sourceFilterHandler.indexOf('invalidatePreview();'))
      .toBeLessThan(sourceFilterHandler.indexOf('setSelectedAgentKnowledgeIds([]);'));
    expect(pageSource).toMatch(/invalidatePreview\(\);\s*setAgentFormat\(/);
    expect(pageSource).toContain('invalidatePreview(); setFrom(');
    expect(pageSource).toContain('invalidatePreview(); setThrough(');
    expect(pageSource).toContain('invalidatePreview(); setTokenBudget(');
    expect(pageSource).toContain('if (requestId !== generation.current) return;');
  });
});
