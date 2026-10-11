import { describe, expect, it } from "vitest";
import { MockAiksApi } from "./api/mock";
import { reportDateRange } from "./project-report-dates";
import page from "./pages/ProjectMemoryPage.tsx?raw";
import core from "../../../crates/aiks-core/src/knowledge/project_report_drafts.rs?raw";
import migration from "../../../crates/aiks-core/migrations/016_project_report_drafts.sql?raw";
import tauri from "./api/tauri.ts?raw";

describe("S3.2 editable local daily/weekly project report revisions", () => {
  it("uses local workdays and week start without UTC boundary shifts", () => {
    const sunday = new Date(2026, 9, 11, 23, 59);
    expect(reportDateRange("today", sunday)).toEqual({
      from: "2026-10-11", through: "2026-10-11",
    });
    expect(reportDateRange("week", sunday)).toEqual({
      from: "2026-10-05", through: "2026-10-11",
    });
    const monday = new Date(2026, 9, 12);
    expect(reportDateRange("week", monday)).toEqual({
      from: "2026-10-12", through: "2026-10-12",
    });
  });

  it("persists edit history and refuses stale revisions or changed project scope", async () => {
    const api = new MockAiksApi();
    const projects = await api.getProjectMemories();
    const project = projects.find(item => item.sources.includes("codex"));
    expect(project).toBeDefined();
    const id = project!.id;
    const first = await api.saveProjectReportDraft(
      null, id, "2026-10-05", "2026-10-11", null, 0, "# Reviewed first draft",
    );
    expect(first.revision).toBe(1);
    const second = await api.saveProjectReportDraft(
      first.draft.id, id, "2026-10-05", "2026-10-11", null, 1, "# User edits v2",
    );
    expect(second.revision).toBe(2);
    expect((await new MockAiksApi().getProjectReportDraft(first.draft.id, 1)).body)
      .toBe("# Reviewed first draft");
    expect((await new MockAiksApi().getProjectReportDraft(first.draft.id)).body)
      .toBe("# User edits v2");
    expect((await api.listProjectReportDrafts(id)).find(
      item => item.id === first.draft.id
    )?.current_revision).toBe(2);
    await expect(api.saveProjectReportDraft(
      first.draft.id, id, "2026-10-05", "2026-10-11", null, 1, "# stale",
    )).rejects.toThrow();
    await expect(api.saveProjectReportDraft(
      first.draft.id, id, "2026-10-05", "2026-10-12", null, 2, "# wrong range",
    )).rejects.toThrow();
    await expect(api.saveProjectReportDraft(
      null, "different-project", "2026-10-05", "2026-10-11", null, 0, "# foreign",
    )).rejects.toThrow();
    expect((await api.getProjectReportDraft(first.draft.id)).revision).toBe(2);
  });

  it("makes report save deliberate, independent of Agent export and SiYuan writes", () => {
    expect(page).toContain('reportDateRange(kind, new Date())');
    expect(page).toContain('getApi().saveProjectReportDraft(');
    expect(page).toContain('getApi().getProjectReportDraft(id, revision)');
    expect(page).toContain('listProjectReportDrafts(selected)');
    expect(page).toContain("载入历史报告会替换当前尚未保存的预览编辑");
    expect(page).toContain('previewType === "review"');
    expect(page).toContain('历史版本没有被覆盖');
    expect(tauri).toContain('invoke("save_project_report_draft"');
    expect(core).toContain("transaction_with_behavior(TransactionBehavior::Immediate)");
    expect(core).toContain("current == expected_revision");
    expect(migration).toContain("CREATE TABLE IF NOT EXISTS project_report_revision");
  });
});
