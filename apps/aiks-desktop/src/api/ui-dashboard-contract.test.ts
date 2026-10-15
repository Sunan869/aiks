import { describe, expect, it } from "vitest";
import sources from "../pages/SourcesPage.tsx?raw";
import processing from "../pages/ProcessingPage.tsx?raw";
import combo from "../components/ui/SearchSelect.tsx?raw";
import memory from "../pages/ProjectMemoryPage.tsx?raw";
import sessions from "../pages/SessionsPage.tsx?raw";
import desktopCommands from "../../src-tauri/src/commands.rs?raw";
describe("unified worklist UX", () => {
  it("filters source cards without mutating the provider scan or configuration", () => {
    expect(sources).toContain('aria-label="搜索数据源"');
    expect(sources).toContain("sourceStatusPresentation(source).tone");
    expect(sources).toContain("visibleSources.map(");
    expect(sources).toContain("saveProviderSettings");
  });
  it("supports local pipeline searching while keeping task retries", () => {
    expect(processing).toContain('id="processing-run-search"');
    expect(processing).toContain("setExpandedSession");
    expect(processing).toContain("voidSession(task.session_id)");
    expect(processing).toContain("restoreVoidSession(task.session_id)");
    expect(processing).toContain("handleBackfill");
    expect(processing).toContain('title="处理结果汇总"');
  });
  it("loads work-record sources from persisted sessions including share imports", () => {
    expect(sessions).toContain("getSessionSources()");
    expect(sessions).toContain("await loadSources()");
    expect(sessions).not.toContain("sourceFilterOptions(sources)");
    expect(desktopCommands).toContain("SELECT DISTINCT source FROM source_session");
  });
  it("exposes read-only task preview alongside existing queue actions", () => {
    expect(processing).toContain("onViewSession?.(task.session_id)}>预览");
    expect(processing).toContain("cancelPendingAiTask(task.session_id)");
  });
  it("uses stable combobox popup ids, not label-derived ids", () => {
    expect(combo).toContain("const listId = useId()");
    expect(combo).toContain("aria-controls={open ? listId : undefined}");
    expect(memory).toContain("项目记忆工作区");
  });
});
