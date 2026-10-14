import { describe, expect, it } from "vitest";
import sessions from "../pages/SessionsPage.tsx?raw";
import knowledge from "../pages/KnowledgeBasePageV3.tsx?raw";

describe("session and knowledge list keyboard accessibility", () => {
  it("preserves list actions with keyboard-operable detail buttons", () => {
    expect(sessions).toContain("onViewDetail(item.id)");
    expect(sessions).toContain("查看工作记录：");
    expect(sessions).toContain('aria-label="工作记录列表"');
    expect(knowledge).toContain("onViewDetail(item.id)");
    expect(knowledge).toContain("查看知识：");
    expect(knowledge).toContain("KnowledgeCard item={item}");
  });
  it("keeps page-scoped filtering honest and recoverable", () => {
    for (const page of [sessions, knowledge]) {
      expect(page).toContain("currentPageItems.length");
      expect(page).toContain("data?.items.length");
      expect(page).toContain("可以清空关键词，或切换页面继续查找");
    }
  });
});
