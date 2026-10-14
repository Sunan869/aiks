import { describe, expect, it } from "vitest";
import processingDetail from "../pages/ProcessingDetailPage.tsx?raw";
import workbench from "../pages/KnowledgeWorkbenchPage.tsx?raw";
import sidebar from "../components/Sidebar.tsx?raw";

describe("desktop keyboard navigability and card actions", () => {
  it("uses an accessible stage disclosure instead of a clickable div", () => {
    expect(processingDetail).toContain("aria-expanded={expanded}");
    expect(processingDetail).toContain("onClick={() => setExpanded(e => !e)}");
    expect(processingDetail).toContain("查看");
  });
  it("separates open-detail from favorite actions", () => {
    expect(workbench).toContain('aria-label={`查看知识：${item.title}`}');
    expect(workbench).toContain("e.stopPropagation()");
    expect(workbench).toContain("md:grid-cols-2");
  });
  it("keeps all navigation links as buttons", () => {
    expect(sidebar).toContain('aria-current={isActive ? "page" : undefined}');
    expect(sidebar).toContain("onNavigate(item.id)");
    expect(sidebar).toContain("min-h-0 flex-1");
  });
});
