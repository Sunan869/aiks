import { describe, expect, it } from "vitest";
import nav from "../navigation.ts?raw";
import sidebar from "../components/Sidebar.tsx?raw";
import app from "../App.tsx?raw";
import project from "../pages/ProjectMemoryPage.tsx?raw";
describe("visible human review entry", () => {
  it("keeps review inside project memory without a duplicate sidebar entry", () => {
    expect(nav).not.toContain('{ id: "review", label: "待复核" }');
    expect(nav).toContain('{ id: "projects", label: "项目记忆" }');
    expect(sidebar).toContain("review: ClipboardCheck");
    expect(app).toContain('case "review": return <ProjectMemoryPage initialTab="review"');
    expect(project).toContain('initialTab = "knowledge"');
    expect(project).toContain('useState<"knowledge" | "review" | "evolution" | "tools">(initialTab)');
  });
  it("discloses actual review criteria rather than inventing automated findings", () => {
    expect(project).toContain("目前不会自动依据 AI 置信度创建复核任务");
    expect(project).toContain("buildProjectReviewQueue");
  });
});