import { describe, expect, it } from "vitest";
import settings from "../pages/SettingsPage.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
import detail from "../pages/KnowledgeDetailPage.tsx?raw";
import knowledge from "../pages/KnowledgeBasePageV3.tsx?raw";

describe("shared desktop form primitives", () => {
  it("reuses shared inputs in settings, memory, knowledge details and knowledge search", () => {
    for (const page of [settings, memory, detail, knowledge]) expect(page).toContain("<UiInput");
    expect(memory).toContain("<UiTextarea");
    expect(detail).toContain("<UiTextarea");
  });
  it("keeps review and user-consent controls intact", () => {
    expect(memory).toContain('aria-label="Token 预算"');
    expect(detail).toContain('aria-label="关联证据"');
    expect(detail).toContain('aria-label="知识整理草稿"');
    expect(detail).toContain('type="checkbox" checked={organizeConfirmed}');
    expect(settings).toContain("ai_max_concurrent");
    expect(knowledge).toContain('aria-label="搜索当前页知识"');
  });
});
