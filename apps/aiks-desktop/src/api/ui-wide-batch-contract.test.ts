import { describe, expect, it } from "vitest";
import sources from "../pages/SourcesPage.tsx?raw";
import sessions from "../pages/SessionsPage.tsx?raw";
import settings from "../pages/SettingsPage.tsx?raw";
import combo from "../components/ui/SearchSelect.tsx?raw";
import calendar from "../components/ui/CalendarDateField.tsx?raw";

describe("cross-page UI batch maintains business controls", () => {
  it("keeps data source operations and read-only provider contract", () => {
    expect(sources).toContain("saveProviderSettings");
    expect(sources).toContain("立即同步");
    expect(sources).toContain("关闭来源不会删除已导入记录");
  });
  it("keeps source filtering and AI concurrency controls", () => {
    expect(sessions).toContain('aria-label="工作记录来源"');
    expect(settings).toContain('aria-label="提炼并发数"');
    expect(settings).toContain("ai_max_concurrent");
  });
  it("exposes an identified, keyboard-navigable combobox list", () => {
    expect(combo).toContain("aria-controls={listId}");
    expect(combo).toContain("id={listId}");
    expect(combo).toContain('event.key === "Home"');
    expect(combo).toContain('event.key === "End"');
    expect(combo).toContain("triggerRef.current?.focus()");
  });
  it("keeps dates in ISO form with an accessible month label", () => {
    expect(calendar).toContain("aria-labelledby={yearMonthLabel}");
    expect(calendar).toContain("onChange(iso(day))");
  });
});
