import { describe, expect, it } from "vitest";
import settings from "../pages/SettingsPage.tsx?raw";
import knowledge from "../pages/KnowledgeWorkbenchPage.tsx?raw";

describe("settings and knowledge readiness", () => {
  it("keeps labelled settings and visible save controls", () => {
    for (const key of ["embedding_base_url", "embedding_model", "embedding_dimensions", "ai_base_url", "ai_model", "max_tool_result_chars"]) {
      expect(settings).toContain(`htmlFor="aiks-setting-${key}"`);
      expect(settings).toContain(`id="aiks-setting-${key}"`);
    }
    expect(settings).toContain("sticky bottom-0");
    expect(settings).toContain("save(true)");
    expect(settings).toContain("aria-label={label}");
  });
  it("preserves knowledge sorting, page totals and actions", () => {
    expect(knowledge).toContain("getKnowledge(filters)");
    expect(knowledge).toContain("setKnowledgeFavorite");
    expect(knowledge).toContain("Math.ceil(data.total / PAGE_SIZE)");
  });
});
